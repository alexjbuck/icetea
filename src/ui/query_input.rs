//! Query input and results display

use crate::app::{App, QueryResult};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub fn render_query_view(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Query input
            Constraint::Min(0),     // Query results
        ])
        .split(area);

    render_query_input(frame, chunks[0], app);
    render_query_results(frame, chunks[1], app);
}

fn render_query_input(frame: &mut Frame, area: Rect, app: &App) {
    let input = Paragraph::new(app.query_input.as_str())
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default()
            .title("SQL Query")
            .borders(Borders::ALL));

    frame.render_widget(input, area);

    // Show cursor
    if !app.query_input.is_empty() {
        frame.set_cursor_position((
            area.x + app.query_input.len() as u16 + 1,
            area.y + 1,
        ));
    } else {
        frame.set_cursor_position((area.x + 1, area.y + 1));
    }
}

fn render_query_results(frame: &mut Frame, area: Rect, app: &App) {
    let content = match &app.last_result {
        Some(QueryResult::Success { rows, message }) => {
            vec![
                Line::from(""),
                Line::from(Span::styled(
                    format!("✓ Success: {} rows", rows),
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(message.clone()),
            ]
        }
        Some(QueryResult::Error { message }) => {
            vec![
                Line::from(""),
                Line::from(Span::styled(
                    "✗ Error",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(message.clone(), Style::default().fg(Color::Red))),
            ]
        }
        None => {
            vec![
                Line::from(""),
                Line::from("Enter a SQL query above and press Enter to execute."),
                Line::from(""),
                Line::from("Examples:"),
                Line::from("  SELECT * FROM catalog.namespace.table LIMIT 10"),
                Line::from("  SHOW TABLES FROM catalog.namespace"),
            ]
        }
    };

    let paragraph = Paragraph::new(content)
        .block(Block::default()
            .title("Query Results")
            .borders(Borders::ALL));

    frame.render_widget(paragraph, area);
}
