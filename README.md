# IceTea

A Terminal User Interface (TUI) for interacting directly with Apache Iceberg catalogs.

## Features

### MVP (Current Development)
- ✅ Multi-catalog configuration support (env vars, config file, CLI args, in-app)
- ✅ TUI scaffolding with ratatui
- ✅ CLI interface with clap
- ✅ Configuration management with figment
- ✅ Connect to multiple catalogs simultaneously
- ✅ Browse catalogs, namespaces, and tables in tree view
- ✅ View table metadata (schema, partitioning, sort order, properties)
- ⏳ View snapshot history as chain/log/tree
- ⏳ Generate list of files in tables/partitions
- ⏳ View list of partitions
- ⏳ Execute SQL queries via DataFusion

### V2 (Planned)
- Write support for table metadata
- Partition/sort order evolution
- Schema evolution
- Table property updates
- Create/drop branches and tags

## Tech Stack

- **CLI**: clap for argument parsing
- **TUI**: ratatui for terminal UI
- **Iceberg**: iceberg-rust for catalog integration
- **SQL**: DataFusion for query execution
- **Config**: figment for layered configuration
- **Testing**: proptest for property-based testing

## Installation

### Prerequisites

- Rust 1.75+ (uses 2024 edition)
- Access to Apache Iceberg catalog(s)

### Build from Source

```bash
git clone <repository-url>
cd icetea
cargo build --release
```

The binary will be at `target/release/icetea`.

## Configuration

IceTea supports multiple configuration methods with the following precedence (highest to lowest):
1. Command-line arguments
2. Environment variables
3. Configuration file
4. Defaults

Every setting in the config file can also be set via env vars or CLI flags.

### Configuration File

Create a `icetea.toml` file:

```toml
[catalogs.my_rest_catalog]
catalog_type = "rest"
uri = "http://localhost:8181"
warehouse = "s3://my-bucket/warehouse"

[catalogs.my_rest_catalog.properties]
# Additional catalog properties
token = "my-auth-token"

[ui]
theme = "dark"  # or "light"
refresh_interval = 30  # seconds

[query]
timeout = 300  # seconds
max_rows = 10000
```

### Environment Variables

Nested keys use `__` as a separator (`ICETEA_` prefix):

```bash
export ICETEA_CONFIG=/path/to/icetea.toml

# UI / query
export ICETEA_UI__THEME=dark
export ICETEA_UI__REFRESH_INTERVAL=30
export ICETEA_QUERY__TIMEOUT=300
export ICETEA_QUERY__MAX_ROWS=10000

# Full nested catalog config
export ICETEA_CATALOGS__MY_REST_CATALOG__CATALOG_TYPE=rest
export ICETEA_CATALOGS__MY_REST_CATALOG__URI=http://localhost:8181
export ICETEA_CATALOGS__MY_REST_CATALOG__WAREHOUSE=s3://my-bucket/warehouse
export ICETEA_CATALOGS__MY_REST_CATALOG__PROPERTIES__TOKEN=my-auth-token

# Convenience CLI-style encodings (comma-separated, same format as flags)
export ICETEA_CATALOG_URIS="my_rest_catalog=rest:http://localhost:8181"
export ICETEA_CATALOG_WAREHOUSES="my_rest_catalog=s3://my-bucket/warehouse"
export ICETEA_CATALOG_PROPERTIES="my_rest_catalog.token=my-auth-token"
```

### Command-Line Arguments

```bash
# Start TUI with a fully specified catalog
icetea \
  --catalog "my_catalog=rest:http://localhost:8181" \
  --catalog-warehouse "my_catalog=s3://my-bucket/warehouse" \
  --catalog-property "my_catalog.token=my-auth-token" \
  --catalog-property "my_catalog.credential=id:secret"

# Multiple catalogs
icetea \
  --catalog "cat1=rest:http://host1:8181" \
  --catalog "cat2=rest:http://host2:8181"

# UI / query settings
icetea --theme light --refresh-interval 60 --query-timeout 120 --max-rows 500

# Use config file
icetea --config /path/to/icetea.toml

# Execute query from CLI
icetea query "SELECT * FROM catalog.namespace.table LIMIT 10" --format table

# List catalogs and tables
icetea list
icetea list my_catalog
```

## Usage

### TUI Mode

Start the TUI:

```bash
icetea
```

**Keyboard Shortcuts:**
- `↑`/`↓` or `j`/`k` - Navigate tree
- `←`/`→` or `h`/`l` - Collapse/expand nodes
- `Enter` - Toggle expand/collapse
- `q` - Quit application
- `:` - Enter query mode
- `?` - Show help
- `ESC` - Return to browser
- `Ctrl+C` - Force quit

**Table Detail View:**
When you select a table in the tree, the detail panel displays:
- **Schema** - All fields with ID, name, type, and required status
- **Partition Spec** - Partition columns with their transforms
- **Sort Order** - Sort columns with direction and null ordering
- **Properties** - Table configuration properties
- **Snapshot Info** - Current snapshot ID and total count

### CLI Mode

```bash
# List catalogs
icetea list

# List tables in a catalog
icetea list my_catalog

# Execute SQL query
icetea query "SELECT * FROM my_catalog.my_db.my_table LIMIT 10"

# Query with output format
icetea query "SELECT * FROM table" --format json
icetea query "SELECT * FROM table" --format csv
```

## Project Structure

```
src/
├── main.rs              # Entry point and event loop
├── app.rs               # Application state
├── cli.rs               # CLI argument definitions
├── config.rs            # Configuration management
├── ui/                  # UI components
│   ├── mod.rs
│   ├── catalog_tree.rs  # Catalog browser tree
│   ├── detail_view.rs   # Table detail panels
│   └── query_input.rs   # SQL query interface
└── iceberg/             # Iceberg integration
    ├── mod.rs
    ├── catalog.rs       # Catalog management
    ├── metadata.rs      # Metadata operations
    └── query.rs         # DataFusion query execution
```

## Development Status

This is a greenfield project in active development. The current implementation provides:

- ✅ Complete project scaffolding
- ✅ TUI framework setup (ratatui 0.30)
- ✅ Configuration management
- ✅ CLI interface
- ✅ Upgraded to iceberg-rust 0.7 + datafusion 45
- ✅ REST catalog connection via RestCatalogBuilder
- ✅ Custom DataFusion TableProvider for Iceberg tables
- ✅ Tree-based catalog/namespace/table browsing
- ✅ Table metadata display (schema, partitioning, sort order, properties)

**Note**: Now using iceberg-rust v0.7 with proper REST catalog integration. The `RestCatalogBuilder` pattern is used to create catalog connections with full configuration support.

**DataFusion Integration**: Implemented a complete catalog-level integration with DataFusion using custom providers:

Architecture:
- `IcebergCatalogProvider` - Wraps Iceberg catalogs as DataFusion catalog providers
- `IcebergSchemaProvider` - Maps Iceberg namespaces to DataFusion schemas
- `IcebergTableProvider` - Bridges individual Iceberg tables to DataFusion
- Full Iceberg-to-Arrow schema conversion supporting all types (primitives, structs, lists, maps)

This design leverages DataFusion's native catalog abstraction layer. After registering a catalog, tables can be queried using SQL:
```sql
SELECT * FROM catalog.namespace.table_name
```

DataFusion automatically discovers namespaces and tables through the provider interfaces, eliminating the need to manually register individual tables.

**Note**: The `datafusion_iceberg` v0.7 crate is incompatible with `iceberg` v0.7 (built against the older `iceberg_rust` crate), so we implemented our own custom integration following DataFusion's catalog provider pattern.

## Contributing

Contributions are welcome! Areas that need work:

1. **Iceberg Integration**: Completing catalog connection and table operations
2. **DataFusion Integration**: Implementing TableProvider for Iceberg tables
3. **UI Enhancements**: Improving the TUI with better navigation and views
4. **Testing**: Adding unit tests and property tests
5. **Documentation**: Expanding examples and use cases

## License

MIT License - see LICENSE file for details.

## Acknowledgments

- Apache Iceberg team for the excellent table format
- ratatui team for the amazing TUI framework
- iceberg-rust contributors for Rust bindings
