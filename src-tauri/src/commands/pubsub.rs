use crate::core::pubsub_manager::spawn_listener;
use crate::core::validate::{
    reject_null_bytes, validate_channel, validate_connection_id, validate_non_empty,
    validate_pattern,
};
use crate::AppState;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use tauri::{AppHandle, State};
use zeroize::{Zeroize, Zeroizing};

/// Maximum number of subscriptions (channels + patterns) per connection.
const MAX_SUBSCRIPTIONS_PER_CONN: usize = 100;

/// Maximum allowed length for a publish message payload.
const MAX_MESSAGE_LEN: usize = 65_536; // 64 KB

/// The full subscription state for a connection, returned to the frontend after
/// every subscribe / unsubscribe so it can render exact channels and glob
/// patterns distinctly.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionState {
    pub channels: Vec<String>,
    pub patterns: Vec<String>,
}

/// Build a Redis connection URL from stored connection config.
/// Returns a `Zeroizing<String>` so the password-bearing URL is wiped on drop.
fn build_redis_url(host: &str, port: u16, password: &str, db: u8, ssl: bool) -> Zeroizing<String> {
    let scheme = if ssl { "rediss" } else { "redis" };
    let url = if password.is_empty() {
        format!("{}://{}:{}/{}", scheme, host, port, db)
    } else {
        let encoded = urlencoding::encode(password);
        format!("{}://:{}@{}:{}/{}", scheme, encoded, host, port, db)
    };
    Zeroizing::new(url)
}

/// Open a dedicated pubsub connection for `connection_id` and subscribe to the
/// given channel + pattern sets. Uses stored connection config (NOT the shared
/// pool, whose connections must never enter subscriber mode).
async fn create_pubsub(
    state: &AppState,
    connection_id: &str,
    channels: &HashSet<String>,
    patterns: &HashSet<String>,
) -> Result<redis::aio::PubSub, String> {
    // Load connection config (scoped lock — never held across await).
    let (host, port, mut password, db, ssl) = {
        let cs = state.config_store.lock().map_err(|e| e.to_string())?;
        let connections = cs.load()?;
        let conn = connections
            .iter()
            .find(|c| c.id == connection_id)
            .ok_or_else(|| format!("Connection not found: {}", connection_id))?;
        (conn.host.clone(), conn.port, conn.password.clone(), conn.db, conn.ssl)
    };

    let url = build_redis_url(&host, port, &password, db, ssl);
    password.zeroize();

    let client = redis::Client::open(url.as_str()).map_err(|e| format!("Client error: {}", e))?;
    let mut pubsub = client
        .get_async_pubsub()
        .await
        .map_err(|e| format!("PubSub connect error: {}", e))?;

    for channel in channels {
        pubsub
            .subscribe(channel.as_str())
            .await
            .map_err(|e| format!("Subscribe error: {}", e))?;
    }
    for pattern in patterns {
        pubsub
            .psubscribe(pattern.as_str())
            .await
            .map_err(|e| format!("PSubscribe error: {}", e))?;
    }

    Ok(pubsub)
}

/// Return the sorted list of a channel/pattern set.
fn sorted(set: HashSet<String>) -> Vec<String> {
    let mut list: Vec<String> = set.into_iter().collect();
    list.sort();
    list
}

/// Publish a message to a channel.
#[tauri::command]
pub async fn pubsub_publish(
    state: State<'_, AppState>,
    connection_id: String,
    channel: String,
    message: String,
) -> Result<usize, String> {
    validate_connection_id(&connection_id)?;
    validate_channel(&channel)?;
    validate_non_empty(&message, "message", MAX_MESSAGE_LEN)?;
    reject_null_bytes(&message, "message")?;

    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    let num_subscribers: isize = redis::cmd("PUBLISH")
        .arg(&channel)
        .arg(&message)
        .query_async(&mut *conn)
        .await
        .map_err(|e| format!("Publish error: {}", e))?;

    Ok(num_subscribers as usize)
}

/// Subscribe to a channel or glob pattern. Establishes (or extends) a dedicated
/// pubsub listener that streams incoming messages to the frontend via
/// `pubsub-message` events. When `is_pattern` is true the target is treated as a
/// glob pattern (PSUBSCRIBE). Returns the full subscription state.
#[tauri::command]
pub async fn pubsub_subscribe(
    app: AppHandle,
    state: State<'_, AppState>,
    connection_id: String,
    channel: String,
    is_pattern: bool,
) -> Result<SubscriptionState, String> {
    validate_connection_id(&connection_id)?;
    validate_channel(&channel)?;

    let mut channels = state.pubsub_manager.channels(&connection_id);
    let mut patterns = state.pubsub_manager.patterns(&connection_id);

    // Enforce subscription count limit to prevent resource exhaustion.
    let current_count = channels.len() + patterns.len();
    if current_count >= MAX_SUBSCRIPTIONS_PER_CONN {
        return Err(format!(
            "Subscription limit reached (max {} per connection)",
            MAX_SUBSCRIPTIONS_PER_CONN
        ));
    }

    if is_pattern {
        patterns.insert(channel);
    } else {
        channels.insert(channel);
    }

    // Establish the connection & subscribe synchronously so failures surface now.
    let pubsub = create_pubsub(state.inner(), &connection_id, &channels, &patterns).await?;
    let handle = spawn_listener(app, connection_id.clone(), pubsub);
    state
        .pubsub_manager
        .replace(&connection_id, channels.clone(), patterns.clone(), handle);

    Ok(SubscriptionState {
        channels: sorted(channels),
        patterns: sorted(patterns),
    })
}

/// Unsubscribe from a single channel/pattern, or from everything when `channel`
/// is None. When `is_pattern` is true the target is removed from the pattern set
/// (PUNSUBSCRIBE semantics). Returns the remaining subscription state.
#[tauri::command]
pub async fn pubsub_unsubscribe(
    app: AppHandle,
    state: State<'_, AppState>,
    connection_id: String,
    channel: Option<String>,
    is_pattern: bool,
) -> Result<SubscriptionState, String> {
    validate_connection_id(&connection_id)?;

    let target = match channel {
        Some(ch) => {
            validate_channel(&ch)?;
            ch
        }
        None => {
            // Unsubscribe from everything: tear down the listener.
            state.pubsub_manager.clear(&connection_id);
            return Ok(SubscriptionState {
                channels: Vec::new(),
                patterns: Vec::new(),
            });
        }
    };

    let mut channels = state.pubsub_manager.channels(&connection_id);
    let mut patterns = state.pubsub_manager.patterns(&connection_id);
    if is_pattern {
        patterns.remove(&target);
    } else {
        channels.remove(&target);
    }

    if channels.is_empty() && patterns.is_empty() {
        state.pubsub_manager.clear(&connection_id);
        return Ok(SubscriptionState {
            channels: Vec::new(),
            patterns: Vec::new(),
        });
    }

    // Re-establish the listener with the reduced channel/pattern set.
    let pubsub = create_pubsub(state.inner(), &connection_id, &channels, &patterns).await?;
    let handle = spawn_listener(app, connection_id.clone(), pubsub);
    state
        .pubsub_manager
        .replace(&connection_id, channels.clone(), patterns.clone(), handle);

    Ok(SubscriptionState {
        channels: sorted(channels),
        patterns: sorted(patterns),
    })
}

/// List all available channels.
#[tauri::command]
pub async fn pubsub_list_channels(
    state: State<'_, AppState>,
    connection_id: String,
    pattern: Option<String>,
) -> Result<Vec<String>, String> {
    validate_connection_id(&connection_id)?;
    if let Some(ref p) = pattern {
        validate_pattern(p)?;
    }

    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    let channels: Vec<String> = if let Some(p) = pattern {
        redis::cmd("PUBSUB")
            .arg("CHANNELS")
            .arg(&p)
            .query_async(&mut *conn)
            .await
            .map_err(|e| format!("List channels error: {}", e))?
    } else {
        redis::cmd("PUBSUB")
            .arg("CHANNELS")
            .query_async(&mut *conn)
            .await
            .map_err(|e| format!("List channels error: {}", e))?
    };

    Ok(channels)
}

/// Get the number of subscribers for a channel.
#[tauri::command]
pub async fn pubsub_num_subs(
    state: State<'_, AppState>,
    connection_id: String,
    channel: String,
) -> Result<usize, String> {
    validate_connection_id(&connection_id)?;
    validate_channel(&channel)?;

    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    let result: HashMap<String, usize> = redis::cmd("PUBSUB")
        .arg("NUMSUB")
        .arg(&channel)
        .query_async(&mut *conn)
        .await
        .map_err(|e| format!("Num subs error: {}", e))?;

    Ok(result.get(&channel).copied().unwrap_or(0))
}
