//! Application state and event handling

use crate::config::Config;
use anyhow::Result;
use std::collections::HashMap;

/// Main application state
pub struct App {
    /// Configuration
    pub config: Config,

    /// Current view state
    pub view_state: ViewState,

    /// Whether the application should exit
    pub should_quit: bool,

    /// Connected catalogs
    pub catalogs: HashMap<String, CatalogState>,

    /// Current selected catalog
    pub selected_catalog: Option<String>,

    /// Current selected namespace/table path
    pub selected_path: Vec<String>,

    /// Query input buffer
    pub query_input: String,

    /// Last query result or error
    pub last_result: Option<QueryResult>,
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

/// State of a catalog connection
#[derive(Debug)]
pub struct CatalogState {
    pub name: String,
    pub connected: bool,
    pub namespaces: Vec<String>,
    pub error: Option<String>,
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
            catalogs: HashMap::new(),
            selected_catalog: None,
            selected_path: Vec::new(),
            query_input: String::new(),
            last_result: None,
        }
    }

    /// Initialize the application (connect to catalogs, etc.)
    pub async fn initialize(&mut self) -> Result<()> {
        // Initialize catalog connections
        for (name, _config) in &self.config.catalogs {
            self.catalogs.insert(
                name.clone(),
                CatalogState {
                    name: name.clone(),
                    connected: false,
                    namespaces: Vec::new(),
                    error: None,
                },
            );
        }

        Ok(())
    }

    /// Handle keyboard input
    pub fn handle_key_event(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::{KeyCode, KeyModifiers};

        match self.view_state {
            ViewState::Browser => {
                match (key.code, key.modifiers) {
                    // Quit
                    (KeyCode::Char('q'), KeyModifiers::NONE) => {
                        self.should_quit = true;
                    }
                    // Switch to query mode
                    (KeyCode::Char(':'), KeyModifiers::NONE) => {
                        self.view_state = ViewState::Query;
                    }
                    // Help
                    (KeyCode::Char('?'), KeyModifiers::NONE) => {
                        self.view_state = ViewState::Help;
                    }
                    _ => {}
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
            }
            ViewState::Help => {
                // Any key exits help
                self.view_state = ViewState::Browser;
            }
            _ => {
                // For other views, ESC returns to browser
                if key.code == KeyCode::Esc {
                    self.view_state = ViewState::Browser;
                }
            }
        }
    }
}
