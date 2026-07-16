//! Table and snapshot statistics derived from Iceberg metadata and storage.

use anyhow::{Context, Result};
use futures::stream::{self, StreamExt};
use iceberg::io::{FileIO, FileIOBuilder};
use iceberg::spec::{ManifestContentType, Struct};
use iceberg::table::Table;
use opendal::EntryMode;
use opendal::Operator;
use opendal::services::{FsConfig, S3Config};
use std::collections::{HashMap, HashSet};
use tracing::{debug, info, warn};

use crate::iceberg::catalog::storage_factory_for_uri;
#[derive(Debug, Clone)]
pub struct StatsProgress {
    /// Short phase name shown in the UI.
    pub phase: String,
    /// Optional `done/total` counters for the current phase.
    pub done: usize,
    pub total: Option<usize>,
    /// Best stats available so far.
    pub partial: TableStats,
}

/// Lazy-loaded table stats (I/O runs in a background task).
#[derive(Debug, Clone)]
pub enum LazyStats {
    /// Stats are being collected asynchronously.
    Loading(StatsProgress),
    /// Stats finished successfully.
    Ready(TableStats),
    /// Stats collection failed.
    Failed(String),
}

impl Default for LazyStats {
    fn default() -> Self {
        Self::Loading(StatsProgress {
            phase: "starting".into(),
            done: 0,
            total: None,
            partial: TableStats::default(),
        })
    }
}

impl LazyStats {
    pub fn as_ready(&self) -> Option<&TableStats> {
        match self {
            Self::Ready(stats) => Some(stats),
            _ => None,
        }
    }

    pub fn partial_stats(&self) -> Option<&TableStats> {
        match self {
            Self::Loading(p) => Some(&p.partial),
            Self::Ready(stats) => Some(stats),
            Self::Failed(_) => None,
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading(_))
    }

    pub fn progress(&self) -> Option<&StatsProgress> {
        match self {
            Self::Loading(p) => Some(p),
            _ => None,
        }
    }
}

/// Aggregate stats for a table (metadata history + current snapshot + orphans).
#[derive(Debug, Clone, Default)]
pub struct TableStats {
    /// Number of metadata JSON files (current + metadata_log).
    pub metadata_file_count: usize,
    /// Earliest metadata timestamp (ms), from metadata_log or last_updated.
    pub metadata_earliest_ms: Option<i64>,
    /// Latest metadata timestamp (ms).
    pub metadata_latest_ms: Option<i64>,
    /// Size of the current metadata JSON file in bytes.
    pub current_metadata_size: Option<u64>,
    /// Sum of sizes for all known metadata JSON files (best-effort).
    pub total_metadata_size: Option<u64>,
    /// Stats for the current snapshot (if any).
    pub current_snapshot: Option<SnapshotStats>,
    /// Orphan file stats (None if listing storage failed).
    pub orphans: Option<OrphanStats>,
    /// Set when storage I/O failed (credentials, listing, etc.).
    pub storage_error: Option<String>,
}

/// Stats for the current snapshot.
#[derive(Debug, Clone, Default)]
pub struct SnapshotStats {
    pub snapshot_id: i64,
    pub timestamp_ms: i64,
    pub operation: String,
    pub manifest_list_path: String,
    pub manifest_list_size: Option<u64>,
    pub manifest_count: usize,
    pub manifest_total_size: u64,
    /// Distinct partition values among live data files.
    pub partition_count: usize,
    pub total_data_files: Option<i64>,
    pub total_delete_files: Option<i64>,
    pub total_records: Option<i64>,
    pub total_files_size: Option<i64>,
    pub added_data_files: Option<i64>,
    pub deleted_data_files: Option<i64>,
    pub changed_partition_count: Option<i64>,
}

/// Breakdown of files under the table's `data/` and `metadata/` prefixes.
///
/// Sizes come from the storage listing (what is actually present), classified by
/// whether each path appears in the closure of all accessible snapshots.
#[derive(Debug, Clone, Default)]
pub struct OrphanStats {
    /// Unique data files on storage referenced by any snapshot.
    pub accessible_data_count: usize,
    pub accessible_data_size: u64,
    /// Files under `metadata/` on storage that are still referenced.
    pub accessible_metadata_count: usize,
    pub accessible_metadata_size: u64,
    /// Orphan metadata files (present but unreferenced).
    pub metadata_count: usize,
    pub metadata_size: u64,
    /// Orphan data files (present but unreferenced).
    pub data_count: usize,
    pub data_size: u64,
}

impl OrphanStats {
    /// Approximate total bytes under `data/` + `metadata/` for this table.
    pub fn total_on_storage(&self) -> u64 {
        self.accessible_data_size
            + self.accessible_metadata_size
            + self.data_size
            + self.metadata_size
    }
}

/// Sync stats available from already-loaded table metadata (no extra I/O).
pub fn collect_sync_stats(table: &Table) -> TableStats {
    let metadata = table.metadata();
    let metadata_log = metadata.metadata_log();
    let current_meta_loc = table.metadata_location();

    let mut earliest = metadata_log.iter().map(|e| e.timestamp_ms).min();
    let mut latest = metadata_log.iter().map(|e| e.timestamp_ms).max();
    let last_updated = metadata.last_updated_ms();
    earliest = match (earliest, Some(last_updated)) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    latest = match (latest, Some(last_updated)) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    };

    let mut stats = TableStats {
        metadata_file_count: metadata_log.len() + usize::from(current_meta_loc.is_some()),
        metadata_earliest_ms: earliest,
        metadata_latest_ms: latest,
        ..Default::default()
    };

    // Snapshot summary fields need no I/O
    if let Some(snapshot) = metadata.current_snapshot() {
        let summary = snapshot.summary();
        let mut summary_map: HashMap<String, String> = summary
            .additional_properties
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        summary_map.insert(
            "operation".to_string(),
            format!("{:?}", summary.operation),
        );

        stats.current_snapshot = Some(SnapshotStats {
            snapshot_id: snapshot.snapshot_id(),
            timestamp_ms: snapshot.timestamp_ms(),
            operation: summary_map
                .get("operation")
                .cloned()
                .unwrap_or_else(|| "unknown".to_string()),
            manifest_list_path: snapshot.manifest_list().to_string(),
            manifest_list_size: None,
            manifest_count: 0,
            manifest_total_size: 0,
            partition_count: 0,
            total_data_files: parse_i64(&summary_map, "total-data-files"),
            total_delete_files: parse_i64(&summary_map, "total-delete-files"),
            total_records: parse_i64(&summary_map, "total-records"),
            total_files_size: parse_i64(&summary_map, "total-files-size"),
            added_data_files: parse_i64(&summary_map, "added-data-files"),
            deleted_data_files: parse_i64(&summary_map, "deleted-data-files"),
            changed_partition_count: parse_i64(&summary_map, "changed-partition-count"),
        });
    }

    stats
}

/// Collect full table stats in phases, reporting progress after each step.
///
/// `storage_props` should be the loadTable `config` map (vended S3 credentials,
/// endpoint, region). These are overlaid onto FileIO so storage I/O works even
/// when the catalog's default FileIO fell back to a broken credential chain.
///
/// Phase order (fast → slow):
/// 1. Manifest-list read (counts/sizes from list entries; one potentially large Avro)
/// 2. Distinct partition count (load current-snapshot manifests in parallel)
/// 3. Orphan scan (all snapshots' manifests + storage list)
///
/// Metadata JSON sizes come from the storage listing in phase 3 (no per-file HEAD).
pub async fn collect_table_stats(
    table: &Table,
    storage_props: &HashMap<String, String>,
    mut on_progress: impl FnMut(StatsProgress),
) -> TableStats {
    let metadata = table.metadata();
    let mut stats = collect_sync_stats(table);
    let started = std::time::Instant::now();

    let file_io = match resolve_file_io(table, storage_props) {
        Ok(io) => io,
        Err(e) => {
            let msg = format!("Storage I/O unavailable: {e:#}");
            warn!("{}", msg);
            stats.storage_error = Some(msg);
            return stats;
        }
    };

    let emit = |on_progress: &mut dyn FnMut(StatsProgress),
                phase: &str,
                done: usize,
                total: Option<usize>,
                partial: &TableStats| {
        on_progress(StatsProgress {
            phase: phase.to_string(),
            done,
            total,
            partial: partial.clone(),
        });
    };

    emit(&mut on_progress, "manifest list", 0, Some(1), &stats);

    // --- Phase 1: manifest list (one read) + current metadata HEAD ---
    let phase1 = std::time::Instant::now();
    if let Some(snapshot) = metadata.current_snapshot() {
        match enrich_snapshot_from_manifest_list(table, &file_io, snapshot.as_ref(), &mut stats)
            .await
        {
            Ok(()) => {}
            Err(e) => {
                let msg = format!("Failed to load manifest list: {e:#}");
                warn!("{}", msg);
                if is_credential_error(&e) {
                    stats.storage_error = Some(credential_error_hint(&e));
                } else {
                    stats.storage_error = Some(msg);
                }
            }
        }
    }

    if let Some(loc) = table.metadata_location() {
        match file_size(&file_io, loc).await {
            Ok(size) => stats.current_metadata_size = Some(size),
            Err(e) => {
                warn!("Failed to HEAD current metadata.json: {:#}", e);
                if stats.storage_error.is_none() && is_credential_error(&e) {
                    stats.storage_error = Some(credential_error_hint(&e));
                }
            }
        }
    }

    emit(&mut on_progress, "manifest list", 1, Some(1), &stats);
    info!(
        "Stats phase 1 done in {:?}: {} manifests, manifest list size={:?}, current metadata size={:?}",
        phase1.elapsed(),
        stats
            .current_snapshot
            .as_ref()
            .map(|s| s.manifest_count)
            .unwrap_or(0),
        stats
            .current_snapshot
            .as_ref()
            .and_then(|s| s.manifest_list_size),
        stats.current_metadata_size
    );

    // Credential failures will never recover mid-scan — skip the expensive crawl.
    if stats
        .storage_error
        .as_ref()
        .is_some_and(|e| e.contains("S3 credentials") || e.contains("IMDS"))
    {
        warn!(
            "Aborting stats after phase 1 due to storage credentials: {}",
            stats.storage_error.as_deref().unwrap_or("")
        );
        emit(&mut on_progress, "done", 1, Some(1), &stats);
        return stats;
    }

    // --- Phase 2: partition cardinality from current snapshot manifests ---
    if let Some(snapshot) = metadata.current_snapshot() {
        let manifest_count = stats
            .current_snapshot
            .as_ref()
            .map(|s| s.manifest_count)
            .unwrap_or(0);
        emit(
            &mut on_progress,
            "partitions",
            0,
            Some(manifest_count),
            &stats,
        );

        let phase2 = std::time::Instant::now();
        match count_partitions_parallel(table, &file_io, snapshot.as_ref(), |done, total| {
            if done % 5 == 0 || done == total {
                emit(
                    &mut on_progress,
                    "partitions",
                    done,
                    Some(total),
                    &stats,
                );
            }
        })
        .await
        {
            Ok(count) => {
                if let Some(snap) = stats.current_snapshot.as_mut() {
                    snap.partition_count = count;
                }
            }
            Err(e) => warn!("Failed to count partitions: {}", e),
        }
        emit(
            &mut on_progress,
            "partitions",
            manifest_count,
            Some(manifest_count),
            &stats,
        );
        info!(
            "Stats phase 2 done in {:?}: {} partitions across {} manifests",
            phase2.elapsed(),
            stats
                .current_snapshot
                .as_ref()
                .map(|s| s.partition_count)
                .unwrap_or(0),
            manifest_count
        );
    }

    // --- Phase 3: orphan scan (also yields metadata file sizes from listing) ---
    emit(
        &mut on_progress,
        "orphans (lists)",
        0,
        None,
        &stats,
    );

    let phase3 = std::time::Instant::now();
    match collect_orphan_stats(table, &file_io, |done, total, phase| {
        emit(&mut on_progress, phase, done, Some(total), &stats);
    })
    .await
    {
        Ok((orphans, meta_sizes)) => {
            stats.orphans = Some(orphans);
            if let Some(current) = table.metadata_location() {
                let norm_current = normalize_path(current);
                if let Some((_, size)) = meta_sizes.iter().find(|(p, _)| normalize_path(p) == norm_current)
                {
                    stats.current_metadata_size = Some(*size);
                }
            }
            if !meta_sizes.is_empty() {
                stats.total_metadata_size = Some(meta_sizes.iter().map(|(_, s)| s).sum());
            }
            info!(
                "Stats phase 3 done in {:?}: accessible data {} ({}), orphan data {} ({}), total on storage {}",
                phase3.elapsed(),
                stats.orphans.as_ref().map(|o| o.accessible_data_count).unwrap_or(0),
                stats
                    .orphans
                    .as_ref()
                    .map(|o| format_bytes(o.accessible_data_size))
                    .unwrap_or_else(|| "0 B".into()),
                stats.orphans.as_ref().map(|o| o.data_count).unwrap_or(0),
                stats
                    .orphans
                    .as_ref()
                    .map(|o| format_bytes(o.data_size))
                    .unwrap_or_else(|| "0 B".into()),
                stats
                    .orphans
                    .as_ref()
                    .map(|o| format_bytes(o.total_on_storage()))
                    .unwrap_or_else(|| "0 B".into()),
            );
        }
        Err(e) => {
            warn!("Failed to collect orphan stats: {:#}", e);
            stats.orphans = None;
            if stats.storage_error.is_none() {
                stats.storage_error = Some(if is_credential_error(&e) {
                    credential_error_hint(&e)
                } else {
                    format!("{e:#}")
                });
            }
        }
    }

    emit(&mut on_progress, "done", 1, Some(1), &stats);
    info!("Stats complete in {:?}", started.elapsed());
    stats
}

/// Rebuild FileIO with loadTable storage config + env AWS creds, and never use IMDS.
fn resolve_file_io(table: &Table, storage_props: &HashMap<String, String>) -> Result<FileIO> {
    let location = table.metadata().location();
    let mut props = table.file_io().config().props().clone();

    for (k, v) in storage_props {
        if k.starts_with("s3.") || k.starts_with("client.") || k == "region" {
            props.insert(k.clone(), v.clone());
        }
    }
    apply_env_s3_creds(&mut props);
    props.insert("s3.disable-ec2-metadata".to_string(), "true".to_string());

    let mut keys: Vec<_> = props.keys().cloned().collect();
    keys.sort();
    info!(
        "Resolved FileIO for {}: has_access_key={} prop_keys={:?}",
        truncate_for_log(location, 60),
        props.contains_key("s3.access-key-id"),
        keys
    );

    Ok(FileIOBuilder::new(storage_factory_for_uri(Some(location)))
        .with_props(props)
        .build())
}

fn apply_env_s3_creds(props: &mut HashMap<String, String>) {
    if let Ok(v) = std::env::var("AWS_ACCESS_KEY_ID") {
        props
            .entry("s3.access-key-id".to_string())
            .or_insert(v);
    }
    if let Ok(v) = std::env::var("AWS_SECRET_ACCESS_KEY") {
        props
            .entry("s3.secret-access-key".to_string())
            .or_insert(v);
    }
    if let Ok(v) = std::env::var("AWS_SESSION_TOKEN") {
        props
            .entry("s3.session-token".to_string())
            .or_insert(v);
    }
    if let Ok(v) = std::env::var("AWS_REGION").or_else(|_| std::env::var("AWS_DEFAULT_REGION")) {
        props.entry("s3.region".to_string()).or_insert(v);
    }
    if let Ok(v) = std::env::var("AWS_ENDPOINT_URL") {
        props.entry("s3.endpoint".to_string()).or_insert(v);
    }
}

fn is_credential_error(err: &anyhow::Error) -> bool {
    let msg = format!("{err:#}");
    msg.contains("LoadCredential")
        || msg.contains("169.254.169.254")
        || msg.contains("credential")
        || msg.contains("Credentials")
}

fn credential_error_hint(err: &anyhow::Error) -> String {
    format!(
        "S3 credentials unavailable (refused IMDS). Set AWS_ACCESS_KEY_ID/AWS_SECRET_ACCESS_KEY \
         or catalog props s3.access-key-id/s3.secret-access-key, or ensure the catalog vends \
         temporary credentials in loadTable config. Detail: {err:#}"
    )
}

/// Load manifest list only — fills counts/sizes without reading each manifest.
async fn enrich_snapshot_from_manifest_list(
    table: &Table,
    file_io: &FileIO,
    snapshot: &iceberg::spec::Snapshot,
    stats: &mut TableStats,
) -> Result<()> {
    let metadata = table.metadata();

    let Some(snap_stats) = stats.current_snapshot.as_mut() else {
        return Ok(());
    };

    let manifest_list_path = snapshot.manifest_list().to_string();
    snap_stats.manifest_list_path = manifest_list_path.clone();

    let t_head = std::time::Instant::now();
    snap_stats.manifest_list_size = file_size(file_io, &manifest_list_path).await.ok();
    debug!(
        "Manifest list HEAD {:?} → {:?}",
        t_head.elapsed(),
        snap_stats.manifest_list_size.map(format_bytes)
    );

    let t_load = std::time::Instant::now();
    let manifest_list = snapshot
        .load_manifest_list(file_io, metadata)
        .await
        .context("Failed to load manifest list")?;
    info!(
        "Manifest list loaded in {:?} ({} entries, path={})",
        t_load.elapsed(),
        manifest_list.entries().len(),
        truncate_for_log(&manifest_list_path, 80)
    );

    let manifests = manifest_list.entries();
    snap_stats.manifest_count = manifests.len();
    snap_stats.manifest_total_size = manifests
        .iter()
        .map(|m| m.manifest_length.max(0) as u64)
        .sum();

    Ok(())
}

fn truncate_for_log(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}

/// Count distinct live partitions by loading current-snapshot manifests in parallel.
async fn count_partitions_parallel(
    table: &Table,
    file_io: &FileIO,
    snapshot: &iceberg::spec::Snapshot,
    mut on_tick: impl FnMut(usize, usize),
) -> Result<usize> {
    let metadata = table.metadata();

    let manifest_list = snapshot
        .load_manifest_list(file_io, metadata)
        .await
        .context("Failed to load manifest list for partitions")?;

    let manifests: Vec<_> = manifest_list
        .entries()
        .iter()
        .filter(|m| {
            m.content == ManifestContentType::Data || m.content == ManifestContentType::Deletes
        })
        .cloned()
        .collect();
    let total = manifests.len();
    on_tick(0, total);

    let file_io = file_io.clone();
    let mut partitions: HashSet<Struct> = HashSet::new();
    let mut done = 0usize;

    let mut stream = stream::iter(manifests.into_iter().map(|mf| {
        let file_io = file_io.clone();
        async move {
            let path = mf.manifest_path.clone();
            match mf.load_manifest(&file_io).await {
                Ok(manifest) => {
                    let parts: Vec<Struct> = manifest
                        .entries()
                        .iter()
                        .filter(|e| e.is_alive())
                        .map(|e| e.data_file().partition().clone())
                        .collect();
                    Ok(parts)
                }
                Err(e) => {
                    warn!("Failed to load manifest {}: {}", path, e);
                    Err(e)
                }
            }
        }
    }))
    .buffer_unordered(32);

    while let Some(result) = stream.next().await {
        done += 1;
        if let Ok(parts) = result {
            partitions.extend(parts);
        }
        if done % 5 == 0 || done == total {
            on_tick(done, total);
        }
    }

    Ok(partitions.len())
}

/// Orphan detection + metadata size harvest from the storage listing.
///
/// Strategy (parallelized — sequential-per-snapshot was ~7s/snapshot):
/// 1. Deduplicate manifest-list paths across all snapshots
/// 2. Load unique manifest lists concurrently
/// 3. Load unique manifests concurrently (this is the bulk of the I/O)
/// 4. List `data/` + `metadata/` and diff
///
/// Returns `(orphan_stats, metadata_json_sizes)`.
async fn collect_orphan_stats(
    table: &Table,
    file_io: &FileIO,
    mut on_tick: impl FnMut(usize, usize, &str),
) -> Result<(OrphanStats, Vec<(String, u64)>)> {
    let metadata = table.metadata();
    let location = metadata.location().to_string();

    let mut referenced: HashSet<String> = HashSet::new();

    if let Some(loc) = table.metadata_location() {
        referenced.insert(normalize_path(loc));
    }
    for entry in metadata.metadata_log() {
        referenced.insert(normalize_path(&entry.metadata_file));
    }

    // Deduplicate: one snapshot per unique manifest-list path
    let mut list_owners: HashMap<String, iceberg::spec::SnapshotRef> = HashMap::new();
    for snap in metadata.snapshots() {
        let path = snap.manifest_list().to_string();
        referenced.insert(normalize_path(&path));
        list_owners.entry(path).or_insert_with(|| snap.clone());
    }

    let list_jobs: Vec<_> = list_owners.into_iter().collect();
    let total_lists = list_jobs.len();
    info!(
        "Orphan scan: {} snapshots → {} unique manifest lists",
        metadata.snapshots().len(),
        total_lists
    );
    on_tick(0, total_lists, "orphans (lists)");

    let t_lists = std::time::Instant::now();
    let mut unique_manifests: HashMap<String, iceberg::spec::ManifestFile> = HashMap::new();
    let mut lists_done = 0usize;
    let mut lists_ok = 0usize;
    let mut first_cred_err: Option<String> = None;

    {
        let mut stream = stream::iter(list_jobs.into_iter().map(|(path, snap)| {
            let file_io = file_io.clone();
            let table = table.clone();
            async move {
                let result = snap
                    .load_manifest_list(&file_io, table.metadata())
                    .await;
                (path, result)
            }
        }))
        .buffer_unordered(24);

        while let Some((path, result)) = stream.next().await {
            lists_done += 1;
            match result {
                Ok(manifest_list) => {
                    lists_ok += 1;
                    for mf in manifest_list.entries() {
                        referenced.insert(normalize_path(&mf.manifest_path));
                        unique_manifests
                            .entry(mf.manifest_path.clone())
                            .or_insert_with(|| mf.clone());
                    }
                }
                Err(e) => {
                    let err = anyhow::anyhow!("{e}");
                    if is_credential_error(&err) && first_cred_err.is_none() {
                        first_cred_err = Some(credential_error_hint(&err));
                    }
                    warn!("Orphan scan: failed to load manifest list {}: {}", path, e);
                }
            }
            // Fail fast: if the first batch all failed on credentials, don't burn minutes.
            if lists_done >= 8.min(total_lists) && lists_ok == 0 {
                if let Some(msg) = first_cred_err {
                    anyhow::bail!(msg);
                }
                anyhow::bail!(
                    "Failed to load any of the first {lists_done} manifest lists from storage"
                );
            }
            if lists_done % 2 == 0 || lists_done == total_lists {
                on_tick(lists_done, total_lists, "orphans (lists)");
            }
        }
    }

    if lists_ok == 0 && total_lists > 0 {
        if let Some(msg) = first_cred_err {
            anyhow::bail!(msg);
        }
        anyhow::bail!("Failed to load any manifest lists from storage");
    }

    let manifests: Vec<_> = unique_manifests.into_values().collect();
    let total_manifests = manifests.len();
    info!(
        "Orphan scan: loaded {}/{} manifest lists in {:?} → {} unique manifests",
        lists_ok,
        total_lists,
        t_lists.elapsed(),
        total_manifests
    );
    on_tick(0, total_manifests, "orphans (manifests)");

    let t_manifests = std::time::Instant::now();
    let mut manifests_done = 0usize;

    {
        let mut stream = stream::iter(manifests.into_iter().map(|mf| {
            let file_io = file_io.clone();
            async move {
                let path = mf.manifest_path.clone();
                let result = mf.load_manifest(&file_io).await;
                (path, result)
            }
        }))
        .buffer_unordered(48);

        while let Some((path, result)) = stream.next().await {
            manifests_done += 1;
            match result {
                Ok(manifest) => {
                    for entry in manifest.entries() {
                        referenced.insert(normalize_path(entry.data_file().file_path()));
                    }
                }
                Err(e) => {
                    warn!("Orphan scan: failed to load manifest {}: {}", path, e);
                }
            }
            if manifests_done % 10 == 0 || manifests_done == total_manifests {
                on_tick(manifests_done, total_manifests, "orphans (manifests)");
            }
        }
    }

    info!(
        "Orphan scan: loaded {} manifests in {:?} → {} referenced paths",
        total_manifests,
        t_manifests.elapsed(),
        referenced.len()
    );

    let data_prefix = join_location(&location, "data");
    let meta_prefix = join_location(&location, "metadata");

    build_operator_for_path(file_io, &location, &meta_prefix)
        .context("Storage listing not available for this table location")?;

    on_tick(0, 2, "orphans (list)");
    info!("Orphan scan: listing {}", data_prefix);
    let t_data = std::time::Instant::now();
    let listed_data = list_files(file_io, &location, &data_prefix)
        .await
        .with_context(|| format!("Failed to list {}", data_prefix))?;
    info!(
        "Orphan scan: listed {} data files in {:?}",
        listed_data.len(),
        t_data.elapsed()
    );
    on_tick(1, 2, "orphans (list)");

    info!("Orphan scan: listing {}", meta_prefix);
    let t_meta = std::time::Instant::now();
    let listed_metadata = list_files(file_io, &location, &meta_prefix)
        .await
        .with_context(|| format!("Failed to list {}", meta_prefix))?;
    info!(
        "Orphan scan: listed {} metadata files in {:?}",
        listed_metadata.len(),
        t_meta.elapsed()
    );
    on_tick(2, 2, "orphans (list)");

    let meta_sizes: Vec<(String, u64)> = listed_metadata
        .iter()
        .filter(|(path, _)| path.contains(".metadata.json"))
        .cloned()
        .collect();

    let mut orphans = OrphanStats::default();

    for (path, size) in &listed_data {
        let norm = normalize_path(path);
        if referenced.contains(&norm) {
            orphans.accessible_data_count += 1;
            orphans.accessible_data_size += *size;
        } else {
            orphans.data_count += 1;
            orphans.data_size += *size;
        }
    }

    for (path, size) in &listed_metadata {
        let norm = normalize_path(path);
        if is_expected_non_manifest_metadata(&norm) {
            continue;
        }
        if referenced.contains(&norm) {
            orphans.accessible_metadata_count += 1;
            orphans.accessible_metadata_size += *size;
        } else {
            orphans.metadata_count += 1;
            orphans.metadata_size += *size;
        }
    }

    info!(
        "Storage breakdown: accessible data {} ({}), accessible metadata {} ({}), \
         orphan data {} ({}), orphan metadata {} ({}), total {}",
        orphans.accessible_data_count,
        format_bytes(orphans.accessible_data_size),
        orphans.accessible_metadata_count,
        format_bytes(orphans.accessible_metadata_size),
        orphans.data_count,
        format_bytes(orphans.data_size),
        orphans.metadata_count,
        format_bytes(orphans.metadata_size),
        format_bytes(orphans.total_on_storage()),
    );

    Ok((orphans, meta_sizes))
}

/// Files under metadata/ that are legitimate but never appear in manifests.
fn is_expected_non_manifest_metadata(path: &str) -> bool {
    path.ends_with("version-hint.text") || path.ends_with('/')
}

async fn file_size(file_io: &FileIO, path: &str) -> Result<u64> {
    let input = file_io
        .new_input(path)
        .with_context(|| format!("Failed to open {}", path))?;
    let meta = input
        .metadata()
        .await
        .with_context(|| format!("Failed to stat {}", path))?;
    Ok(meta.size)
}

fn parse_i64(map: &HashMap<String, String>, key: &str) -> Option<i64> {
    map.get(key).and_then(|s| s.parse().ok())
}

fn normalize_path(path: &str) -> String {
    // Normalize schemes and strip trailing slash for set comparison
    let p = path.trim_end_matches('/');
    if let Some(rest) = p.strip_prefix("s3a://") {
        format!("s3://{}", rest)
    } else if let Some(rest) = p.strip_prefix("s3n://") {
        format!("s3://{}", rest)
    } else if let Some(rest) = p.strip_prefix("file://") {
        format!("file:///{}", rest.trim_start_matches('/'))
    } else if p.starts_with('/') {
        format!("file://{}", p)
    } else {
        p.to_string()
    }
}

fn join_location(base: &str, child: &str) -> String {
    format!("{}/{}", base.trim_end_matches('/'), child)
}

/// List files under `prefix` using an Operator rebuilt from the table's FileIO config.
async fn list_files(
    file_io: &FileIO,
    table_location: &str,
    prefix: &str,
) -> Result<Vec<(String, u64)>> {
    let (op, relative_prefix) = build_operator_for_path(file_io, table_location, prefix)?;

    let list_path = if relative_prefix.ends_with('/') {
        relative_prefix.clone()
    } else {
        format!("{}/", relative_prefix)
    };

    debug!("Listing storage path: {} (relative {})", prefix, list_path);

    let entries = op
        .list_with(&list_path)
        .recursive(true)
        .await
        .with_context(|| format!("opendal list failed for {}", list_path))?;

    let scheme = scheme_of(table_location);
    let mut files = Vec::new();
    for entry in entries {
        if entry.metadata().mode() != EntryMode::FILE {
            continue;
        }
        let rel = entry.path();
        let full_path = absolute_from_relative(&scheme, table_location, rel);
        let size = entry.metadata().content_length();
        files.push((full_path, size));
    }

    Ok(files)
}

fn absolute_from_relative(scheme: &str, table_location: &str, relative: &str) -> String {
    match scheme {
        "s3" | "s3a" | "s3n" => {
            let bucket = parse_location(table_location)
                .map(|(_, auth, _)| auth)
                .unwrap_or_default();
            format!(
                "s3://{}/{}",
                bucket,
                relative.trim_start_matches('/')
            )
        }
        "file" | "fs" | "" => {
            let path = if relative.starts_with('/') {
                relative.to_string()
            } else {
                format!("/{}", relative)
            };
            format!("file://{}", path)
        }
        other => format!(
            "{}://{}",
            other,
            relative.trim_start_matches('/')
        ),
    }
}

fn scheme_of(location: &str) -> String {
    location
        .split("://")
        .next()
        .unwrap_or("")
        .to_string()
}

/// Parse `scheme://authority/path` into (scheme, authority/bucket, path).
fn parse_location(location: &str) -> Option<(String, String, String)> {
    let (scheme, rest) = location.split_once("://")?;
    if scheme == "file" || scheme.is_empty() {
        let path = rest.to_string();
        return Some((scheme.to_string(), String::new(), path));
    }
    let (authority, path) = match rest.split_once('/') {
        Some((auth, p)) => (auth.to_string(), p.to_string()),
        None => (rest.to_string(), String::new()),
    };
    Some((scheme.to_string(), authority, path))
}

/// Build an opendal Operator for listing and the relative prefix path within it.
fn build_operator_for_path(
    file_io: &FileIO,
    table_location: &str,
    prefix: &str,
) -> Result<(Operator, String)> {
    let props = file_io.config().props().clone();
    let scheme = scheme_of(table_location);

    match scheme.as_str() {
        "s3" | "s3a" | "s3n" => {
            let (bucket, relative) = s3_bucket_and_key(prefix)
                .with_context(|| format!("Invalid S3 path: {}", prefix))?;
            let mut cfg = s3_config_from_props(props);
            cfg.bucket = bucket;
            let op = Operator::from_config(cfg).context("Failed to build S3 operator")?;
            Ok((op, relative))
        }
        "file" | "fs" | "" => {
            let mut cfg = FsConfig::default();
            cfg.root = Some("/".to_string());
            let op = Operator::from_config(cfg).context("Failed to build FS operator")?;
            let relative = strip_file_scheme(prefix)
                .trim_start_matches('/')
                .to_string();
            Ok((op, relative))
        }
        other => anyhow::bail!("Storage listing not supported for scheme: {}", other),
    }
}

fn strip_file_scheme(path: &str) -> String {
    path.strip_prefix("file://")
        .or_else(|| path.strip_prefix("file:/"))
        .unwrap_or(path)
        .to_string()
}

fn s3_bucket_and_key(path: &str) -> Option<(String, String)> {
    let rest = path
        .strip_prefix("s3://")
        .or_else(|| path.strip_prefix("s3a://"))
        .or_else(|| path.strip_prefix("s3n://"))?;
    let (bucket, key) = rest.split_once('/')?;
    Some((bucket.to_string(), key.to_string()))
}

fn s3_config_from_props(mut props: HashMap<String, String>) -> S3Config {
    apply_env_s3_creds(&mut props);
    let mut cfg = S3Config::default();

    if let Some(v) = props.remove("s3.endpoint") {
        cfg.endpoint = Some(v);
    }
    if let Some(v) = props.remove("s3.access-key-id") {
        cfg.access_key_id = Some(v);
    }
    if let Some(v) = props.remove("s3.secret-access-key") {
        cfg.secret_access_key = Some(v);
    }
    if let Some(v) = props.remove("s3.session-token") {
        cfg.session_token = Some(v);
    }
    if let Some(v) = props.remove("s3.region").or_else(|| props.remove("client.region")) {
        cfg.region = Some(v);
    }
    if let Some(v) = props.remove("s3.path-style-access") {
        let truthy = ["true", "t", "1", "on"].contains(&v.to_lowercase().as_str());
        cfg.enable_virtual_host_style = !truthy;
    }
    if let Some(v) = props.remove("s3.allow-anonymous") {
        if ["true", "t", "1", "on"].contains(&v.to_lowercase().as_str()) {
            cfg.skip_signature = true;
        }
    }
    if let Some(v) = props.remove("s3.disable-ec2-metadata") {
        if ["true", "t", "1", "on"].contains(&v.to_lowercase().as_str()) {
            cfg.disable_ec2_metadata = true;
        }
    }
    if let Some(v) = props.remove("s3.disable-config-load") {
        if ["true", "t", "1", "on"].contains(&v.to_lowercase().as_str()) {
            cfg.disable_config_load = true;
        }
    }

    // IceTea is a local TUI — never probe EC2 instance metadata (hangs for minutes).
    cfg.disable_ec2_metadata = true;

    debug!(
        "S3 list operator: endpoint={:?} region={:?} has_keys={} path_style={}",
        cfg.endpoint,
        cfg.region,
        cfg.access_key_id.is_some(),
        !cfg.enable_virtual_host_style
    );

    cfg
}

/// Format a byte count as a human-readable string.
pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;

    let b = bytes as f64;
    if b >= TB {
        format!("{:.2} TB", b / TB)
    } else if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.2} MB", b / MB)
    } else if b >= KB {
        format!("{:.2} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_bytes_scales() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(2048), "2.00 KB");
        assert_eq!(format_bytes(5 * 1024 * 1024), "5.00 MB");
    }

    #[test]
    fn normalize_path_unifies_schemes() {
        assert_eq!(
            normalize_path("s3a://bucket/path/"),
            "s3://bucket/path"
        );
        assert_eq!(
            normalize_path("file:///tmp/table/data/file.parquet"),
            "file:///tmp/table/data/file.parquet"
        );
        assert_eq!(
            normalize_path("/tmp/table/data/file.parquet"),
            "file:///tmp/table/data/file.parquet"
        );
    }
}
