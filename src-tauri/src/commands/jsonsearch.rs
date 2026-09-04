//! RedisJSON & RediSearch commands. Every command verifies module support
//! via the capability profile first so unsupported servers get friendly errors.

use crate::core::capability::{self, ServerCapability};
use crate::core::jsonsearch::{
    FtCreateSpec, FtIndexInfo, FtSearchResult, JsonSearchCollector,
};
use crate::core::pool::AnyPool;
use crate::core::validate::{validate_connection_id, validate_key, validate_non_empty};
use redis::cluster_routing::{MultipleNodeRoutingInfo, RoutingInfo};
use crate::AppState;
use tauri::State;

/// Max length for index names / JSON paths.
const MAX_NAME_LEN: usize = 1024;
/// Max length for an FT.SEARCH query string.
const MAX_QUERY_LEN: usize = 65_536;
/// Max PARAMS pairs per FT.SEARCH.
const MAX_SEARCH_PARAMS: usize = 64;
/// Max JSON value size accepted from the frontend.
const MAX_JSON_LEN: usize = 10_485_760; // 10 MB

fn pool_of(
    state: &State<'_, AppState>,
    connection_id: &str,
) -> Result<AnyPool, String> {
    let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
    pm.get_pool(connection_id)
}

async fn ensure_json(
    pool: &AnyPool,
    connection_id: &str,
) -> Result<ServerCapability, String> {
    let cap = capability::get_or_probe(pool, connection_id, false).await?;
    if !cap.json_supported {
        return Err(
            "RedisJSON module is not installed on this server. Install the RedisJSON module to manage JSON keys.".to_string(),
        );
    }
    Ok(cap)
}

async fn ensure_search(
    pool: &AnyPool,
    connection_id: &str,
) -> Result<ServerCapability, String> {
    let cap = capability::get_or_probe(pool, connection_id, false).await?;
    if !cap.search_supported {
        return Err(
            "RediSearch module is not installed on this server. Install the RediSearch module to use search features.".to_string(),
        );
    }
    Ok(cap)
}

// ---------------------------------------------------------------------------
// RedisJSON
// ---------------------------------------------------------------------------

/// JSON.GET key [path]; returns the raw JSON string.
#[tauri::command]
pub async fn json_get(
    state: State<'_, AppState>,
    connection_id: String,
    key: String,
    path: Option<String>,
) -> Result<String, String> {
    validate_connection_id(&connection_id)?;
    validate_key(&key)?;
    let path = path.unwrap_or_else(|| "$".to_string());
    validate_non_empty(&path, "path", MAX_NAME_LEN)?;

    let pool = pool_of(&state, &connection_id)?;
    ensure_json(&pool, &connection_id).await?;
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    JsonSearchCollector::new().json_get(&mut conn, &key, &path).await
}

/// JSON.SET key path value. The value must be valid JSON.
#[tauri::command]
pub async fn json_set(
    state: State<'_, AppState>,
    connection_id: String,
    key: String,
    path: String,
    value: String,
) -> Result<bool, String> {
    validate_connection_id(&connection_id)?;
    validate_key(&key)?;
    validate_non_empty(&path, "path", MAX_NAME_LEN)?;
    if value.len() > MAX_JSON_LEN {
        return Err("JSON value exceeds maximum size".to_string());
    }
    // Reject malformed JSON before hitting the server.
    serde_json::from_str::<serde_json::Value>(&value)
        .map_err(|e| format!("Invalid JSON value: {}", e))?;

    let pool = pool_of(&state, &connection_id)?;
    ensure_json(&pool, &connection_id).await?;
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    JsonSearchCollector::new()
        .json_set(&mut conn, &key, &path, &value)
        .await?;
    Ok(true)
}

/// JSON.DEL key [path]; returns the number of removed paths.
#[tauri::command]
pub async fn json_del(
    state: State<'_, AppState>,
    connection_id: String,
    key: String,
    path: Option<String>,
) -> Result<i64, String> {
    validate_connection_id(&connection_id)?;
    validate_key(&key)?;
    let path = path.unwrap_or_else(|| "$".to_string());
    validate_non_empty(&path, "path", MAX_NAME_LEN)?;

    let pool = pool_of(&state, &connection_id)?;
    ensure_json(&pool, &connection_id).await?;
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    JsonSearchCollector::new().json_del(&mut conn, &key, &path).await
}

/// JSON.TYPE key [path].
#[tauri::command]
pub async fn json_type(
    state: State<'_, AppState>,
    connection_id: String,
    key: String,
    path: Option<String>,
) -> Result<String, String> {
    validate_connection_id(&connection_id)?;
    validate_key(&key)?;
    let path = path.unwrap_or_else(|| "$".to_string());
    validate_non_empty(&path, "path", MAX_NAME_LEN)?;

    let pool = pool_of(&state, &connection_id)?;
    ensure_json(&pool, &connection_id).await?;
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    JsonSearchCollector::new()
        .json_type(&mut conn, &key, &path)
        .await
}

// ---------------------------------------------------------------------------
// RediSearch
// ---------------------------------------------------------------------------

/// FT._LIST → all index names.
/// In cluster mode the command has no key argument, so redis-rs would route
/// it to a random node (possibly a replica that may not serve search metadata).
/// We explicitly fan out to all masters and merge the results.
#[tauri::command]
pub async fn ft_list(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<String>, String> {
    validate_connection_id(&connection_id)?;

    let pool = pool_of(&state, &connection_id)?;
    ensure_search(&pool, &connection_id).await?;
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    // Cluster: route FT._LIST to all masters to avoid hitting replicas.
    if let Some(cluster) = conn.as_cluster() {
        let val = cluster
            .route_command(
                &redis::cmd("FT._LIST"),
                RoutingInfo::MultiNode((MultipleNodeRoutingInfo::AllMasters, None)),
            )
            .await
            .map_err(|e| format!("FT._LIST error: {}", e))?;
        let mut indexes: Vec<String> = Vec::new();
        if let redis::Value::Map(entries) = val {
            for (_, node_val) in entries {
                if let Ok(names) = redis::from_redis_value::<Vec<String>>(&node_val) {
                    indexes.extend(names);
                }
            }
        }
        indexes.sort();
        indexes.dedup();
        return Ok(indexes);
    }

    JsonSearchCollector::new().ft_list(&mut conn).await
}

/// FT.INFO index → parsed summary with field definitions.
#[tauri::command]
pub async fn ft_info(
    state: State<'_, AppState>,
    connection_id: String,
    index: String,
) -> Result<FtIndexInfo, String> {
    validate_connection_id(&connection_id)?;
    validate_non_empty(&index, "index", MAX_NAME_LEN)?;

    let pool = pool_of(&state, &connection_id)?;
    ensure_search(&pool, &connection_id).await?;
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    JsonSearchCollector::new().ft_info(&mut conn, &index).await
}

/// FT.SEARCH with optional LIMIT and PARAMS (for KNN vector queries).
#[tauri::command]
pub async fn ft_search(
    state: State<'_, AppState>,
    connection_id: String,
    index: String,
    query: String,
    offset: Option<u64>,
    limit: Option<u64>,
    params: Option<Vec<(String, Vec<u8>)>>,
    with_scores: Option<bool>,
) -> Result<FtSearchResult, String> {
    validate_connection_id(&connection_id)?;
    validate_non_empty(&index, "index", MAX_NAME_LEN)?;
    validate_non_empty(&query, "query", MAX_QUERY_LEN)?;
    let params = params.unwrap_or_default();
    if params.len() > MAX_SEARCH_PARAMS {
        return Err(format!("too many PARAMS pairs (max {})", MAX_SEARCH_PARAMS));
    }
    for (k, v) in &params {
        validate_non_empty(k, "param name", MAX_NAME_LEN)?;
        if v.len() > MAX_QUERY_LEN {
            return Err("param value exceeds maximum length".to_string());
        }
    }

    let pool = pool_of(&state, &connection_id)?;
    ensure_search(&pool, &connection_id).await?;
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    JsonSearchCollector::new()
        .ft_search(
            &mut conn,
            &index,
            &query,
            offset.unwrap_or(0),
            limit.unwrap_or(20).min(1000),
            &params,
            with_scores.unwrap_or(false),
        )
        .await
}

/// FT.CREATE from a structured spec (supports TEXT/TAG/NUMERIC/GEO/VECTOR fields).
#[tauri::command]
pub async fn ft_create(
    state: State<'_, AppState>,
    connection_id: String,
    spec: FtCreateSpec,
) -> Result<bool, String> {
    validate_connection_id(&connection_id)?;
    validate_non_empty(&spec.name, "index name", MAX_NAME_LEN)?;
    if spec.fields.is_empty() {
        return Err("index must define at least one field".to_string());
    }
    for p in &spec.prefixes {
        validate_non_empty(p, "prefix", MAX_NAME_LEN)?;
    }
    for f in &spec.fields {
        validate_non_empty(&f.identifier, "field identifier", MAX_NAME_LEN)?;
    }

    let pool = pool_of(&state, &connection_id)?;
    let cap = ensure_search(&pool, &connection_id).await?;
    let has_vector = spec
        .fields
        .iter()
        .any(|f| f.field_type.eq_ignore_ascii_case("VECTOR"));
    if has_vector && !cap.vector_search_supported {
        return Err(capability::unsupported_err(
            "Vector search",
            &format!("RediSearch {}", cap.search_version.unwrap_or_default()),
            "RediSearch 2.4 or later",
        ));
    }
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    JsonSearchCollector::new().ft_create(&mut conn, &spec).await?;
    Ok(true)
}

/// FT.DROPINDEX index [DD].
#[tauri::command]
pub async fn ft_drop_index(
    state: State<'_, AppState>,
    connection_id: String,
    index: String,
    delete_docs: Option<bool>,
) -> Result<bool, String> {
    validate_connection_id(&connection_id)?;
    validate_non_empty(&index, "index", MAX_NAME_LEN)?;

    let pool = pool_of(&state, &connection_id)?;
    ensure_search(&pool, &connection_id).await?;
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    JsonSearchCollector::new()
        .ft_drop(&mut conn, &index, delete_docs.unwrap_or(false))
        .await?;
    Ok(true)
}
