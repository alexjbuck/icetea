# IceTea

A Terminal User Interface (TUI) for interacting directly with Apache Iceberg catalogs.

## Features

### MVP (Current Development)
- ✅ Multi-catalog configuration support (env vars, config file, CLI args, in-app)
- ✅ TUI scaffolding with ratatui
- ✅ CLI interface with clap
- ✅ Configuration management with figment
- ⏳ Connect to multiple catalogs simultaneously
- ⏳ Read all table metadata
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
1. In-application configuration
2. Command-line arguments
3. Environment variables
4. Configuration file
5. Defaults

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

```bash
export ICETEA_CONFIG=/path/to/icetea.toml
export ICETEA_CATALOGS="rest:http://localhost:8181"
```

### Command-Line Arguments

```bash
# Start TUI with catalog
icetea --catalog "my_catalog=rest:http://localhost:8181"

# Multiple catalogs
icetea --catalog "cat1=rest:http://host1:8181" --catalog "cat2=rest:http://host2:8181"

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
- `q` - Quit application
- `:` - Enter query mode
- `?` - Show help
- `ESC` - Return to browser
- `Ctrl+C` - Force quit

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

**Note**: Now using iceberg-rust v0.7 with proper REST catalog integration. The `RestCatalogBuilder` pattern is used to create catalog connections with full configuration support.

**DataFusion Integration**: Implemented a custom `TableProvider` that bridges Iceberg tables to DataFusion. The implementation includes:
- Full Iceberg-to-Arrow schema conversion supporting all primitive types, structs, lists, and maps
- `IcebergTableProvider` implementing DataFusion's `TableProvider` trait
- `IcebergScanExec` execution plan for reading Iceberg data (scan implementation pending)
- Registered tables can be queried via SQL through DataFusion's query engine

**Note**: The `datafusion_iceberg` v0.7 crate is incompatible with `iceberg` v0.7 (built against the older `iceberg_rust` crate), so we implemented our own custom integration.

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
