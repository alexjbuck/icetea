//! UI components and rendering

pub mod catalog_tree;
pub mod detail_view;
pub mod query_input;

use crate::app::{App, ViewState};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

/// Render the main UI
pub fn render(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),      // Title bar
            Constraint::Min(0),         // Main content
            Constraint::Length(3),      // Status/input bar
        ])
        .split(frame.area());

    render_title_bar(frame, chunks[0], app);
    render_main_content(frame, chunks[1], app);
    render_status_bar(frame, chunks[2], app);
}

fn render_title_bar(frame: &mut Frame, area: Rect, _app: &App) {
    let title = Line::from(vec![
        Span::styled("IceTea", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(" - Apache Iceberg TUI"),
    ]);

    let title_bar = Paragraph::new(title);
    frame.render_widget(title_bar, area);
}

fn render_main_content(frame: &mut Frame, area: Rect, app: &mut App) {
    match app.view_state {
        ViewState::Browser => {
            render_browser_view(frame, area, app);
        }
        ViewState::TableDetail => {
            detail_view::render_table_detail(frame, area, app);
        }
        ViewState::SnapshotHistory => {
            detail_view::render_snapshot_history(frame, area, app);
        }
        ViewState::FileList => {
            detail_view::render_file_list(frame, area, app);
        }
        ViewState::Query => {
            query_input::render_query_view(frame, area, app);
        }
        ViewState::Help => {
            render_help(frame, area);
        }
    }
}

fn render_browser_view(frame: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30),  // Catalog tree
            Constraint::Percentage(70),  // Detail panel
        ])
        .split(area);

    catalog_tree::render_catalog_tree(frame, chunks[0], app);
    detail_view::render_detail_panel(frame, chunks[1], app);
}

fn render_help(frame: &mut Frame, area: Rect) {
    let help_text = vec![
        Line::from(""),
        Line::from(Span::styled("IceTea - Keyboard Shortcuts", Style::default().add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(vec![
            Span::styled("Tab", Style::default().fg(Color::Yellow)),
            Span::raw(" - Switch focus between catalog tree and detail pane"),
        ]),
        Line::from(vec![
            Span::styled("j/k / ↑↓", Style::default().fg(Color::Yellow)),
            Span::raw(" - Navigate tree (or scroll detail when focused)"),
        ]),
        Line::from(vec![
            Span::styled("Ctrl+u/d  Shift+↑/↓  PgUp/PgDn", Style::default().fg(Color::Yellow)),
            Span::raw(" - Page-scroll detail pane (when focused)"),
        ]),
        Line::from(vec![
            Span::styled("g / G  (Home/End)", Style::default().fg(Color::Yellow)),
            Span::raw(" - Jump detail to top / bottom (when focused)"),
        ]),
        Line::from(vec![
            Span::styled("q", Style::default().fg(Color::Yellow)),
            Span::raw(" - Quit application"),
        ]),
        Line::from(vec![
            Span::styled(":", Style::default().fg(Color::Yellow)),
            Span::raw(" - Enter query mode"),
        ]),
        Line::from(vec![
            Span::styled("?", Style::default().fg(Color::Yellow)),
            Span::raw(" - Show this help"),
        ]),
        Line::from(vec![
            Span::styled("ESC", Style::default().fg(Color::Yellow)),
            Span::raw(" - Return to browser"),
        ]),
        Line::from(""),
        Line::from("Press any key to return..."),
    ];

    let help_block = Paragraph::new(help_text)
        .block(Block::default()
            .title("Help")
            .borders(Borders::ALL));

    frame.render_widget(help_block, area);
}

fn render_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let connected_count = app.catalog_manager.list_catalogs().len();

    let status_text = match app.view_state {
        ViewState::Browser => {
            let focus = match app.focused_pane {
                crate::app::PaneFocus::Tree => "focus:tree",
                crate::app::PaneFocus::Detail => "focus:detail",
            };
            if let Some(msg) = &app.status_message {
                format!(
                    "{} | {} | Tab:switch  ↑↓/jk  Ctrl+u/d:page  ←→/hl  Enter  ?:help  q",
                    msg, focus
                )
            } else {
                format!(
                    "Connected: {} | {} | Tab:switch  ↑↓/jk  Ctrl+u/d:page  ←→/hl  Enter  ?:help  q",
                    connected_count, focus
                )
            }
        }
        ViewState::Query => {
            format!("> {}", app.query_input)
        }
        _ => {
            "Press ESC to return to browser".to_string()
        }
    };

    let status = Paragraph::new(status_text)
        .block(Block::default().borders(Borders::ALL));

    frame.render_widget(status, area);
}
