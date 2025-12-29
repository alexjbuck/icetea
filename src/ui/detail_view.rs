//! Detail view panels for showing table metadata, snapshots, files, etc.

use crate::app::App;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub fn render_detail_panel(frame: &mut Frame, area: Rect, app: &App) {
    let content = if let Some(catalog_name) = &app.selected_catalog {
        if app.selected_path.is_empty() {
            vec![
                Line::from(""),
                Line::from(Span::styled(
                    format!("Catalog: {}", catalog_name),
                    Style::default().fg(Color::Cyan),
                )),
                Line::from(""),
                Line::from("Select a namespace or table to view details."),
            ]
        } else {
            vec![
                Line::from(""),
                Line::from(Span::styled(
                    format!("Path: {}", app.selected_path.join(".")),
                    Style::default().fg(Color::Cyan),
                )),
                Line::from(""),
                Line::from("Table details will appear here."),
            ]
        }
    } else {
        vec![
            Line::from(""),
            Line::from("Select a catalog from the tree to view details."),
        ]
    };

    let paragraph = Paragraph::new(content)
        .block(Block::default()
            .title("Detail View")
            .borders(Borders::ALL));

    frame.render_widget(paragraph, area);
}

pub fn render_table_detail(frame: &mut Frame, area: Rect, _app: &App) {
    let content = vec![
        Line::from(""),
        Line::from(Span::styled("Table Details", Style::default().fg(Color::Yellow))),
        Line::from(""),
        Line::from("Schema, properties, and metadata will appear here."),
    ];

    let paragraph = Paragraph::new(content)
        .block(Block::default()
            .title("Table Detail")
            .borders(Borders::ALL));

    frame.render_widget(paragraph, area);
}

pub fn render_snapshot_history(frame: &mut Frame, area: Rect, _app: &App) {
    let content = vec![
        Line::from(""),
        Line::from(Span::styled("Snapshot History", Style::default().fg(Color::Yellow))),
        Line::from(""),
        Line::from("Timeline of table snapshots will appear here."),
        Line::from(""),
        Line::from("• Commit history"),
        Line::from("• Branches and tags"),
        Line::from("• Snapshot metadata"),
    ];

    let paragraph = Paragraph::new(content)
        .block(Block::default()
            .title("Snapshot History")
            .borders(Borders::ALL));

    frame.render_widget(paragraph, area);
}

pub fn render_file_list(frame: &mut Frame, area: Rect, _app: &App) {
    let content = vec![
        Line::from(""),
        Line::from(Span::styled("File List", Style::default().fg(Color::Yellow))),
        Line::from(""),
        Line::from("List of data files in the table/partition will appear here."),
        Line::from(""),
        Line::from("Columns:"),
        Line::from("• File path"),
        Line::from("• File size"),
        Line::from("• Record count"),
        Line::from("• Partition values"),
    ];

    let paragraph = Paragraph::new(content)
        .block(Block::default()
            .title("File List")
            .borders(Borders::ALL));

    frame.render_widget(paragraph, area);
}
