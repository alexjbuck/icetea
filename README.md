# IceTea

A terminal UI for poking around Apache Iceberg catalogs. Connect to one or more REST
catalogs, browse namespaces and tables, and inspect table metadata without leaving the
terminal.

## Status

IceTea is early and under active development. Here's where things actually stand.

Working today:

- Connect to multiple REST catalogs at once (from a config file, env vars, CLI flags, or a
  mix of all three)
- Browse catalogs, namespaces, and tables in a tree
- Inspect table metadata: schema, partition spec, sort order, properties, and current
  snapshot info
- A DataFusion-backed catalog provider that maps Iceberg catalogs/namespaces/tables onto
  DataFusion's catalog abstraction (see [DataFusion integration](#datafusion-integration))

Still in progress:

- `icetea list` only prints the configured catalog names right now — it does not yet list
  namespaces or tables
- `icetea query` parses its arguments but does not execute anything yet
- Snapshot history view, file/partition listing

Planned for later:

- Write support (schema/partition/sort-order evolution, property updates)
- Branch and tag management
- Glue/Hive catalog support

## How it's built

- **clap** for the CLI
- **ratatui** + **crossterm** for the terminal UI
- **iceberg-rust** (`iceberg`, `iceberg-catalog-rest`, `iceberg-storage-opendal`) for catalog
  access
- **DataFusion** for query execution (once it's wired up)
- **figment** for layered configuration
- **proptest** for property tests

Exact versions live in `Cargo.toml`.

## Installation

You'll need a recent Rust toolchain (edition 2024, so Rust 1.94 or newer) and access to at
least one Iceberg REST catalog.

Build from source:

```bash
git clone <repository-url>
cd icetea
cargo build --release
```

The binary lands at `target/release/icetea`.

## Configuration

Configuration comes from four places. When the same setting shows up in more than one, the
first one listed wins:

1. Command-line arguments
2. Environment variables
3. Configuration file
4. Built-in defaults

Anything you can put in the config file can also be set through an env var or a CLI flag.

### Config file

Create an `icetea.toml`:

```toml
[catalogs.my_rest_catalog]
catalog_type = "rest"
uri = "http://localhost:8181"
warehouse = "s3://my-bucket/warehouse"

[catalogs.my_rest_catalog.properties]
# Anything the catalog needs, e.g. an auth token
token = "my-auth-token"

[ui]
theme = "dark"  # or "light"
refresh_interval = 30  # seconds

[query]
timeout = 300  # seconds
max_rows = 10000
```

The `[catalogs]` settings are what drive the app today. The `[ui]` and `[query]` values
(`theme`, `refresh_interval`, `timeout`, `max_rows`) are accepted and validated but not wired
into anything yet, so setting them currently has no visible effect. They're documented here so
the config shape is stable as those features land.

### Environment variables

Everything is prefixed with `ICETEA_`, and nested keys use `__` as the separator:

```bash
export ICETEA_CONFIG=/path/to/icetea.toml

# UI and query settings
export ICETEA_UI__THEME=dark
export ICETEA_UI__REFRESH_INTERVAL=30
export ICETEA_QUERY__TIMEOUT=300
export ICETEA_QUERY__MAX_ROWS=10000

# A fully specified catalog
export ICETEA_CATALOGS__MY_REST_CATALOG__CATALOG_TYPE=rest
export ICETEA_CATALOGS__MY_REST_CATALOG__URI=http://localhost:8181
export ICETEA_CATALOGS__MY_REST_CATALOG__WAREHOUSE=s3://my-bucket/warehouse
export ICETEA_CATALOGS__MY_REST_CATALOG__PROPERTIES__TOKEN=my-auth-token
```

If you'd rather not spell out the full nested form, there are comma-separated shorthands that
match the CLI flags:

```bash
export ICETEA_CATALOG_URIS="my_rest_catalog=rest:http://localhost:8181"
export ICETEA_CATALOG_WAREHOUSES="my_rest_catalog=s3://my-bucket/warehouse"
export ICETEA_CATALOG_PROPERTIES="my_rest_catalog.token=my-auth-token"
```

### Command-line flags

```bash
# Start the TUI with one fully specified catalog
icetea \
  --catalog "my_catalog=rest:http://localhost:8181" \
  --catalog-warehouse "my_catalog=s3://my-bucket/warehouse" \
  --catalog-property "my_catalog.token=my-auth-token" \
  --catalog-property "my_catalog.credential=id:secret"

# Several catalogs at once (--catalog is repeatable)
icetea \
  --catalog "cat1=rest:http://host1:8181" \
  --catalog "cat2=rest:http://host2:8181"

# UI and query settings
icetea --theme light --refresh-interval 60 --query-timeout 120 --max-rows 500

# Point at a config file
icetea --config /path/to/icetea.toml
```

Other useful flags:

- `-v`, `--verbose` — turn on verbose logging
- `--log-file <FILE>` — where to write logs (defaults to
  `$XDG_STATE_HOME/icetea/icetea.log`)

## Usage

### TUI

Run `icetea` with no subcommand to open the browser.

Keys:

- `↑`/`↓` or `j`/`k` — move up and down the tree
- `←`/`→` or `h`/`l` — collapse/expand a node
- `Enter` — toggle expand/collapse
- `:` — query mode
- `?` — help
- `Esc` — back to the browser
- `q` — quit
- `Ctrl+C` — force quit

Select a table and the detail panel shows its schema (field ID, name, type, required flag),
partition spec, sort order, properties, and current snapshot ID plus total snapshot count.

### CLI subcommands

Two subcommands exist, but both are still partial (see [Status](#status)):

```bash
# Prints the configured catalog names. Namespace/table listing is not done yet.
icetea list
icetea list my_catalog

# Accepts a query and format, but execution isn't wired up yet.
icetea query "SELECT * FROM my_catalog.my_db.my_table LIMIT 10"
icetea query "SELECT * FROM table" --format json
```

## Project layout

```
src/
├── main.rs              # Entry point and event loop
├── app.rs               # Application state
├── cli.rs               # CLI argument definitions
├── config.rs            # Configuration loading
├── ui/                  # UI components
│   ├── mod.rs
│   ├── catalog_tree.rs  # Catalog browser tree
│   ├── detail_view.rs   # Table detail panels
│   └── query_input.rs   # SQL query interface
└── iceberg/             # Iceberg integration
    ├── mod.rs
    ├── catalog.rs       # Catalog management
    ├── metadata.rs      # Metadata extraction
    └── query.rs         # DataFusion query execution
```

## DataFusion integration

Rather than registering tables one at a time, IceTea plugs Iceberg into DataFusion's native
catalog layer with a small set of providers:

- `IcebergCatalogProvider` wraps an Iceberg catalog as a DataFusion catalog
- `IcebergSchemaProvider` maps Iceberg namespaces to DataFusion schemas
- `IcebergTableProvider` bridges a single Iceberg table
- Schema conversion covers the full Iceberg type system: primitives, structs, lists, and maps

Once a catalog is registered, DataFusion discovers its namespaces and tables on its own, so a
query is just:

```sql
SELECT * FROM catalog.namespace.table_name
```

We rolled our own integration because the `datafusion_iceberg` crate targets the older
`iceberg_rust` crate and doesn't line up with the `iceberg` version we depend on.

## Releasing

Version bumps and changelog entries come from [knope](https://knope.tech) changesets, not from
conventional commits.

Install the tooling with [mise](https://mise.jdx.dev) (it pulls in `cargo:knope`):

```bash
mise install
```

When a PR includes a user-facing change, record it with a changeset and commit the generated
file alongside the PR:

```bash
knope document-change   # writes a Markdown file under .changeset/
```

To preview what a release would do:

```bash
knope release --dry-run
```

To actually cut a release, run the **Release** workflow in GitHub Actions
(`workflow_dispatch`). It uses the built-in `GITHUB_TOKEN` with `contents: write` to push the
release commit and tag and create the GitHub Release, so no PAT secret is needed. You can also
run it locally with a token that can write contents:

```bash
GITHUB_TOKEN=... knope release
```

Either way, knope consumes the pending changesets, bumps `Cargo.toml` and `Cargo.lock`, updates
`CHANGELOG.md`, and publishes the release. Publishing to crates.io isn't set up yet.

## Contributing

Contributions are welcome. The areas that need the most help right now:

- Finishing catalog operations (namespace/table listing behind `icetea list`)
- Wiring up query execution through DataFusion
- Filling out the TUI: snapshot history, file and partition views
- Tests, especially around metadata extraction and config parsing

## License

MIT. See the [LICENSE](LICENSE) file.

## Acknowledgments

Built on the Apache Iceberg project, ratatui, and iceberg-rust. Thanks to everyone who
maintains them.
