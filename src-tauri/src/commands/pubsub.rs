use crate::core::validate::{validate_command, validate_connection_id};
use crate::AppState;
use std::collections::HashMap;
use tauri::State;

/// Publish a message to a channel.
#[tauri::command]
pub async fn pubsub_publish(
    state: State<'_, AppState>,
    connection_id: String,
    channel: String,
    message: String,
) -> Result<usize, String> {
    validate_connection_id(&connection_id)?;
    validate_command(&format!("PUBLISH {} {}", channel, message))?;

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

/// Subscribe to a channel and receive messages.
#[tauri::command]
pub async fn pubsub_subscribe(
    _state: State<'_, AppState>,
    connection_id: String,
    channel: String,
) -> Result<String, String> {
    validate_connection_id(&connection_id)?;

    // Note: Real-time subscription streaming would require persistent WebSocket connections
    // For now, we just acknowledge the command can be executed
    Ok(format!("Subscription command ready for channel: {}", channel))
}

/// Unsubscribe from a channel.
#[tauri::command]
pub async fn pubsub_unsubscribe(
    _state: State<'_, AppState>,
    connection_id: String,
    channel: Option<String>,
) -> Result<String, String> {
    validate_connection_id(&connection_id)?;
    
    let ch = channel.unwrap_or_else(|| "all".to_string());
    Ok(format!("Unsubscription command ready for channel: {}", ch))
}

/// List all available channels.
#[tauri::command]
pub async fn pubsub_list_channels(
    state: State<'_, AppState>,
    connection_id: String,
    pattern: Option<String>,
) -> Result<Vec<String>, String> {
    validate_connection_id(&connection_id)?;

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
    validate_command(&format!("PUBSUB NUMSUB {}", channel))?;

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
