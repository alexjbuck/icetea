//! CLI argument parsing and definitions

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "icetea")]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Path to configuration file
    #[arg(short, long, value_name = "FILE", env = "ICETEA_CONFIG")]
    pub config: Option<PathBuf>,

    /// Enable verbose logging
    #[arg(short, long)]
    pub verbose: bool,

    /// Catalog connection (repeatable). Format: `name=type:uri` or `uri`
    ///
    /// Examples:
    ///   --catalog my_cat=rest:http://localhost:8181
    ///   --catalog http://localhost:8181
    #[arg(long = "catalog", env = "ICETEA_CATALOG_URIS", value_delimiter = ',')]
    pub catalogs: Vec<String>,

    /// Catalog warehouse (repeatable). Format: `name=warehouse`
    ///
    /// Example: --catalog-warehouse my_cat=s3://bucket/warehouse
    #[arg(long = "catalog-warehouse", env = "ICETEA_CATALOG_WAREHOUSES", value_delimiter = ',')]
    pub catalog_warehouses: Vec<String>,

    /// Catalog property (repeatable). Format: `name.key=value`
    ///
    /// Example: --catalog-property my_cat.credential=id:secret
    #[arg(long = "catalog-property", env = "ICETEA_CATALOG_PROPERTIES", value_delimiter = ',')]
    pub catalog_properties: Vec<String>,

    /// UI theme (`dark` or `light`)
    #[arg(long)]
    pub theme: Option<String>,

    /// Catalog/table refresh interval in seconds
    #[arg(long)]
    pub refresh_interval: Option<u64>,

    /// Default query timeout in seconds
    #[arg(long = "query-timeout")]
    pub query_timeout: Option<u64>,

    /// Maximum rows to fetch for queries
    #[arg(long)]
    pub max_rows: Option<usize>,

    /// Subcommand to execute (if any)
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// List available catalogs and tables
    List {
        /// Catalog name to list from
        catalog: Option<String>,
    },
    /// Execute a SQL query
    Query {
        /// SQL query to execute
        query: String,
        /// Output format (table, json, csv)
        #[arg(short, long, default_value = "table")]
        format: String,
    },
}

impl Cli {
    pub fn parse_args() -> Self {
        Self::parse()
    }
}
