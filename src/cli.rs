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

    /// Catalog connection URI (can be specified multiple times)
    #[arg(long = "catalog", env = "ICETEA_CATALOGS")]
    pub catalogs: Vec<String>,

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
