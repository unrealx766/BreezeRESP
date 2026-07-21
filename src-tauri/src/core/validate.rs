//! IPC parameter validation utilities.
//!
//! All strings arriving from the frontend via `invoke()` must pass through
//! these guards before being forwarded to Redis commands or the filesystem,
//! mitigating injection / abuse vectors when the frontend is compromised
//! (e.g. via XSS on maliciously-crafted Redis values).

/// Maximum allowed length for identifiers (connection id, key name, pattern, etc.)
const MAX_IDENTIFIER_LEN: usize = 1024;

/// Maximum allowed length for a single Redis command string
const MAX_COMMAND_LEN: usize = 65_536; // 64 KB

/// Maximum number of pipeline commands in a single batch
const MAX_PIPELINE_COMMANDS: usize = 500;

/// Maximum length of a connection name
const MAX_CONNECTION_NAME_LEN: usize = 128;

/// Maximum host name length
const MAX_HOST_LEN: usize = 253;

/// Maximum length of a single pipeline argument
const MAX_ARG_LEN: usize = 65_536; // 64 KB

/// Maximum number of arguments per pipeline command
const MAX_ARGS_PER_CMD: usize = 64;

/// Blocked Redis administrative commands that could compromise the host system
/// or disrupt the connection pool.
const BLOCKED_COMMANDS: &[&str] = &[
    // ── Server administration ──
    "CONFIG",       // CONFIG SET dir / CONFIG SET dbfilename → RCE
    "DEBUG",        // DEBUG SEGFAULT, etc.
    "MODULE",       // load arbitrary .so/.dll
    "SHUTDOWN",     // server shutdown
    "ACL",          // tamper with ACL rules
    "FLUSHDB",      // wipe current database
    "FLUSHALL",     // wipe all databases
    "SWAPDB",       // swap databases
    "BGSAVE",       // trigger background save
    "BGREWRITEAOF", // trigger AOF rewrite
    "SAVE",         // blocking save
    "CLUSTER",      // cluster management
    "MIGRATE",      // migrate keys to another instance
    "RESTORE",      // inject serialised RDB data
    "SORT",         // SORT … STORE writes arbitrary keys
    "WAIT",         // block until replication catches up
    "OBJECT",       // internal object introspection
    "LATENCY",      // latency monitoring
    "SLOWLOG",      // slow-log management
    // ── Scripting / replication ──
    "SCRIPT",       // arbitrary Lua execution
    "EVAL",         // arbitrary Lua execution
    "EVALSHA",      // arbitrary Lua execution
    "SLAVEOF",      // replication hijack
    "REPLICAOF",    // replication hijack (newer alias)
    // ── Connection-breaking commands ──
    "SUBSCRIBE",    // enters subscription mode — breaks subsequent commands
    "PSUBSCRIBE",   // enters subscription mode
    "UNSUBSCRIBE",  // subscription mode commands
    "PUNSUBSCRIBE", // subscription mode commands
    "MONITOR",      // streams all commands (sensitive data leak)
    "SELECT",       // changes DB on pooled connection → corrupts pool state
    "QUIT",         // disconnects the pooled connection
    "RESET",        // resets connection state
    // ── Blocking commands (could hang the pool connection) ──
    "BLPOP",        // blocking list pop
    "BRPOP",        // blocking list pop
    "BLMOVE",       // blocking list move
    "BRPOPLPUSH",   // blocking rpop-lpush
    "BZPOPMIN",     // blocking sorted-set pop
    "BZPOPMAX",     // blocking sorted-set pop
    // ── Data movement ──
    "COPY",         // copy key (side effects)
    "MOVE",         // move key to another DB
];

// ---------------------------------------------------------------------------
// Generic string guards
// ---------------------------------------------------------------------------

/// Reject empty or whitespace-only strings, and enforce a length ceiling.
pub fn validate_non_empty(s: &str, field: &str, max_len: usize) -> Result<(), String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Err(format!("{} must not be empty", field));
    }
    if s.len() > max_len {
        return Err(format!(
            "{} exceeds maximum length ({} > {})",
            field,
            s.len(),
            max_len
        ));
    }
    Ok(())
}

/// Reject strings containing null bytes (which can truncate C-level APIs).
pub fn reject_null_bytes(s: &str, field: &str) -> Result<(), String> {
    if s.contains('\0') {
        return Err(format!("{} must not contain null bytes", field));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Domain-specific validators
// ---------------------------------------------------------------------------

/// Validate a connection id (non-empty, reasonable length, no null bytes).
pub fn validate_connection_id(id: &str) -> Result<(), String> {
    validate_non_empty(id, "connection_id", MAX_IDENTIFIER_LEN)?;
    reject_null_bytes(id, "connection_id")?;
    Ok(())
}

/// Validate a Redis key name.
pub fn validate_key(key: &str) -> Result<(), String> {
    validate_non_empty(key, "key", MAX_IDENTIFIER_LEN)?;
    reject_null_bytes(key, "key")?;
    Ok(())
}

/// Validate a SCAN / KEYS pattern.
pub fn validate_pattern(pattern: &str) -> Result<(), String> {
    validate_non_empty(pattern, "pattern", MAX_IDENTIFIER_LEN)?;
    reject_null_bytes(pattern, "pattern")?;
    Ok(())
}

/// Validate a Pub/Sub channel name.
///
/// Note: subscription commands are dispatched over a dedicated pubsub connection
/// (not the shared pool), so the `SUBSCRIBE` blocklist in `validate_command`
/// does not apply here. We only enforce the same identifier hygiene as keys.
pub fn validate_channel(channel: &str) -> Result<(), String> {
    validate_non_empty(channel, "channel", MAX_IDENTIFIER_LEN)?;
    reject_null_bytes(channel, "channel")?;
    Ok(())
}

/// Validate a raw Redis command string (sandbox / pipeline).
pub fn validate_command(cmd: &str) -> Result<(), String> {
    validate_non_empty(cmd, "command", MAX_COMMAND_LEN)?;
    reject_null_bytes(cmd, "command")?;

    // Block obviously dangerous Redis administrative commands that could
    // compromise the host system when the frontend is under XSS control.
    let first_token = cmd
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_uppercase();

    if BLOCKED_COMMANDS.contains(&first_token.as_str()) {
        return Err(format!(
            "Command '{}' is blocked for security reasons",
            first_token
        ));
    }

    Ok(())
}

/// Validate a pipeline command list (size + per-command checks including blocked commands).
pub fn validate_pipeline_commands(commands: &[(String, Vec<String>)]) -> Result<(), String> {
    if commands.len() > MAX_PIPELINE_COMMANDS {
        return Err(format!(
            "Too many pipeline commands ({} > {})",
            commands.len(),
            MAX_PIPELINE_COMMANDS
        ));
    }
    for (cmd, args) in commands {
        validate_non_empty(cmd, "pipeline command", MAX_IDENTIFIER_LEN)?;
        reject_null_bytes(cmd, "pipeline command")?;
        // Also check blocked commands for each pipeline command
        let first_token = cmd.trim().to_uppercase();
        if BLOCKED_COMMANDS.contains(&first_token.as_str()) {
            return Err(format!(
                "Command '{}' is blocked for security reasons",
                first_token
            ));
        }
        // Validate arguments
        if args.len() > MAX_ARGS_PER_CMD {
            return Err(format!(
                "Too many arguments for command '{}' ({} > {})",
                first_token,
                args.len(),
                MAX_ARGS_PER_CMD
            ));
        }
        for arg in args {
            if arg.len() > MAX_ARG_LEN {
                return Err(format!(
                    "Argument for command '{}' exceeds maximum length ({} > {})",
                    first_token,
                    arg.len(),
                    MAX_ARG_LEN
                ));
            }
            reject_null_bytes(arg, "pipeline argument")?;
        }
    }
    Ok(())
}

/// Validate connection configuration fields.
pub fn validate_connection_config(
    host: &str,
    port: u16,
    name: &str,
    password: &str,
) -> Result<(), String> {
    validate_non_empty(host, "host", MAX_HOST_LEN)?;
    reject_null_bytes(host, "host")?;

    validate_non_empty(name, "name", MAX_CONNECTION_NAME_LEN)?;
    reject_null_bytes(name, "name")?;

    reject_null_bytes(password, "password")?;
    if password.len() > 4096 {
        return Err("password exceeds maximum length".to_string());
    }

    if port == 0 {
        return Err("port must be greater than 0".to_string());
    }

    Ok(())
}

/// Validate the count parameter for SCAN commands.
pub fn validate_scan_count(count: u64) -> Result<(), String> {
    if count == 0 {
        return Err("count must be greater than 0".to_string());
    }
    if count > 100_000 {
        return Err("count must not exceed 100000".to_string());
    }
    Ok(())
}

/// Validate TTL value (positive or -1 for persist).
pub fn validate_ttl(ttl: i64) -> Result<(), String> {
    if ttl < -1 {
        return Err("TTL must be -1 (persist) or a positive integer".to_string());
    }
    Ok(())
}
