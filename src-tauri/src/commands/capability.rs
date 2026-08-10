//! Capability probing command (Redis version + module support).

use crate::core::capability::{self, ServerCapability};
use crate::core::validate::validate_connection_id;
use crate::AppState;
use tauri::State;

/// Probe (or return the cached) capability profile of a connection.
/// Pass `force = true` to re-probe after e.g. a server upgrade.
#[tauri::command]
pub async fn get_server_capability(
    state: State<'_, AppState>,
    connection_id: String,
    force: Option<bool>,
) -> Result<ServerCapability, String> {
    validate_connection_id(&connection_id)?;

    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    capability::get_or_probe(&pool, &connection_id, force.unwrap_or(false)).await
}
