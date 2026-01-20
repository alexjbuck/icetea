# IceTea - Apache Iceberg TUI Development Guide

## Project Overview

IceTea is a terminal user interface (TUI) for exploring Apache Iceberg catalogs, tables, and metadata. Built with Rust using ratatui for the UI and iceberg-rust for catalog operations.

**Core Philosophy:**
- **Responsive UI**: Never block the UI thread - all I/O must be async
- **Pure rendering**: UI functions only display state, never mutate it
- **Verified connections**: Only show catalogs that are actually reachable
- **Type-safe metadata**: Leverage Rust's type system for correctness

## Architecture

### Component Structure

```
src/
├── main.rs              # Entry point, TUI lifecycle management
├── app.rs               # Application state container - SINGLE SOURCE OF TRUTH
├── config.rs            # Layered configuration (CLI > env > config file)
├── cli.rs               # CLI argument parsing
├── iceberg/
│   ├── catalog.rs       # Catalog lifecycle, REST API orchestration
│   ├── metadata.rs      # Metadata extraction - MUST handle nested types
│   ├── catalog_provider.rs  # DataFusion integration
│   ├── table_provider.rs    # DataFusion table provider
│   └── query.rs         # Query execution (future)
└── ui/
    ├── mod.rs           # UI dispatcher - routes to specialized renderers
    ├── catalog_tree.rs  # Tree navigation pane
    ├── detail_view.rs   # Metadata display pane
    └── query_input.rs   # Query mode (future)
```

### State Management Principles

**CRITICAL: All mutable state lives in `App` struct** (`app.rs`)

✅ **CORRECT - State in App:**
```rust
impl App {
    pub async fn load_table_metadata(&mut self) {
        // Mutates self.selected_table_metadata
        self.selected_table_metadata = Some(metadata);
    }
}
```

❌ **WRONG - State in UI:**
```rust
fn render_detail(app: &mut App) {
    // NEVER mutate app during render!
    app.selected_table_metadata = Some(...);  // NO!
}
```

**State ownership:**
- `config`: User configuration - immutable after load
- `catalog_manager`: Catalog connections - mutated on connect/disconnect only
- `tree_items`: Display state - rebuilt on expansion changes
- `selected_table_metadata`: Cached data - loaded on selection change
- `view_state`: Navigation state - changed by keyboard handlers

### Async Architecture Best Practices

**Rule: UI must remain responsive at all times**

✅ **CORRECT - Async with feedback:**
```rust
pub async fn load_selected_table_metadata(&mut self) -> bool {
    self.loading = true;  // Show spinner immediately
    self.status_message = Some("Loading...".to_string());

    match self.catalog_manager.load_table(...).await {
        Ok(table) => {
            self.selected_table_metadata = Some(extract(table));
            self.loading = false;
            true
        }
        Err(e) => {
            self.status_message = Some(format!("Error: {}", e));
            self.loading = false;
            false
        }
    }
}
```

❌ **WRONG - Blocking UI:**
```rust
pub fn load_metadata(&mut self) {
    // This blocks the entire UI thread!
    let table = block_on(catalog.load_table(...));  // NO!
}
```

## Code Style and Standards

### Error Handling - REQUIRED PATTERNS

**Always use `anyhow::Result` for fallible functions:**

✅ **CORRECT:**
```rust
pub async fn connect(&mut self, name: String, config: &Config) -> Result<()> {
    let catalog = builder
        .load(name, props)
        .await
        .context("Failed to load REST catalog")?;  // ALWAYS add context
    Ok(())
}
```

❌ **WRONG:**
```rust
pub async fn connect(&mut self, name: String) -> Result<()> {
    let catalog = builder.load(name, props).await?;  // Missing context!
    Ok(())
}
```

**Error display hierarchy:**
1. **User-facing errors** → Status bar message (brief, actionable)
2. **Developer errors** → Log with `tracing::warn!` or `tracing::error!` (detailed)
3. **Never panic** in user-facing code (except truly impossible states)

### Logging Standards

**Log to file ONLY** - never to stderr (interferes with TUI)

Configuration in `main.rs`:
```rust
// Logs write to icetea.log in current directory
let log_file = std::fs::OpenOptions::new()
    .create(true)
    .append(true)
    .open("icetea.log")?;

tracing_subscriber::fmt()
    .with_writer(std::sync::Mutex::new(log_file))
    .with_ansi(false)  // No color codes in log file
    .init();
```

**Logging level guidelines:**

| Level | Use Case | Example |
|-------|----------|---------|
| `debug!` | Internal flow, API calls | `"Fetching catalog config from {}"` |
| `info!` | User-visible state changes | `"Fetched catalog config: {} properties"` |
| `warn!` | Recoverable errors | `"Failed to fetch config: {}"` |
| `error!` | Critical failures | `"OAuth2 token refresh failed"` |

✅ **CORRECT - Informative logging:**
```rust
tracing::debug!("Fetching table storage config from: {}", url);
match response.status() {
    s if s.is_success() => {
        tracing::debug!("Got {} storage properties", config.len());
        for (k, v) in &config {
            tracing::debug!("  {}: {}", k, v);
        }
    }
    s => {
        tracing::warn!("Failed to fetch: {} - {}", s, body);
    }
}
```

### UI Rendering - STRICT RULES

**RULE 1: Render functions are PURE - no side effects**

✅ **CORRECT - Pure rendering:**
```rust
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let text = format!("Table: {}", app.selected_table_metadata.name);
    let paragraph = Paragraph::new(text);
    frame.render_widget(paragraph, area);
}
```

❌ **WRONG - Side effects in render:**
```rust
pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    if app.selected_table_metadata.is_none() {
        app.load_table_metadata();  // NO! Side effect during render
    }
}
```

**RULE 2: Use consistent colors**

Color meanings are FIXED - do not deviate:

```rust
// Labels and field names
Style::default().fg(Color::Yellow)

// Types and primary values
Style::default().fg(Color::Cyan)

// Success indicators (✓, "Connected")
Style::default().fg(Color::Green)

// Error indicators (✗, "Failed")
Style::default().fg(Color::Red)

// Secondary/hint text
Style::default().fg(Color::DarkGray)

// Section headers
Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
```

**RULE 3: Always truncate unbounded strings**

✅ **CORRECT:**
```rust
Span::styled(truncate_string(&field.name, 22), Style::default().fg(Color::White))
```

❌ **WRONG:**
```rust
Span::styled(field.name.clone(), Style::default().fg(Color::White))  // Can overflow!
```

### Tree Management Pattern

**The tree is flattened and rebuilt on changes - NEVER mutate tree_items directly**

✅ **CORRECT - Signal rebuild:**
```rust
pub fn toggle_selected(&mut self) -> bool {
    if let Some(item) = self.tree_items.get(self.selected_index) {
        if self.expanded.contains(&item.key) {
            self.expanded.remove(&item.key);
        } else {
            self.expanded.insert(item.key.clone());
        }
        return true;  // Tells caller to rebuild tree
    }
    false
}

// In main loop:
let needs_rebuild = app.handle_key_event(key);
if needs_rebuild {
    app.rebuild_tree().await;  // Async rebuild
}
```

❌ **WRONG - Direct tree mutation:**
```rust
pub fn toggle_selected(&mut self) {
    // Manually inserting/removing from tree_items
    self.tree_items.push(...);  // NO! Tree is rebuilt from scratch
}
```

**Tree key format is hierarchical:**
- Catalog: `"catalog_name"`
- Namespace: `"catalog_name/namespace_name"`
- Table: `"catalog_name/namespace_name/table_name"`

## Critical Domain Knowledge

### Iceberg Field IDs - MUST UNDERSTAND

**RULE: Every field needs a unique ID, including nested fields**

This is a common source of errors when working with iceberg-rust serialization.

✅ **CORRECT - All fields have IDs:**
```python
pa.schema([
    pa.field("data_stream_id", pa.string()).with_metadata({"PARQUET:field_id": "1"}),
    pa.field("vector", pa.list_(
        pa.field("element", pa.float32()).with_metadata({"PARQUET:field_id": "9"})
    )).with_metadata({"PARQUET:field_id": "3"}),
    pa.field("location", pa.struct_([
        pa.field("lat", pa.float64()).with_metadata({"PARQUET:field_id": "11"}),
        pa.field("lon", pa.float64()).with_metadata({"PARQUET:field_id": "12"}),
    ])).with_metadata({"PARQUET:field_id": "10"}),
])
```

**Field ID extraction MUST be recursive:**

```rust
fn extract_field_info(field: &NestedFieldRef) -> FieldInfo {
    let nested_fields = match field.field_type.as_ref() {
        Type::List(list_type) => {
            // MUST extract element field ID
            vec![FieldInfo {
                id: list_type.element_field.id,  // Required!
                name: "element".to_string(),
                ...
            }]
        }
        Type::Struct(struct_type) => {
            // MUST recurse into struct fields
            struct_type.fields()
                .iter()
                .map(|f| extract_field_info(f))
                .collect()
        }
        ...
    };
}
```

### REST Catalog Connection Verification

**RULE: Never trust lazy connections - always verify**

Iceberg REST catalog connections are lazy and don't fail until used.

✅ **CORRECT - Verified connection:**
```rust
// Step 1: Create catalog (doesn't connect yet)
catalog_manager.connect(name, config).await?;

// Step 2: Test connection with real API call
if let Some(catalog) = catalog_manager.get_catalog(&name) {
    match catalog.list_namespaces(None).await {
        Ok(_) => {
            // Connection verified
        }
        Err(_) => {
            // Remove unreachable catalog
            catalog_manager.remove_catalog(&name);
        }
    }
}
```

❌ **WRONG - Unverified connection:**
```rust
catalog_manager.connect(name, config).await?;
// Assumes it worked - will show as "connected" even if server is down!
```

### OAuth2 Authentication Pattern

**Use client_credentials grant for service-to-service auth:**

```rust
async fn get_oauth2_token(token_url: &str, credential: &str, scope: Option<&String>) -> Result<String> {
    // credential format: "client_id:client_secret"
    let parts: Vec<&str> = credential.split(':').collect();

    // MUST URL-encode all form values
    let form_data = format!(
        "grant_type=client_credentials&client_id={}&client_secret={}",
        urlencoding::encode(parts[0]),
        urlencoding::encode(parts[1])
    );

    // MUST use correct content type
    let response = client
        .post(token_url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(form_data)
        .send()
        .await?;

    // Parse access_token from response
    let token_response: TokenResponse = response.json().await?;
    Ok(token_response.access_token)
}
```

**TODO: Implement token caching with expiration tracking**

## Implementation Patterns

### Adding a New Metadata Field

**Complete checklist:**

1. ✅ Add field to `TableMetadata` struct in `metadata.rs`
2. ✅ Implement `Debug` and `Clone` for any new types
3. ✅ Extract field in `from_table()` method
4. ✅ Update `App::new()` to initialize if needed
5. ✅ Display in appropriate UI module
6. ✅ Add tests for extraction logic

Example:
```rust
// 1. Add to struct
#[derive(Debug, Clone)]
pub struct TableMetadata {
    pub location: String,
    pub schema: SchemaInfo,
    pub new_field: Option<NewFieldType>,  // Add here
}

// 2. Extract in from_table
impl TableMetadata {
    pub fn from_table(table: &Table) -> Result<Self> {
        let new_field = Some(extract_new_field(&metadata));

        Ok(Self {
            location: metadata.location().to_string(),
            schema: schema_info,
            new_field,  // Include here
        })
    }
}

// 3. Display in UI
fn render_table_metadata_content(...) {
    if let Some(value) = &metadata.new_field {
        lines.push(Line::from(vec![
            Span::styled("New Field: ", Style::default().fg(Color::Yellow)),
            Span::styled(value.to_string(), Style::default().fg(Color::White)),
        ]));
    }
}
```

### Adding a New View Mode

**Complete implementation:**

```rust
// 1. Add to ViewState enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewState {
    Browser,
    TableDetail,
    MyNewView,  // Add here
    Query,
    Help,
}

// 2. Create render function in ui/ module
pub fn render_my_new_view(frame: &mut Frame, area: Rect, app: &App) {
    // Pure rendering only - no mutations!
    let content = format!("My new view content: {}", app.some_field);
    let paragraph = Paragraph::new(content)
        .block(Block::default().title("My New View").borders(Borders::ALL));
    frame.render_widget(paragraph, area);
}

// 3. Add to render dispatcher
fn render_main_content(frame: &mut Frame, area: Rect, app: &App) {
    match app.view_state {
        ViewState::Browser => render_browser_view(frame, area, app),
        ViewState::MyNewView => render_my_new_view(frame, area, app),
        // ...
    }
}

// 4. Add keyboard shortcut
ViewState::Browser => {
    match (key.code, key.modifiers) {
        (KeyCode::Char('m'), KeyModifiers::NONE) => {
            self.view_state = ViewState::MyNewView;
            false
        }
        // ...
    }
}

// 5. Handle ESC in new view
ViewState::MyNewView => {
    if key.code == KeyCode::Esc {
        self.view_state = ViewState::Browser;
    }
    false
}

// 6. Update help text
fn render_help(frame: &mut Frame, area: Rect) {
    let help_text = vec![
        // ...
        Line::from(vec![
            Span::styled("m", Style::default().fg(Color::Yellow)),
            Span::raw(" - Switch to My New View"),
        ]),
        // ...
    ];
}
```

## Testing and Debugging

### Running with Debug Logs

**ALWAYS test with logging enabled:**

```bash
# Terminal 1: Run app with debug logging
RUST_LOG=debug cargo run

# Terminal 2: Tail log file
tail -f icetea.log
```

**Look for these log patterns:**
- `Fetching catalog config from:` - REST API calls
- `Got OAuth2 token` - Authentication success
- `Fetched catalog config: N properties` - Config loaded
- `Failed to...` - Error conditions

### Common Debugging Scenarios

**Tree not updating after key press:**

Check:
1. Does `handle_key_event()` return `true`?
2. Is `rebuild_tree().await` called in the main loop?
3. Are `expanded` keys being added/removed correctly?

**Metadata not displaying:**

Check:
1. Is `selected_table_metadata` populated?
2. Is caching working correctly (check `cached_table_key`)?
3. Are fields being extracted in `from_table()`?
4. Is UI rendering the fields?

**OAuth2 authentication failing:**

Check log for:
- `Failed to get OAuth2 token:` → credential format or URL wrong
- `OAuth2 token request failed: 401` → wrong credentials
- `OAuth2 token request failed: 400` → malformed request

Common fixes:
- Verify credential format: `"client_id:client_secret"` (colon-separated)
- Check token URL ends with `/oauth2/token`
- Verify scope matches server requirements

## Performance Guidelines

### Tree Rebuilding

**Current complexity: O(catalogs × namespaces × tables)**

For large catalogs (>100 namespaces or >1000 tables):
- ❌ Do NOT auto-expand all nodes on startup
- ✅ DO expand on-demand only
- ✅ DO consider pagination API if catalog supports it
- ✅ DO cache namespace/table lists per catalog

### Metadata Loading

**Current strategy: Cache per table**

```rust
pub async fn load_selected_table_metadata(&mut self) -> bool {
    // Check cache first
    if self.cached_table_key.as_ref() == Some(&item.key)
        && self.selected_table_metadata.is_some()
    {
        return true;  // Use cached data
    }

    // Load if cache miss
    // ...
}
```

**TODO: Consider cache invalidation strategy for long-running sessions**

## Dependencies and Version Policy

**Prefer stable, well-maintained crates:**

| Crate | Role | Why This One |
|-------|------|--------------|
| `iceberg`, `iceberg-catalog-rest` | Iceberg operations | Official Apache implementation |
| `ratatui` | TUI framework | Most active TUI crate, excellent docs |
| `tokio` | Async runtime | Industry standard, well-tested |
| `tracing` | Logging | Structured logging, great ecosystem |
| `figment` | Config | Layered config with env/file support |
| `reqwest` | HTTP client | Ergonomic, widely used |
| `anyhow` | Error handling | Simple, composable error handling |

**Version constraints:**
- Use `cargo add` to get latest compatible versions
- Pin major versions only: `iceberg = "0.4"`
- Let Cargo resolve minor/patch versions
- Update dependencies regularly: `cargo update`

## Future Work - Prioritized

**High Priority:**
- [ ] OAuth2 token caching with expiration
- [ ] Export/copy schema to clipboard or file
- [ ] Connection retry logic with backoff
- [ ] Table search/filter in large catalogs

**Medium Priority:**
- [ ] SQL query execution via DataFusion
- [ ] Snapshot history viewer with diffs
- [ ] Support for Glue/Hive catalogs
- [ ] Pagination for large result sets

**Low Priority:**
- [ ] Table statistics display
- [ ] File listing with sizes
- [ ] Schema evolution tracking
- [ ] Toggle borders feature for text selection

## Troubleshooting Reference

### Build Issues

| Error | Cause | Fix |
|-------|-------|-----|
| "use of unstable library feature" | Using nightly-only feature | Use stable Rust |
| Type mismatch with `Box<Type>` | Iceberg uses boxed types | Use `.as_ref()` when matching |
| Missing trait implementations | New struct needs derives | Add `#[derive(Debug, Clone)]` |

### Runtime Issues

| Symptom | Likely Cause | Check |
|---------|--------------|-------|
| Catalog shows connected but empty | Auth works but access denied | Check OAuth2 scopes |
| "No warehouse specified" | Config missing warehouse param | Add `warehouse = "name"` |
| OAuth2 always fails | Credential format wrong | Must be `"id:secret"` |
| Tree doesn't update | Rebuild not triggered | Return `true` from handler |

### Log Analysis

**OAuth2 flow (successful):**
```
DEBUG Obtaining OAuth2 token from: https://...
DEBUG Got OAuth2 token, adding to request
DEBUG Fetching catalog config from: http://...
DEBUG Got successful response from catalog config endpoint
INFO  Fetched catalog config for catalog_name: 3 properties
```

**OAuth2 flow (failed):**
```
DEBUG Obtaining OAuth2 token from: https://...
WARN  Failed to get OAuth2 token: OAuth2 token request failed: 401
WARN  Failed to fetch catalog config for catalog_name: ...
```
