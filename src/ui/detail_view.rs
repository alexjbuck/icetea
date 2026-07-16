//! Detail view panels for showing table metadata, snapshots, files, etc.

use crate::app::App;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub fn render_detail_panel(frame: &mut Frame, area: Rect, app: &mut App) {
    let content = if let Some(item) = app.selected_item() {
        use crate::app::TreeItemType;
        match &item.item_type {
            TreeItemType::Catalog { connected } => {
                render_catalog_details(&item.name, *connected, app)
            }
            TreeItemType::Namespace => {
                vec![
                    Line::from(""),
                    Line::from(Span::styled(
                        format!("Namespace: {}", item.name),
                        Style::default().fg(Color::Cyan),
                    )),
                    Line::from(""),
                    Line::from(format!("Path: {}", item.key)),
                    Line::from(""),
                    Line::from("Expand to view tables."),
                ]
            }
            TreeItemType::Table => {
                render_table_metadata_content(&item.name, &item.key, app)
            }
        }
    } else {
        vec![
            Line::from(""),
            Line::from("Select an item from the tree to view details."),
        ]
    };

    // No wrap: scroll offsets are 1:1 with Line items. Wrapping made max-scroll
    // guesses wrong (word-wrap ≠ character-wrap), clipping the last orphan line.
    let viewport = area.height.saturating_sub(2);
    app.detail_line_count = content.len() as u16;
    app.detail_viewport_rows = viewport;
    let max_scroll = app
        .detail_line_count
        .saturating_sub(viewport.max(1));
    if app.detail_scroll > max_scroll {
        app.detail_scroll = max_scroll;
    }

    let focused = app.focused_pane == crate::app::PaneFocus::Detail;
    let title = if focused {
        "Detail View  [focused — Tab:tree  j/k  Ctrl+u/d:page]"
    } else {
        "Detail View  [Tab to focus/scroll]"
    };
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };

    let paragraph = Paragraph::new(content)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .scroll((app.detail_scroll, 0));

    frame.render_widget(paragraph, area);
}

/// Render catalog configuration details
fn render_catalog_details(name: &str, connected: bool, app: &App) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("Catalog: {}", name),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    // Connection status
    let status_color = if connected { Color::Green } else { Color::Red };
    lines.push(Line::from(vec![
        Span::styled("Status: ", Style::default().fg(Color::Yellow)),
        Span::styled(
            if connected { "Connected ✓" } else { "Disconnected ✗" },
            Style::default().fg(status_color),
        ),
    ]));

    // Get catalog configuration
    if let Some(catalog_config) = app.config.catalogs.get(name) {
        lines.push(Line::from(""));

        // Catalog type
        lines.push(Line::from(vec![
            Span::styled("Type: ", Style::default().fg(Color::Yellow)),
            Span::raw(catalog_config.catalog_type.clone()),
        ]));

        // URI
        lines.push(Line::from(vec![
            Span::styled("URI: ", Style::default().fg(Color::Yellow)),
            Span::raw(catalog_config.uri.clone()),
        ]));

        // Warehouse
        if let Some(warehouse) = &catalog_config.warehouse {
            lines.push(Line::from(vec![
                Span::styled("Warehouse: ", Style::default().fg(Color::Yellow)),
                Span::raw(warehouse.clone()),
            ]));
        }

        // Show server-provided configuration (from /v1/config endpoint)
        if let Some(server_config) = app.catalog_manager.get_catalog_config(name) {
            if !server_config.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "━━━ Server Configuration ━━━",
                    Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
                )));

                let mut props: Vec<_> = server_config.iter().collect();
                props.sort_by(|a, b| a.0.cmp(b.0));

                for (key, value) in props {
                    lines.push(Line::from(vec![
                        Span::styled(format!("  {}: ", key), Style::default().fg(Color::Yellow)),
                        Span::styled(truncate_string(value, 60), Style::default().fg(Color::White)),
                    ]));
                }
            }
        }

        // Show client properties if any
        if !catalog_config.properties.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "━━━ Client Properties ━━━",
                Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
            )));

            let mut props: Vec<_> = catalog_config.properties.iter().collect();
            props.sort_by(|a, b| a.0.cmp(b.0));

            for (key, value) in props {
                // Hide credential values for security
                let display_value = if key.contains("credential") || key.contains("secret") || key.contains("password") {
                    "***hidden***".to_string()
                } else {
                    truncate_string(value, 60)
                };

                lines.push(Line::from(vec![
                    Span::styled(format!("  {}: ", key), Style::default().fg(Color::Yellow)),
                    Span::styled(display_value, Style::default().fg(Color::White)),
                ]));
            }
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from("Expand to view namespaces."));

    lines
}

/// Render table metadata content including schema, partitioning, and sorting
fn render_table_metadata_content(name: &str, key: &str, app: &App) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("Table: {}", name),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    // Get catalog name from key (format: catalog/namespace/table)
    let _catalog_name = key.split('/').next().unwrap_or("");

    // Check if we have metadata loaded
    if let Some(metadata) = &app.selected_table_metadata {
        // Location
        lines.push(Line::from(vec![
            Span::styled("Location: ", Style::default().fg(Color::Yellow)),
            Span::raw(metadata.location.clone()),
        ]));

        // Show S3 endpoint from table storage config (fetched from table load response)
        if !metadata.storage_properties.is_empty() {
            if let Some(endpoint) = metadata.storage_properties.get("s3.endpoint")
                .or_else(|| metadata.storage_properties.get("s3-endpoint"))
            {
                lines.push(Line::from(vec![
                    Span::styled("S3 Endpoint: ", Style::default().fg(Color::Yellow)),
                    Span::raw(endpoint.clone()),
                ]));
            }

            if let Some(region) = metadata.storage_properties.get("s3.region")
                .or_else(|| metadata.storage_properties.get("s3.region-name"))
            {
                lines.push(Line::from(vec![
                    Span::styled("S3 Region: ", Style::default().fg(Color::Yellow)),
                    Span::raw(region.clone()),
                ]));
            }
        }

        lines.push(Line::from(""));

        // Schema section
        lines.push(Line::from(Span::styled(
            "━━━ Schema ━━━",
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(format!("Schema ID: {}", metadata.schema.schema_id)));
        lines.push(Line::from(""));

        if metadata.schema.fields.is_empty() {
            lines.push(Line::from(Span::styled(
                "  (no fields)",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            // Header
            lines.push(Line::from(vec![
                Span::styled("  ID  ", Style::default().fg(Color::DarkGray)),
                Span::styled("Name                    ", Style::default().fg(Color::DarkGray)),
                Span::styled("Type                    ", Style::default().fg(Color::DarkGray)),
                Span::styled("Req", Style::default().fg(Color::DarkGray)),
            ]));

            for field in &metadata.schema.fields {
                render_field(&mut lines, field, 0);
            }
        }
        lines.push(Line::from(""));

        // Partition Spec section
        lines.push(Line::from(Span::styled(
            "━━━ Partition Spec ━━━",
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        )));
        
        if metadata.partition_spec.is_empty() {
            lines.push(Line::from(Span::styled(
                "  (unpartitioned)",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            for (i, field) in metadata.partition_spec.iter().enumerate() {
                lines.push(Line::from(vec![
                    Span::styled(format!("  {}. ", i + 1), Style::default().fg(Color::DarkGray)),
                    Span::styled(field.name.clone(), Style::default().fg(Color::White)),
                    Span::styled(" = ", Style::default().fg(Color::DarkGray)),
                    Span::styled(field.transform.clone(), Style::default().fg(Color::Yellow)),
                    Span::styled(format!("(source_id: {})", field.source_id), Style::default().fg(Color::DarkGray)),
                ]));
            }
        }
        lines.push(Line::from(""));

        // Sort Order section
        lines.push(Line::from(Span::styled(
            "━━━ Sort Order ━━━",
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        )));
        
        if metadata.sort_order.is_empty() {
            lines.push(Line::from(Span::styled(
                "  (unsorted)",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            for (i, field) in metadata.sort_order.iter().enumerate() {
                let dir_symbol = if field.direction.contains("Asc") { "↑" } else { "↓" };
                lines.push(Line::from(vec![
                    Span::styled(format!("  {}. ", i + 1), Style::default().fg(Color::DarkGray)),
                    Span::styled(format!("source_id={}", field.source_id), Style::default().fg(Color::White)),
                    Span::raw(" "),
                    Span::styled(field.transform.clone(), Style::default().fg(Color::Yellow)),
                    Span::raw(" "),
                    Span::styled(dir_symbol.to_string(), Style::default().fg(Color::Green)),
                    Span::styled(format!(" {}", field.direction), Style::default().fg(Color::Cyan)),
                    Span::styled(format!(" (nulls: {})", field.null_order), Style::default().fg(Color::DarkGray)),
                ]));
            }
        }
        lines.push(Line::from(""));

        // Properties section (show ALL properties for debugging)
        if !metadata.properties.is_empty() {
            lines.push(Line::from(Span::styled(
                "━━━ Properties ━━━",
                Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
            )));

            let mut props: Vec<_> = metadata.properties.iter().collect();
            props.sort_by(|a, b| a.0.cmp(b.0));

            // Show ALL properties (not just 10) to debug what's available
            for (key, value) in props.iter() {
                lines.push(Line::from(vec![
                    Span::styled(format!("  {}: ", key), Style::default().fg(Color::Yellow)),
                    Span::styled(truncate_string(value, 60), Style::default().fg(Color::White)),
                ]));
            }

            lines.push(Line::from(""));
        }

        // Snapshot info
        if let Some(snapshot_id) = metadata.current_snapshot_id {
            lines.push(Line::from(Span::styled(
                "━━━ Current Snapshot ━━━",
                Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(vec![
                Span::styled("  ID: ", Style::default().fg(Color::Yellow)),
                Span::styled(format!("{}", snapshot_id), Style::default().fg(Color::White)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("  Total snapshots: ", Style::default().fg(Color::Yellow)),
                Span::styled(format!("{}", metadata.snapshots.len()), Style::default().fg(Color::White)),
            ]));

            // Prefer full/partial async stats; fall back to sync preview
            let snap = metadata
                .stats
                .partial_stats()
                .and_then(|s| s.current_snapshot.as_ref())
                .or(metadata.sync_stats.current_snapshot.as_ref());

            if let Some(snap) = snap {
                let ts = chrono::DateTime::from_timestamp_millis(snap.timestamp_ms)
                    .map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                    .unwrap_or_else(|| snap.timestamp_ms.to_string());
                lines.push(Line::from(vec![
                    Span::styled("  Timestamp: ", Style::default().fg(Color::Yellow)),
                    Span::styled(ts, Style::default().fg(Color::White)),
                ]));
                lines.push(Line::from(vec![
                    Span::styled("  Operation: ", Style::default().fg(Color::Yellow)),
                    Span::styled(snap.operation.clone(), Style::default().fg(Color::Cyan)),
                ]));

                if let Some(n) = snap.total_records {
                    lines.push(Line::from(vec![
                        Span::styled("  Total records: ", Style::default().fg(Color::Yellow)),
                        Span::styled(format!("{}", n), Style::default().fg(Color::White)),
                    ]));
                }
                if let Some(n) = snap.total_data_files {
                    lines.push(Line::from(vec![
                        Span::styled("  Data files: ", Style::default().fg(Color::Yellow)),
                        Span::styled(format!("{}", n), Style::default().fg(Color::White)),
                    ]));
                }
                if let Some(n) = snap.total_delete_files {
                    lines.push(Line::from(vec![
                        Span::styled("  Delete files: ", Style::default().fg(Color::Yellow)),
                        Span::styled(format!("{}", n), Style::default().fg(Color::White)),
                    ]));
                }
                if let Some(n) = snap.total_files_size {
                    lines.push(Line::from(vec![
                        Span::styled("  Data size: ", Style::default().fg(Color::Yellow)),
                        Span::styled(
                            crate::iceberg::format_bytes(n as u64),
                            Style::default().fg(Color::White),
                        ),
                    ]));
                }

                if metadata.stats.is_loading()
                    && snap.manifest_count == 0
                    && snap.manifest_list_size.is_none()
                {
                    let msg = metadata
                        .stats
                        .progress()
                        .map(|p| match p.total {
                            Some(t) => format!("  Partitions / manifests: {}… ({}/{})", p.phase, p.done, t),
                            None => format!("  Partitions / manifests: {}…", p.phase),
                        })
                        .unwrap_or_else(|| "  Partitions / manifests: scanning…".into());
                    lines.push(Line::from(Span::styled(
                        msg,
                        Style::default().fg(Color::DarkGray),
                    )));
                } else if metadata.stats.is_loading()
                    && snap.partition_count == 0
                    && snap.manifest_count > 0
                {
                    let msg = metadata
                        .stats
                        .progress()
                        .map(|p| match p.total {
                            Some(t) => format!(
                                "  Partitions: counting… ({}/{}) · {} manifests ({})",
                                p.done,
                                t,
                                snap.manifest_count,
                                crate::iceberg::format_bytes(snap.manifest_total_size)
                            ),
                            None => format!(
                                "  Partitions: counting… · {} manifests ({})",
                                snap.manifest_count,
                                crate::iceberg::format_bytes(snap.manifest_total_size)
                            ),
                        })
                        .unwrap_or_else(|| "  Partitions: counting…".into());
                    lines.push(Line::from(Span::styled(
                        msg,
                        Style::default().fg(Color::DarkGray),
                    )));
                    if let Some(size) = snap.manifest_list_size {
                        lines.push(Line::from(vec![
                            Span::styled("  Manifest list size: ", Style::default().fg(Color::Yellow)),
                            Span::styled(
                                crate::iceberg::format_bytes(size),
                                Style::default().fg(Color::White),
                            ),
                        ]));
                    }
                } else {
                    lines.push(Line::from(vec![
                        Span::styled("  Partitions: ", Style::default().fg(Color::Yellow)),
                        Span::styled(
                            format!("{}", snap.partition_count),
                            Style::default().fg(Color::White),
                        ),
                    ]));
                    if let Some(n) = snap.changed_partition_count {
                        lines.push(Line::from(vec![
                            Span::styled("  Changed partitions: ", Style::default().fg(Color::Yellow)),
                            Span::styled(format!("{}", n), Style::default().fg(Color::DarkGray)),
                        ]));
                    }
                    if let Some(n) = snap.added_data_files {
                        lines.push(Line::from(vec![
                            Span::styled("  Added data files: ", Style::default().fg(Color::Yellow)),
                            Span::styled(format!("{}", n), Style::default().fg(Color::Green)),
                        ]));
                    }
                    if let Some(n) = snap.deleted_data_files {
                        lines.push(Line::from(vec![
                            Span::styled("  Deleted data files: ", Style::default().fg(Color::Yellow)),
                            Span::styled(format!("{}", n), Style::default().fg(Color::Red)),
                        ]));
                    }

                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![
                        Span::styled("  Manifest list: ", Style::default().fg(Color::Yellow)),
                        Span::styled(
                            truncate_string(&snap.manifest_list_path, 50),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]));
                    if let Some(size) = snap.manifest_list_size {
                        lines.push(Line::from(vec![
                            Span::styled("  Manifest list size: ", Style::default().fg(Color::Yellow)),
                            Span::styled(
                                crate::iceberg::format_bytes(size),
                                Style::default().fg(Color::White),
                            ),
                        ]));
                    }
                    lines.push(Line::from(vec![
                        Span::styled("  Manifest files: ", Style::default().fg(Color::Yellow)),
                        Span::styled(
                            format!(
                                "{} ({})",
                                snap.manifest_count,
                                crate::iceberg::format_bytes(snap.manifest_total_size)
                            ),
                            Style::default().fg(Color::White),
                        ),
                    ]));
                }
            }
        }

        lines.push(Line::from(""));

        // Query planning tax — minimum bytes every scan must fetch before data
        let partial = metadata
            .stats
            .partial_stats()
            .unwrap_or(&metadata.sync_stats);
        let snap = partial
            .current_snapshot
            .as_ref()
            .or(metadata.sync_stats.current_snapshot.as_ref());

        lines.push(Line::from(Span::styled(
            "━━━ Query Planning Tax ━━━",
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(Span::styled(
            "  Bytes fetched before reading data files",
            Style::default().fg(Color::DarkGray),
        )));

        let meta_size = partial.current_metadata_size;
        let list_size = snap.and_then(|s| s.manifest_list_size);
        let manifest_count = snap.map(|s| s.manifest_count).unwrap_or(0);
        let manifest_bytes = snap.map(|s| s.manifest_total_size).unwrap_or(0);
        let storage_err = partial.storage_error.as_ref();

        // Each planning artifact on its own line
        lines.push(Line::from(vec![
            Span::styled("  metadata.json:      ", Style::default().fg(Color::Yellow)),
            match meta_size {
                Some(size) => Span::styled(
                    crate::iceberg::format_bytes(size),
                    Style::default().fg(Color::Cyan),
                ),
                None if metadata.stats.is_loading() => {
                    Span::styled("measuring…", Style::default().fg(Color::DarkGray))
                }
                None if storage_err.is_some() => {
                    Span::styled("unavailable", Style::default().fg(Color::Red))
                }
                None => Span::styled("unknown", Style::default().fg(Color::DarkGray)),
            },
        ]));

        lines.push(Line::from(vec![
            Span::styled("  manifest-list.avro: ", Style::default().fg(Color::Yellow)),
            match list_size {
                Some(size) => Span::styled(
                    crate::iceberg::format_bytes(size),
                    Style::default().fg(Color::Cyan),
                ),
                None if metadata.stats.is_loading() => {
                    Span::styled("measuring…", Style::default().fg(Color::DarkGray))
                }
                None if storage_err.is_some() => {
                    Span::styled("unavailable", Style::default().fg(Color::Red))
                }
                None => Span::styled("unknown", Style::default().fg(Color::DarkGray)),
            },
        ]));

        if manifest_count > 0 {
            lines.push(Line::from(vec![
                Span::styled("  manifest files:     ", Style::default().fg(Color::Yellow)),
                Span::styled(
                    format!("{}", manifest_count),
                    Style::default().fg(Color::White),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::styled("  manifest files size:", Style::default().fg(Color::Yellow)),
                Span::styled(
                    format!(
                        " {}  (upper bound; pruned per query)",
                        crate::iceberg::format_bytes(manifest_bytes)
                    ),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        } else if metadata.stats.is_loading() {
            lines.push(Line::from(vec![
                Span::styled("  manifest files:     ", Style::default().fg(Color::Yellow)),
                Span::styled("loading list…", Style::default().fg(Color::DarkGray)),
            ]));
        }

        if let Some(s) = snap {
            if !s.manifest_list_path.is_empty() && s.manifest_list_size.is_some() {
                lines.push(Line::from(vec![
                    Span::styled("  list path: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        truncate_string(&s.manifest_list_path, 55),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
            }
        }

        lines.push(Line::from(""));

        // Metadata files section — historical retention / storage
        let meta_stats = metadata
            .stats
            .partial_stats()
            .unwrap_or(&metadata.sync_stats);
        lines.push(Line::from(Span::styled(
            "━━━ Metadata Files ━━━",
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(vec![
            Span::styled("  Metadata files: ", Style::default().fg(Color::Yellow)),
            Span::styled(
                format!("{}", meta_stats.metadata_file_count),
                Style::default().fg(Color::White),
            ),
        ]));
        if let Some(size) = meta_stats.current_metadata_size {
            lines.push(Line::from(vec![
                Span::styled("  Current metadata size: ", Style::default().fg(Color::Yellow)),
                Span::styled(
                    crate::iceberg::format_bytes(size),
                    Style::default().fg(Color::White),
                ),
            ]));
        } else if metadata.stats.is_loading() {
            let msg = metadata
                .stats
                .progress()
                .map(|p| {
                    if p.phase.starts_with("orphans") {
                        "  Metadata sizes: from storage list (after orphan scan)…".to_string()
                    } else {
                        format!("  Metadata sizes: waiting ({})…", p.phase)
                    }
                })
                .unwrap_or_else(|| "  Metadata sizes: pending…".into());
            lines.push(Line::from(Span::styled(
                msg,
                Style::default().fg(Color::DarkGray),
            )));
        }
        if let Some(size) = meta_stats.total_metadata_size {
            lines.push(Line::from(vec![
                Span::styled("  Total metadata size: ", Style::default().fg(Color::Yellow)),
                Span::styled(
                    crate::iceberg::format_bytes(size),
                    Style::default().fg(Color::White),
                ),
            ]));
        }
        if let (Some(earliest), Some(latest)) = (
            meta_stats.metadata_earliest_ms,
            meta_stats.metadata_latest_ms,
        ) {
            let fmt = |ms: i64| {
                chrono::DateTime::from_timestamp_millis(ms)
                    .map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                    .unwrap_or_else(|| ms.to_string())
            };
            lines.push(Line::from(vec![
                Span::styled("  Metadata range: ", Style::default().fg(Color::Yellow)),
                Span::styled(
                    format!("{} → {}", fmt(earliest), fmt(latest)),
                    Style::default().fg(Color::White),
                ),
            ]));
        }

        lines.push(Line::from(""));

        // Storage inventory (from data/ + metadata/ listing)
        lines.push(Line::from(Span::styled(
            "━━━ Storage (all snapshots) ━━━",
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        )));
        match &metadata.stats {
            crate::iceberg::LazyStats::Loading(progress) => {
                let msg = if progress.phase.starts_with("orphans") {
                    match progress.total {
                        Some(t) => format!(
                            "  {}… ({}/{})",
                            progress.phase, progress.done, t
                        ),
                        None => format!("  {}…", progress.phase),
                    }
                } else {
                    format!("  Waiting (currently: {})…", progress.phase)
                };
                lines.push(Line::from(Span::styled(
                    msg,
                    Style::default().fg(Color::DarkGray),
                )));
            }
            crate::iceberg::LazyStats::Failed(msg) => {
                lines.push(Line::from(Span::styled(
                    format!("  Failed: {}", truncate_string(msg, 50)),
                    Style::default().fg(Color::Red),
                )));
            }
            crate::iceberg::LazyStats::Ready(stats) => match &stats.orphans {
                Some(s) => {
                    lines.push(Line::from(Span::styled(
                        "  Unique paths present under data/ + metadata/",
                        Style::default().fg(Color::DarkGray),
                    )));
                    lines.push(Line::from(vec![
                        Span::styled("  Accessible data:     ", Style::default().fg(Color::Yellow)),
                        Span::styled(
                            format!(
                                "{} ({})",
                                s.accessible_data_count,
                                crate::iceberg::format_bytes(s.accessible_data_size)
                            ),
                            Style::default().fg(Color::Cyan),
                        ),
                    ]));
                    lines.push(Line::from(vec![
                        Span::styled("  Accessible metadata: ", Style::default().fg(Color::Yellow)),
                        Span::styled(
                            format!(
                                "{} ({})",
                                s.accessible_metadata_count,
                                crate::iceberg::format_bytes(s.accessible_metadata_size)
                            ),
                            Style::default().fg(Color::Cyan),
                        ),
                    ]));

                    let meta_color = if s.metadata_count > 0 {
                        Color::Red
                    } else {
                        Color::Green
                    };
                    let data_color = if s.data_count > 0 {
                        Color::Red
                    } else {
                        Color::Green
                    };
                    lines.push(Line::from(vec![
                        Span::styled("  Orphan data:         ", Style::default().fg(Color::Yellow)),
                        Span::styled(
                            format!(
                                "{} ({})",
                                s.data_count,
                                crate::iceberg::format_bytes(s.data_size)
                            ),
                            Style::default().fg(data_color),
                        ),
                    ]));
                    lines.push(Line::from(vec![
                        Span::styled("  Orphan metadata:     ", Style::default().fg(Color::Yellow)),
                        Span::styled(
                            format!(
                                "{} ({})",
                                s.metadata_count,
                                crate::iceberg::format_bytes(s.metadata_size)
                            ),
                            Style::default().fg(meta_color),
                        ),
                    ]));
                    lines.push(Line::from(vec![
                        Span::styled("  Total on storage:    ", Style::default().fg(Color::Yellow)),
                        Span::styled(
                            crate::iceberg::format_bytes(s.total_on_storage()),
                            Style::default()
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]));
                    lines.push(Line::from(Span::styled(
                        "  (= accessible data + accessible metadata + orphans)",
                        Style::default().fg(Color::DarkGray),
                    )));
                }
                None => {
                    let msg = match &stats.storage_error {
                        Some(err) => format!("  (scan failed: {})", truncate_string(err, 60)),
                        None => "  (unable to scan storage)".to_string(),
                    };
                    lines.push(Line::from(Span::styled(
                        msg,
                        Style::default().fg(Color::Red),
                    )));
                }
            },
        }

        // Trailing blank so the last content line isn't glued to the border.
        lines.push(Line::from(""));
    } else if app.loading {
        lines.push(Line::from(Span::styled(
            "Loading table metadata...",
            Style::default().fg(Color::Yellow),
        )));
    } else {
        lines.push(Line::from(format!("Path: {}", key)));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Fetching table metadata...",
            Style::default().fg(Color::DarkGray),
        )));
    }

    lines
}

/// Render a field and its nested fields recursively
fn render_field(lines: &mut Vec<Line<'static>>, field: &crate::iceberg::metadata::FieldInfo, indent_level: usize) {
    

    let req_marker = if field.required { "✓" } else { "○" };
    let req_color = if field.required { Color::Green } else { Color::DarkGray };

    // Create indentation
    let indent = "  ".repeat(indent_level);

    lines.push(Line::from(vec![
        Span::styled(format!("{:>4}  ", field.id), Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{}{:<24}", indent, truncate_string(&field.name, 22 - indent_level * 2)), Style::default().fg(Color::White)),
        Span::styled(format!("{:<24}", truncate_string(&field.field_type, 22)), Style::default().fg(Color::Cyan)),
        Span::styled(req_marker.to_string(), Style::default().fg(req_color)),
    ]));

    // Render nested fields with increased indentation
    for nested_field in &field.nested_fields {
        render_field(lines, nested_field, indent_level + 1);
    }
}

/// Truncate a string to a maximum length, adding ellipsis if needed
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len.saturating_sub(1)])
    }
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
