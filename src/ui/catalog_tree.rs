//! Catalog tree view for browsing catalogs, namespaces, and tables

use crate::app::App;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

pub fn render_catalog_tree(frame: &mut Frame, area: Rect, app: &App) {
    let mut items = Vec::new();

    // Render catalog list
    if app.catalogs.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            "No catalogs configured",
            Style::default().fg(Color::DarkGray),
        ))));
    } else {
        for (name, state) in &app.catalogs {
            let status_icon = if state.connected { "●" } else { "○" };
            let status_color = if state.connected { Color::Green } else { Color::Red };

            let selected = app.selected_catalog.as_ref().map(|s| s == name).unwrap_or(false);
            let style = if selected {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            items.push(ListItem::new(Line::from(vec![
                Span::styled(status_icon, Style::default().fg(status_color)),
                Span::raw(" "),
                Span::styled(name, style),
            ])));

            // Show error if any
            if let Some(error) = &state.error {
                items.push(ListItem::new(Line::from(Span::styled(
                    format!("  ↳ Error: {}", error),
                    Style::default().fg(Color::Red),
                ))));
            }

            // TODO: Show namespaces and tables in tree structure
        }
    }

    let list = List::new(items)
        .block(Block::default()
            .title("Catalogs")
            .borders(Borders::ALL));

    frame.render_widget(list, area);
}
