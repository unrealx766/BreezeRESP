//! Redis Streams commands: browse metadata/entries/groups/PEL plus write
//! (XADD/XTRIM) and management (XDEL/XACK/XGROUP/XCLAIM) operations.

use crate::core::capability::{self, ServerCapability};
use crate::core::pool::AnyPool;
use crate::core::streams::{
    ConsumerGroup, ConsumerInfo, PendingEntry, StreamEntry, StreamInfo, StreamsCollector,
};
use crate::core::validate::{
    validate_connection_id, validate_key, validate_non_empty,
};
use crate::AppState;
use tauri::State;

/// Max ids per batch operation (XDEL / XACK / XCLAIM).
const MAX_BATCH_IDS: usize = 500;
/// Max field pairs per XADD.
const MAX_ADD_FIELDS: usize = 100;
/// Max length for a single identifier (group / consumer / entry id).
const MAX_ID_LEN: usize = 1024;
/// Max length for a single field name or value.
const MAX_FIELD_LEN: usize = 65_536;

fn pool_of(
    state: &State<'_, AppState>,
    connection_id: &str,
) -> Result<AnyPool, String> {
    let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
    pm.get_pool(connection_id)
}

/// Fetch the cached capability profile and verify Streams support.
async fn ensure_streams(
    pool: &AnyPool,
    connection_id: &str,
) -> Result<ServerCapability, String> {
    let cap = capability::get_or_probe(pool, connection_id, false).await?;
    if !cap.streams_supported {
        return Err(capability::unsupported_err(
            "Streams",
            &cap.redis_version,
            "5.0 or later",
        ));
    }
    Ok(cap)
}

fn validate_ids(ids: &[String]) -> Result<(), String> {
    if ids.is_empty() {
        return Err("ids must not be empty".to_string());
    }
    if ids.len() > MAX_BATCH_IDS {
        return Err(format!("too many ids (max {})", MAX_BATCH_IDS));
    }
    for id in ids {
        validate_non_empty(id, "entry id", MAX_ID_LEN)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Read-only commands
// ---------------------------------------------------------------------------

/// List stream keys (SCAN ... TYPE stream), with optional glob pattern.
#[tauri::command]
pub async fn list_streams(
    state: State<'_, AppState>,
    connection_id: String,
    pattern: Option<String>,
    limit: Option<u64>,
) -> Result<Vec<String>, String> {
    validate_connection_id(&connection_id)?;
    let pattern = pattern.filter(|p| !p.is_empty()).unwrap_or_else(|| "*".to_string());
    crate::core::validate::validate_pattern(&pattern)?;
    let limit = limit.unwrap_or(500).min(5000) as usize;

    let pool = pool_of(&state, &connection_id)?;
    let cap = ensure_streams(&pool, &connection_id).await?;
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    // SCAN TYPE filter requires Redis >= 6.0 (same threshold as XINFO FULL)
    let type_filter_supported = cap.stream_full_supported;
    let collector = StreamsCollector::new();
    if let Some(cluster) = conn.as_cluster() {
        return collector
            .list_streams_cluster(cluster, &pattern, limit, type_filter_supported)
            .await;
    }
    collector
        .list_streams(&mut conn, &pattern, limit, type_filter_supported)
        .await
}

/// Fetch stream metadata (FULL COUNT 10 form on Redis 7.0+, basic otherwise).
#[tauri::command]
pub async fn get_stream_info(
    state: State<'_, AppState>,
    connection_id: String,
    key: String,
) -> Result<StreamInfo, String> {
    validate_connection_id(&connection_id)?;
    validate_key(&key)?;

    let pool = pool_of(&state, &connection_id)?;
    let cap = ensure_streams(&pool, &connection_id).await?;
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    StreamsCollector::new()
        .get_stream_info(&mut conn, &key, cap.stream_full_count_supported)
        .await
}

/// Browse stream entries with XRANGE. Empty start/end default to `-` / `+`.
#[tauri::command]
pub async fn get_stream_entries(
    state: State<'_, AppState>,
    connection_id: String,
    key: String,
    start: Option<String>,
    end: Option<String>,
    count: Option<u64>,
) -> Result<Vec<StreamEntry>, String> {
    validate_connection_id(&connection_id)?;
    validate_key(&key)?;
    let start = start.unwrap_or_default();
    let end = end.unwrap_or_default();
    for s in [&start, &end] {
        if !s.is_empty() {
            validate_non_empty(s, "entry id bound", MAX_ID_LEN)?;
        }
    }
    let count = count.unwrap_or(50).min(1000);

    let pool = pool_of(&state, &connection_id)?;
    ensure_streams(&pool, &connection_id).await?;
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    StreamsCollector::new()
        .get_entries(&mut conn, &key, &start, &end, count)
        .await
}

/// List consumer groups of a stream.
#[tauri::command]
pub async fn get_stream_groups(
    state: State<'_, AppState>,
    connection_id: String,
    key: String,
) -> Result<Vec<ConsumerGroup>, String> {
    validate_connection_id(&connection_id)?;
    validate_key(&key)?;

    let pool = pool_of(&state, &connection_id)?;
    ensure_streams(&pool, &connection_id).await?;
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    StreamsCollector::new().get_groups(&mut conn, &key).await
}

/// List consumers inside a consumer group.
#[tauri::command]
pub async fn get_stream_consumers(
    state: State<'_, AppState>,
    connection_id: String,
    key: String,
    group: String,
) -> Result<Vec<ConsumerInfo>, String> {
    validate_connection_id(&connection_id)?;
    validate_key(&key)?;
    validate_non_empty(&group, "group", MAX_ID_LEN)?;

    let pool = pool_of(&state, &connection_id)?;
    ensure_streams(&pool, &connection_id).await?;
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    StreamsCollector::new()
        .get_consumers(&mut conn, &key, &group)
        .await
}

/// List pending entries (PEL) of a consumer group.
#[tauri::command]
pub async fn get_pending_entries(
    state: State<'_, AppState>,
    connection_id: String,
    key: String,
    group: String,
    count: Option<u64>,
) -> Result<Vec<PendingEntry>, String> {
    validate_connection_id(&connection_id)?;
    validate_key(&key)?;
    validate_non_empty(&group, "group", MAX_ID_LEN)?;
    let count = count.unwrap_or(100).min(1000);

    let pool = pool_of(&state, &connection_id)?;
    ensure_streams(&pool, &connection_id).await?;
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    StreamsCollector::new()
        .get_pending(&mut conn, &key, &group, count)
        .await
}

// ---------------------------------------------------------------------------
// Write commands
// ---------------------------------------------------------------------------

/// XADD a message. Returns the generated entry id.
#[tauri::command]
pub async fn stream_add_message(
    state: State<'_, AppState>,
    connection_id: String,
    key: String,
    id: Option<String>,
    fields: Vec<(String, String)>,
) -> Result<String, String> {
    validate_connection_id(&connection_id)?;
    validate_key(&key)?;
    let id = id.unwrap_or_default();
    if !id.is_empty() {
        validate_non_empty(&id, "entry id", MAX_ID_LEN)?;
    }
    if fields.is_empty() {
        return Err("fields must not be empty".to_string());
    }
    if fields.len() > MAX_ADD_FIELDS {
        return Err(format!("too many fields (max {})", MAX_ADD_FIELDS));
    }
    for (f, v) in &fields {
        validate_non_empty(f, "field name", MAX_FIELD_LEN)?;
        if v.len() > MAX_FIELD_LEN {
            return Err("field value exceeds maximum length".to_string());
        }
    }

    let pool = pool_of(&state, &connection_id)?;
    ensure_streams(&pool, &connection_id).await?;
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    StreamsCollector::new()
        .add_message(&mut conn, &key, &id, &fields)
        .await
}

/// XTRIM MAXLEN. Returns the number of removed entries.
#[tauri::command]
pub async fn stream_trim(
    state: State<'_, AppState>,
    connection_id: String,
    key: String,
    max_len: u64,
    approximate: Option<bool>,
) -> Result<u64, String> {
    validate_connection_id(&connection_id)?;
    validate_key(&key)?;

    let pool = pool_of(&state, &connection_id)?;
    ensure_streams(&pool, &connection_id).await?;
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    StreamsCollector::new()
        .trim(&mut conn, &key, max_len, approximate.unwrap_or(true))
        .await
}

// ---------------------------------------------------------------------------
// Management commands
// ---------------------------------------------------------------------------

/// XDEL entries by id. Returns the removed count.
#[tauri::command]
pub async fn stream_delete_entries(
    state: State<'_, AppState>,
    connection_id: String,
    key: String,
    ids: Vec<String>,
) -> Result<u64, String> {
    validate_connection_id(&connection_id)?;
    validate_key(&key)?;
    validate_ids(&ids)?;

    let pool = pool_of(&state, &connection_id)?;
    ensure_streams(&pool, &connection_id).await?;
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    StreamsCollector::new()
        .delete_entries(&mut conn, &key, &ids)
        .await
}

/// XACK pending entries. Returns the acknowledged count.
#[tauri::command]
pub async fn stream_ack(
    state: State<'_, AppState>,
    connection_id: String,
    key: String,
    group: String,
    ids: Vec<String>,
) -> Result<u64, String> {
    validate_connection_id(&connection_id)?;
    validate_key(&key)?;
    validate_non_empty(&group, "group", MAX_ID_LEN)?;
    validate_ids(&ids)?;

    let pool = pool_of(&state, &connection_id)?;
    ensure_streams(&pool, &connection_id).await?;
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    StreamsCollector::new().ack(&mut conn, &key, &group, &ids).await
}

/// XGROUP DELCONSUMER. Returns removed pending count.
#[tauri::command]
pub async fn stream_delete_consumer(
    state: State<'_, AppState>,
    connection_id: String,
    key: String,
    group: String,
    consumer: String,
) -> Result<u64, String> {
    validate_connection_id(&connection_id)?;
    validate_key(&key)?;
    validate_non_empty(&group, "group", MAX_ID_LEN)?;
    validate_non_empty(&consumer, "consumer", MAX_ID_LEN)?;

    let pool = pool_of(&state, &connection_id)?;
    ensure_streams(&pool, &connection_id).await?;
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    StreamsCollector::new()
        .delete_consumer(&mut conn, &key, &group, &consumer)
        .await
}

/// XGROUP DESTROY a consumer group.
#[tauri::command]
pub async fn stream_delete_group(
    state: State<'_, AppState>,
    connection_id: String,
    key: String,
    group: String,
) -> Result<bool, String> {
    validate_connection_id(&connection_id)?;
    validate_key(&key)?;
    validate_non_empty(&group, "group", MAX_ID_LEN)?;

    let pool = pool_of(&state, &connection_id)?;
    ensure_streams(&pool, &connection_id).await?;
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    StreamsCollector::new()
        .delete_group(&mut conn, &key, &group)
        .await?;
    Ok(true)
}

/// XCLAIM pending entries to another consumer. Returns the claimed entries.
#[tauri::command]
pub async fn stream_claim(
    state: State<'_, AppState>,
    connection_id: String,
    key: String,
    group: String,
    consumer: String,
    min_idle_ms: Option<u64>,
    ids: Vec<String>,
) -> Result<Vec<StreamEntry>, String> {
    validate_connection_id(&connection_id)?;
    validate_key(&key)?;
    validate_non_empty(&group, "group", MAX_ID_LEN)?;
    validate_non_empty(&consumer, "consumer", MAX_ID_LEN)?;
    validate_ids(&ids)?;

    let pool = pool_of(&state, &connection_id)?;
    ensure_streams(&pool, &connection_id).await?;
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    StreamsCollector::new()
        .claim(
            &mut conn,
            &key,
            &group,
            &consumer,
            min_idle_ms.unwrap_or(0),
            &ids,
        )
        .await
}
