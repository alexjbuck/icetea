//! Catalog tree view for browsing catalogs, namespaces, and tables

use crate::app::{App, TreeItemType};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};

pub fn render_catalog_tree(frame: &mut Frame, area: Rect, app: &App) {
    let mut items = Vec::new();

    if app.tree_items.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            "No catalogs configured",
            Style::default().fg(Color::DarkGray),
        ))));
    } else {
        for (idx, tree_item) in app.tree_items.iter().enumerate() {
            let is_selected = idx == app.selected_index;

            // Build indentation
            let indent = "  ".repeat(tree_item.depth);

            // Build prefix based on item type and expansion state
            let (icon, icon_color) = match &tree_item.item_type {
                TreeItemType::Catalog { connected } => {
                    if *connected {
                        if app.expanded.contains(&tree_item.key) {
                            ("▼ ", Color::Green)
                        } else {
                            ("▶ ", Color::Green)
                        }
                    } else {
                        ("○ ", Color::Red)
                    }
                }
                TreeItemType::Namespace => {
                    if app.expanded.contains(&tree_item.key) {
                        ("▼ ", Color::Blue)
                    } else {
                        ("▶ ", Color::Blue)
                    }
                }
                TreeItemType::Table => ("  ", Color::White),
            };

            // Build the type indicator
            let type_indicator = match &tree_item.item_type {
                TreeItemType::Catalog { .. } => "󰆼 ", // catalog/database icon
                TreeItemType::Namespace => "󰉋 ",     // folder icon
                TreeItemType::Table => "󰓫 ",         // table icon
            };

            let type_color = match &tree_item.item_type {
                TreeItemType::Catalog { connected } => {
                    if *connected { Color::Cyan } else { Color::DarkGray }
                }
                TreeItemType::Namespace => Color::Yellow,
                TreeItemType::Table => Color::Magenta,
            };

            // Build the name style
            let name_style = if is_selected {
                Style::default()
                    .fg(Color::White)
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let line = Line::from(vec![
                Span::raw(indent),
                Span::styled(icon, Style::default().fg(icon_color)),
                Span::styled(type_indicator, Style::default().fg(type_color)),
                Span::styled(&tree_item.name, name_style),
            ]);

            items.push(ListItem::new(line));
        }
    }

    let list = List::new(items)
        .block(Block::default()
            .title("Catalogs")
            .borders(Borders::ALL))
        .highlight_style(Style::default().bg(Color::DarkGray));

    // We're manually handling selection highlighting in the items themselves
    frame.render_widget(list, area);
}
