use crate::core::validate::{validate_connection_id, validate_pipeline_commands};
use crate::AppState;
use crate::core::format::format_redis_value;
use crate::core::pipeline_store::{StoredPipeline, StoredPipelineCommand};
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineCommand {
    pub command: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineResult {
    pub success: bool,
    pub value: String,
    pub error: Option<String>,
    pub latency_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineResponse {
    pub results: Vec<PipelineResult>,
    pub total_latency_ms: f64,
    pub individual_sum_ms: f64,
}

/// Execute a batch of Redis commands as a pipeline
#[tauri::command]
pub async fn execute_pipeline(
    state: State<'_, AppState>,
    connection_id: String,
    commands: Vec<PipelineCommand>,
) -> Result<PipelineResponse, String> {
    validate_connection_id(&connection_id)?;
    // Build a vec of (cmd, args) for validation
    let cmd_refs: Vec<(String, Vec<String>)> = commands
        .iter()
        .map(|c| (c.command.clone(), c.args.clone()))
        .collect();
    validate_pipeline_commands(&cmd_refs)?;

    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

    // Cluster: an arbitrary command batch may span multiple slots, which a
    // single pipeline cannot serve. Execute commands one by one instead
    // (each individually timed; no atomicity in cluster mode).
    if pool.is_cluster() {
        let mut pipeline_results = Vec::with_capacity(commands.len());
        let mut individual_sum = 0.0_f64;
        let total_start = std::time::Instant::now();

        for cmd in &commands {
            let mut redis_cmd = redis::cmd(&cmd.command);
            for arg in &cmd.args {
                redis_cmd.arg(arg);
            }
            let start = std::time::Instant::now();
            let res: Result<redis::Value, redis::RedisError> =
                redis_cmd.query_async(&mut conn).await;
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;

            let result = match res {
                Ok(val) => PipelineResult {
                    success: true,
                    value: format_redis_value(&val),
                    error: None,
                    latency_ms: (elapsed * 100.0).round() / 100.0,
                },
                Err(e) => PipelineResult {
                    success: false,
                    value: String::new(),
                    error: Some(e.to_string()),
                    latency_ms: (elapsed * 100.0).round() / 100.0,
                },
            };
            individual_sum += result.latency_ms;
            pipeline_results.push(result);
        }

        let total_elapsed = total_start.elapsed().as_secs_f64() * 1000.0;
        return Ok(PipelineResponse {
            results: pipeline_results,
            total_latency_ms: (total_elapsed * 100.0).round() / 100.0,
            individual_sum_ms: (individual_sum * 100.0).round() / 100.0,
        });
    }

    let mut pipe = redis::pipe();
    for cmd in &commands {
        let mut pipeline_cmd = redis::cmd(&cmd.command);
        for arg in &cmd.args {
            pipeline_cmd.arg(arg);
        }
        pipe.add_command(pipeline_cmd);
    }

    let start = std::time::Instant::now();
    let results: Vec<redis::Value> = pipe
        .query_async(&mut conn)
        .await
        .map_err(|e| format!("Pipeline execution error: {}", e))?;
    let total_elapsed = start.elapsed().as_secs_f64() * 1000.0;

    let mut pipeline_results = Vec::new();
    let mut individual_sum = 0.0_f64;

    // Estimate per-command latency (total / count as approximation)
    let per_cmd_latency = if commands.is_empty() {
        0.0
    } else {
        total_elapsed / commands.len() as f64
    };

    for (i, val) in results.into_iter().enumerate() {
        let formatted = format_redis_value(&val);
        let is_error = matches!(val, redis::Value::ServerError(_));

        let result = PipelineResult {
            success: !is_error,
            value: if is_error {
                String::new()
            } else {
                formatted.clone()
            },
            error: if is_error {
                Some(formatted)
            } else {
                None
            },
            latency_ms: (per_cmd_latency * 100.0).round() / 100.0,
        };
        individual_sum += result.latency_ms;
        pipeline_results.push(result);

        let _ = i;
    }

    Ok(PipelineResponse {
        results: pipeline_results,
        total_latency_ms: (total_elapsed * 100.0).round() / 100.0,
        individual_sum_ms: (individual_sum * 100.0).round() / 100.0,
    })
}

/// Save a pipeline configuration to encrypted local storage
#[tauri::command]
pub async fn save_pipeline(
    state: State<'_, AppState>,
    id: String,
    name: String,
    commands: Vec<PipelineCommand>,
    created_at: u64,
) -> Result<(), String> {
    crate::core::validate::validate_non_empty(&id, "pipeline id", 256)?;
    crate::core::validate::validate_non_empty(&name, "pipeline name", 256)?;

    let store = state.pipeline_store.lock().map_err(|e| e.to_string())?;
    let stored = StoredPipeline {
        id,
        name,
        commands: commands
            .into_iter()
            .map(|c| StoredPipelineCommand {
                command: c.command,
                args: c.args,
            })
            .collect(),
        created_at,
    };
    store.save(stored)
}

/// List all saved pipeline configurations
#[tauri::command]
pub async fn list_pipelines(
    state: State<'_, AppState>,
) -> Result<Vec<StoredPipeline>, String> {
    let store = state.pipeline_store.lock().map_err(|e| e.to_string())?;
    store.load_all()
}

/// Delete a saved pipeline by id
#[tauri::command]
pub async fn delete_pipeline(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    crate::core::validate::validate_non_empty(&id, "pipeline id", 256)?;

    let store = state.pipeline_store.lock().map_err(|e| e.to_string())?;
    store.delete(&id)
}
