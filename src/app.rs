//! Application state and event handling

use crate::config::Config;
use crate::iceberg::{CatalogManager, TableMetadata};
use anyhow::Result;
use iceberg::Catalog;
use std::collections::HashSet;

/// Main application state
pub struct App {
    /// Configuration
    pub config: Config,

    /// Current view state
    pub view_state: ViewState,

    /// Whether the application should exit
    pub should_quit: bool,

    /// Catalog manager for Iceberg connections
    pub catalog_manager: CatalogManager,

    /// Tree items for display
    pub tree_items: Vec<TreeItem>,

    /// Currently selected tree index
    pub selected_index: usize,

    /// Expanded tree items (by their path key)
    pub expanded: HashSet<String>,

    /// Query input buffer
    pub query_input: String,

    /// Status message to display
    pub status_message: Option<String>,

    /// Whether we're currently loading
    pub loading: bool,

    /// Last query result
    pub last_result: Option<QueryResult>,

    /// Currently selected table's metadata (cached when a table is selected)
    pub selected_table_metadata: Option<TableMetadata>,

    /// Key of the table whose metadata is currently cached
    pub cached_table_key: Option<String>,
}

/// Represents an item in the tree view
#[derive(Debug, Clone)]
pub struct TreeItem {
    /// Display name
    pub name: String,
    /// Depth in tree (for indentation)
    pub depth: usize,
    /// Type of item
    pub item_type: TreeItemType,
    /// Unique key for this item
    pub key: String,
    /// Whether this item can be expanded
    pub expandable: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TreeItemType {
    Catalog { connected: bool },
    Namespace,
    Table,
}

/// Current view/mode in the UI
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewState {
    /// Browsing catalog tree
    Browser,
    /// Viewing table details
    TableDetail,
    /// Viewing snapshot history
    SnapshotHistory,
    /// Viewing file list
    FileList,
    /// Query input mode
    Query,
    /// Help screen
    Help,
}

/// Query execution result
#[derive(Debug)]
pub enum QueryResult {
    Success { rows: usize, message: String },
    Error { message: String },
}

impl App {
    /// Create a new application instance
    pub fn new(config: Config) -> Self {
        Self {
            config,
            view_state: ViewState::Browser,
            should_quit: false,
            catalog_manager: CatalogManager::new(),
            tree_items: Vec::new(),
            selected_index: 0,
            expanded: HashSet::new(),
            query_input: String::new(),
            status_message: None,
            loading: false,
            last_result: None,
            selected_table_metadata: None,
            cached_table_key: None,
        }
    }

    /// Initialize the application (connect to catalogs, etc.)
    pub async fn initialize(&mut self) -> Result<()> {
        self.status_message = Some("Connecting to catalogs...".to_string());
        self.loading = true;

        // Connect to all configured catalogs and verify connection
        for (name, catalog_config) in self.config.catalogs.clone() {
            match self.catalog_manager.connect(name.clone(), &catalog_config).await {
                Ok(_) => {
                    // Verify the connection actually works by listing namespaces
                    if let Some(catalog) = self.catalog_manager.get_catalog(&name) {
                        match catalog.list_namespaces(None).await {
                            Ok(_) => {
                                self.status_message = Some(format!("Connected to {}", name));
                            }
                            Err(e) => {
                                // Connection failed, remove the catalog
                                self.catalog_manager.remove_catalog(&name);
                                self.status_message = Some(format!("Failed to connect to {}: {}", name, e));
                            }
                        }
                    }
                }
                Err(e) => {
                    self.status_message = Some(format!("Failed to connect to {}: {}", name, e));
                }
            }
        }

        self.rebuild_tree().await;
        self.loading = false;
        self.status_message = Some("Ready".to_string());

        Ok(())
    }

    /// Rebuild the visible tree from current state
    pub async fn rebuild_tree(&mut self) {
        self.tree_items.clear();

        // Add all configured catalogs
        for name in self.config.catalogs.keys() {
            let connected = self.catalog_manager.get_catalog(name).is_some();
            let key = name.clone();

            self.tree_items.push(TreeItem {
                name: name.clone(),
                depth: 0,
                item_type: TreeItemType::Catalog { connected },
                key: key.clone(),
                expandable: connected,
            });

            // If expanded and connected, show namespaces
            if self.expanded.contains(&key) && connected {
                if let Some(catalog) = self.catalog_manager.get_catalog(name) {
                    if let Ok(namespaces) = catalog.list_namespaces(None).await {
                        for ns in namespaces {
                            let ns_name = ns.as_ref().join(".");
                            let ns_key = format!("{}/{}", name, ns_name);

                            self.tree_items.push(TreeItem {
                                name: ns_name.clone(),
                                depth: 1,
                                item_type: TreeItemType::Namespace,
                                key: ns_key.clone(),
                                expandable: true,
                            });

                            // If namespace is expanded, show tables
                            if self.expanded.contains(&ns_key) {
                                if let Ok(tables) = catalog.list_tables(&ns).await {
                                    for table_ident in tables {
                                        let table_name = table_ident.name().to_string();
                                        let table_key = format!("{}/{}", ns_key, table_name);

                                        self.tree_items.push(TreeItem {
                                            name: table_name,
                                            depth: 2,
                                            item_type: TreeItemType::Table,
                                            key: table_key,
                                            expandable: false,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Clamp selected index
        if !self.tree_items.is_empty() && self.selected_index >= self.tree_items.len() {
            self.selected_index = self.tree_items.len() - 1;
        }
    }

    /// Toggle expansion of the selected item
    pub fn toggle_selected(&mut self) -> bool {
        if let Some(item) = self.tree_items.get(self.selected_index) {
            if item.expandable {
                let key = item.key.clone();
                if self.expanded.contains(&key) {
                    self.expanded.remove(&key);
                } else {
                    self.expanded.insert(key);
                }
                return true; // Need to rebuild tree
            }
        }
        false
    }

    /// Move selection up
    pub fn select_previous(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    /// Move selection down
    pub fn select_next(&mut self) {
        if !self.tree_items.is_empty() && self.selected_index < self.tree_items.len() - 1 {
            self.selected_index += 1;
        }
    }

    /// Collapse selected item (or move to parent)
    pub fn collapse_or_parent(&mut self) -> bool {
        if let Some(item) = self.tree_items.get(self.selected_index) {
            let key = item.key.clone();
            if self.expanded.contains(&key) {
                // Collapse current item
                self.expanded.remove(&key);
                return true;
            } else if item.depth > 0 {
                // Move to parent - find the parent item
                for i in (0..self.selected_index).rev() {
                    if self.tree_items[i].depth < item.depth {
                        self.selected_index = i;
                        break;
                    }
                }
            }
        }
        false
    }

    /// Expand selected item
    pub fn expand_selected(&mut self) -> bool {
        if let Some(item) = self.tree_items.get(self.selected_index) {
            if item.expandable && !self.expanded.contains(&item.key) {
                self.expanded.insert(item.key.clone());
                return true;
            }
        }
        false
    }

    /// Get the currently selected item
    pub fn selected_item(&self) -> Option<&TreeItem> {
        self.tree_items.get(self.selected_index)
    }

    /// Load metadata for the currently selected table (if it's a table)
    /// Returns true if metadata was loaded or is already cached
    pub async fn load_selected_table_metadata(&mut self) -> bool {
        let item = match self.selected_item() {
            Some(item) if item.item_type == TreeItemType::Table => item.clone(),
            _ => {
                // Not a table, clear cached metadata
                self.selected_table_metadata = None;
                self.cached_table_key = None;
                return false;
            }
        };

        // Check if we already have this table's metadata cached
        if self.cached_table_key.as_ref() == Some(&item.key) && self.selected_table_metadata.is_some() {
            return true;
        }

        // Parse the table key: catalog/namespace/table_name
        let parts: Vec<&str> = item.key.split('/').collect();
        if parts.len() < 3 {
            self.status_message = Some(format!("Invalid table key: {}", item.key));
            return false;
        }

        let catalog_name = parts[0];
        let namespace = parts[1];
        let table_name = parts[2];

        self.status_message = Some(format!("Loading metadata for {}...", item.name));
        self.loading = true;

        match self.catalog_manager.load_table(catalog_name, namespace, table_name).await {
            Ok(table) => {
                match TableMetadata::from_table(&table) {
                    Ok(mut metadata) => {
                        // Fetch storage config from the REST API
                        if let Some(catalog_config) = self.config.catalogs.get(catalog_name) {
                            if let Ok(storage_config) = self.catalog_manager
                                .fetch_table_storage_config(catalog_name, catalog_config, namespace, table_name)
                                .await
                            {
                                metadata.storage_properties = storage_config;
                            }
                        }

                        self.selected_table_metadata = Some(metadata);
                        self.cached_table_key = Some(item.key.clone());
                        self.status_message = Some(format!("Loaded metadata for {}", item.name));
                        self.loading = false;
                        true
                    }
                    Err(e) => {
                        self.status_message = Some(format!("Failed to parse metadata: {}", e));
                        self.loading = false;
                        false
                    }
                }
            }
            Err(e) => {
                self.status_message = Some(format!("Failed to load table: {}", e));
                self.loading = false;
                false
            }
        }
    }

    /// Clear cached table metadata (called when selection changes)
    pub fn clear_table_metadata_if_needed(&mut self) {
        if let Some(item) = self.selected_item() {
            if self.cached_table_key.as_ref() != Some(&item.key) {
                self.selected_table_metadata = None;
            }
        }
    }

    /// Handle keyboard input (synchronous part)
    /// Returns true if tree needs rebuilding
    pub fn handle_key_event(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::{KeyCode, KeyModifiers};

        match self.view_state {
            ViewState::Browser => {
                match (key.code, key.modifiers) {
                    // Quit
                    (KeyCode::Char('q'), KeyModifiers::NONE) => {
                        self.should_quit = true;
                        false
                    }
                    // Navigation - up
                    (KeyCode::Up, KeyModifiers::NONE) | (KeyCode::Char('k'), KeyModifiers::NONE) => {
                        self.select_previous();
                        false
                    }
                    // Navigation - down
                    (KeyCode::Down, KeyModifiers::NONE) | (KeyCode::Char('j'), KeyModifiers::NONE) => {
                        self.select_next();
                        false
                    }
                    // Collapse / go to parent
                    (KeyCode::Left, KeyModifiers::NONE) | (KeyCode::Char('h'), KeyModifiers::NONE) => {
                        self.collapse_or_parent()
                    }
                    // Expand
                    (KeyCode::Right, KeyModifiers::NONE) | (KeyCode::Char('l'), KeyModifiers::NONE) => {
                        self.expand_selected()
                    }
                    // Toggle expand/collapse
                    (KeyCode::Enter, KeyModifiers::NONE) => {
                        self.toggle_selected()
                    }
                    // Switch to query mode
                    (KeyCode::Char(':'), KeyModifiers::NONE) => {
                        self.view_state = ViewState::Query;
                        false
                    }
                    // Help
                    (KeyCode::Char('?'), KeyModifiers::NONE) => {
                        self.view_state = ViewState::Help;
                        false
                    }
                    _ => false,
                }
            }
            ViewState::Query => {
                match key.code {
                    KeyCode::Esc => {
                        self.view_state = ViewState::Browser;
                        self.query_input.clear();
                    }
                    KeyCode::Char(c) => {
                        self.query_input.push(c);
                    }
                    KeyCode::Backspace => {
                        self.query_input.pop();
                    }
                    KeyCode::Enter => {
                        // TODO: Execute query
                        self.view_state = ViewState::Browser;
                    }
                    _ => {}
                }
                false
            }
            ViewState::Help => {
                // Any key exits help
                self.view_state = ViewState::Browser;
                false
            }
            _ => {
                // For other views, ESC returns to browser
                if key.code == KeyCode::Esc {
                    self.view_state = ViewState::Browser;
                }
                false
            }
        }
    }
}
