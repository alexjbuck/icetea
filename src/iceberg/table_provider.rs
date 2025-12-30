//! Custom DataFusion TableProvider for Iceberg tables

use anyhow::Result;
use async_trait::async_trait;
use datafusion::arrow::datatypes::{DataType, Field, Schema as ArrowSchema, SchemaRef};
use datafusion::catalog::Session;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::Result as DataFusionResult;
use datafusion::execution::{SendableRecordBatchStream, TaskContext};
use datafusion::logical_expr::{Expr, TableProviderFilterPushDown};
use datafusion::physical_plan::{DisplayAs, DisplayFormatType, ExecutionPlan, PlanProperties};
use datafusion::physical_plan::execution_plan::{Boundedness, EmissionType};
use datafusion::physical_plan::stream::RecordBatchStreamAdapter;
use datafusion::physical_plan::metrics::ExecutionPlanMetricsSet;
use datafusion::physical_expr::{EquivalenceProperties, Partitioning};
use iceberg::table::Table as IcebergTable;
use iceberg::spec::{PrimitiveType, Type as IcebergType};
use std::any::Any;
use std::fmt;
use std::sync::Arc;
use futures::stream;

/// TableProvider implementation for Iceberg tables
#[derive(Debug)]
pub struct IcebergTableProvider {
    table: Arc<IcebergTable>,
    schema: SchemaRef,
}

impl IcebergTableProvider {
    /// Create a new Iceberg table provider
    pub fn new(table: Arc<IcebergTable>) -> Result<Self> {
        let schema = Self::convert_schema(&table)?;
        Ok(Self { table, schema })
    }

    /// Convert Iceberg schema to Arrow schema
    fn convert_schema(table: &IcebergTable) -> Result<SchemaRef> {
        let metadata = table.metadata();
        let iceberg_schema = metadata.current_schema();

        let mut fields = Vec::new();

        // Convert each field in the Iceberg schema to an Arrow field
        for field in iceberg_schema.as_struct().fields() {
            let arrow_field = Self::convert_field(field)?;
            fields.push(arrow_field);
        }

        Ok(Arc::new(ArrowSchema::new(fields)))
    }

    /// Convert a single Iceberg field to an Arrow field
    fn convert_field(field: &iceberg::spec::NestedField) -> Result<Field> {
        let data_type = Self::convert_type(&field.field_type)?;
        Ok(Field::new(
            &field.name,
            data_type,
            !field.required,
        ))
    }

    /// Convert Iceberg type to Arrow DataType
    fn convert_type(iceberg_type: &IcebergType) -> Result<DataType> {
        match iceberg_type {
            IcebergType::Primitive(prim) => match prim {
                PrimitiveType::Boolean => Ok(DataType::Boolean),
                PrimitiveType::Int => Ok(DataType::Int32),
                PrimitiveType::Long => Ok(DataType::Int64),
                PrimitiveType::Float => Ok(DataType::Float32),
                PrimitiveType::Double => Ok(DataType::Float64),
                PrimitiveType::Date => Ok(DataType::Date32),
                PrimitiveType::Time => Ok(DataType::Time64(datafusion::arrow::datatypes::TimeUnit::Microsecond)),
                PrimitiveType::Timestamp => Ok(DataType::Timestamp(
                    datafusion::arrow::datatypes::TimeUnit::Microsecond,
                    None,
                )),
                PrimitiveType::Timestamptz => Ok(DataType::Timestamp(
                    datafusion::arrow::datatypes::TimeUnit::Microsecond,
                    Some("UTC".into()),
                )),
                PrimitiveType::TimestampNs => Ok(DataType::Timestamp(
                    datafusion::arrow::datatypes::TimeUnit::Nanosecond,
                    None,
                )),
                PrimitiveType::TimestamptzNs => Ok(DataType::Timestamp(
                    datafusion::arrow::datatypes::TimeUnit::Nanosecond,
                    Some("UTC".into()),
                )),
                PrimitiveType::String => Ok(DataType::Utf8),
                PrimitiveType::Uuid => Ok(DataType::Binary),
                PrimitiveType::Fixed(_size) => Ok(DataType::Binary),
                PrimitiveType::Binary => Ok(DataType::Binary),
                PrimitiveType::Decimal { precision, scale } => {
                    Ok(DataType::Decimal128(*precision as u8, *scale as i8))
                }
            },
            IcebergType::Struct(struct_type) => {
                let mut fields = Vec::new();
                for field in struct_type.fields() {
                    fields.push(Self::convert_field(field)?);
                }
                Ok(DataType::Struct(fields.into()))
            }
            IcebergType::List(list_type) => {
                let element_field = Self::convert_field(&list_type.element_field)?;
                Ok(DataType::List(Arc::new(element_field)))
            }
            IcebergType::Map(map_type) => {
                let key_field = Self::convert_field(&map_type.key_field)?;
                let value_field = Self::convert_field(&map_type.value_field)?;
                Ok(DataType::Map(
                    Arc::new(Field::new(
                        "entries",
                        DataType::Struct(vec![key_field, value_field].into()),
                        false,
                    )),
                    false,
                ))
            }
        }
    }
}

#[async_trait]
impl TableProvider for IcebergTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn table_type(&self) -> TableType {
        TableType::Base
    }

    async fn scan(
        &self,
        _state: &dyn Session,
        projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        _limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        let projected_schema = if let Some(projection) = projection {
            let projected_fields: Vec<Field> = projection
                .iter()
                .map(|i| self.schema.field(*i).clone())
                .collect();
            Arc::new(ArrowSchema::new(projected_fields))
        } else {
            self.schema.clone()
        };

        Ok(Arc::new(IcebergScanExec::new(
            self.table.clone(),
            projected_schema,
        )))
    }

    fn supports_filters_pushdown(
        &self,
        filters: &[&Expr],
    ) -> DataFusionResult<Vec<TableProviderFilterPushDown>> {
        // For now, we don't push down filters to Iceberg
        // This could be enhanced to use Iceberg's predicate pushdown
        Ok(vec![TableProviderFilterPushDown::Unsupported; filters.len()])
    }
}

/// Physical execution plan for scanning Iceberg tables
#[derive(Debug)]
struct IcebergScanExec {
    table: Arc<IcebergTable>,
    projected_schema: SchemaRef,
    metrics: ExecutionPlanMetricsSet,
    properties: PlanProperties,
}

impl IcebergScanExec {
    fn new(table: Arc<IcebergTable>, projected_schema: SchemaRef) -> Self {
        // Create plan properties for a single partition, bounded execution
        let properties = PlanProperties::new(
            EquivalenceProperties::new(projected_schema.clone()),
            Partitioning::UnknownPartitioning(1),
            EmissionType::Incremental,
            Boundedness::Bounded,
        );

        Self {
            table,
            projected_schema,
            metrics: ExecutionPlanMetricsSet::new(),
            properties,
        }
    }
}

impl DisplayAs for IcebergScanExec {
    fn fmt_as(&self, _t: DisplayFormatType, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "IcebergScanExec: table={}", self.table.identifier())
    }
}

impl ExecutionPlan for IcebergScanExec {
    fn name(&self) -> &str {
        "IcebergScanExec"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        self.projected_schema.clone()
    }

    fn properties(&self) -> &datafusion::physical_plan::PlanProperties {
        &self.properties
    }

    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
        vec![]
    }

    fn with_new_children(
        self: Arc<Self>,
        _children: Vec<Arc<dyn ExecutionPlan>>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        Ok(self)
    }

    fn execute(
        &self,
        _partition: usize,
        _context: Arc<TaskContext>,
    ) -> DataFusionResult<SendableRecordBatchStream> {
        // TODO: Implement actual data reading from Iceberg
        // For now, return an empty stream
        let schema = self.schema();
        let stream = stream::empty();

        Ok(Box::pin(RecordBatchStreamAdapter::new(
            schema,
            stream,
        )))
    }

    fn metrics(&self) -> Option<datafusion::physical_plan::metrics::MetricsSet> {
        Some(self.metrics.clone_inner())
    }

    fn statistics(&self) -> DataFusionResult<datafusion::common::Statistics> {
        // TODO: Get statistics from Iceberg metadata
        Ok(datafusion::common::Statistics::new_unknown(&self.schema()))
    }
}
