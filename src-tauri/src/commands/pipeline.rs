use crate::AppState;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineCommand {
    pub command: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineResult {
    pub success: bool,
    pub value: String,
    pub error: Option<String>,
    pub latency_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineResponse {
    pub results: Vec<PipelineResult>,
    pub total_latency_ms: f64,
    pub individual_sum_ms: f64,
}

fn format_redis_value(val: &redis::Value) -> String {
    match val {
        redis::Value::Nil => "(nil)".to_string(),
        redis::Value::Int(i) => i.to_string(),
        redis::Value::BulkString(data) => {
            String::from_utf8(data.clone()).unwrap_or_else(|_| format!("{:?}", data))
        }
        redis::Value::SimpleString(s) => s.clone(),
        redis::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(format_redis_value).collect();
            format!("[{}]", items.join(", "))
        }
        redis::Value::Okay => "OK".to_string(),
        _ => format!("{:?}", val),
    }
}

/// Execute a batch of Redis commands as a pipeline
#[tauri::command]
pub async fn execute_pipeline(
    state: State<'_, AppState>,
    connection_id: String,
    commands: Vec<PipelineCommand>,
) -> Result<PipelineResponse, String> {
    let pool = {
        let pm = state.pool_manager.lock().map_err(|e| e.to_string())?;
        pm.get_pool(&connection_id)?
    };
    let mut conn = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

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
        .query_async(&mut *conn)
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
