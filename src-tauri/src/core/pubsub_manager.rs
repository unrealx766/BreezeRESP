//! Real-time Pub/Sub subscription manager.
//!
//! Redis subscription commands (`SUBSCRIBE`/`PSUBSCRIBE`) put a connection into
//! subscriber mode, which breaks pooled connections. They are therefore blocked
//! on the shared pool (see `core::validate`). Instead, each connection that has
//! active subscriptions owns a **dedicated** pubsub connection driven by a
//! background task. Incoming messages are forwarded to the frontend via the
//! Tauri event system (`pubsub-message`).
//!
//! Dynamic channel changes are handled by (re)spawning the listener task with
//! the full desired channel set. For a single-user desktop tool this is simpler
//! and more robust than juggling a live subscribe/unsubscribe control stream.

use futures::StreamExt;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use tokio::task::JoinHandle;

/// Event name emitted to the frontend for each received Pub/Sub message.
pub const PUBSUB_EVENT: &str = "pubsub-message";

/// Payload emitted to the frontend for a single received message.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PubSubEvent {
    pub connection_id: String,
    pub channel: String,
    pub message: String,
    pub timestamp: u64,
    /// The glob pattern the message matched, when delivered via a pattern
    /// subscription (PSUBSCRIBE); `None` for exact-channel subscriptions.
    pub pattern: Option<String>,
}

/// A live subscription for one connection: the exact channels, glob patterns and
/// the handle to the background listener task.
struct Subscription {
    channels: HashSet<String>,
    patterns: HashSet<String>,
    handle: JoinHandle<()>,
}

/// Tracks active subscriptions across all connections.
pub struct PubSubManager {
    subs: Mutex<HashMap<String, Subscription>>,
}

impl PubSubManager {
    pub fn new() -> Self {
        Self {
            subs: Mutex::new(HashMap::new()),
        }
    }

    /// Returns the current desired channel set for a connection (empty if none).
    pub fn channels(&self, connection_id: &str) -> HashSet<String> {
        self.subs
            .lock()
            .ok()
            .and_then(|m| m.get(connection_id).map(|s| s.channels.clone()))
            .unwrap_or_default()
    }

    /// Returns the current desired glob-pattern set for a connection (empty if none).
    pub fn patterns(&self, connection_id: &str) -> HashSet<String> {
        self.subs
            .lock()
            .ok()
            .and_then(|m| m.get(connection_id).map(|s| s.patterns.clone()))
            .unwrap_or_default()
    }

    /// Replace the listener for a connection, aborting the previous task.
    pub fn replace(
        &self,
        connection_id: &str,
        channels: HashSet<String>,
        patterns: HashSet<String>,
        handle: JoinHandle<()>,
    ) {
        let mut map = match self.subs.lock() {
            Ok(m) => m,
            Err(_) => {
                handle.abort();
                return;
            }
        };
        if let Some(prev) = map.insert(
            connection_id.to_string(),
            Subscription { channels, patterns, handle },
        ) {
            prev.handle.abort();
        }
    }

    /// Abort and remove a connection's listener (on unsubscribe-all / disconnect).
    pub fn clear(&self, connection_id: &str) {
        if let Ok(mut map) = self.subs.lock() {
            if let Some(prev) = map.remove(connection_id) {
                prev.handle.abort();
            }
        }
    }
}

impl Default for PubSubManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Spawn a background task that reads messages from an already-subscribed
/// pubsub connection and forwards them to the frontend as `pubsub-message`
/// events. The task ends when it is aborted or the connection drops.
pub fn spawn_listener(
    app: AppHandle,
    connection_id: String,
    mut pubsub: redis::aio::PubSub,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut stream = pubsub.on_message();
        while let Some(msg) = stream.next().await {
            let channel = msg.get_channel_name().to_string();
            // Pattern subscriptions (PSUBSCRIBE) carry the matched pattern; exact
            // channel subscriptions do not.
            let pattern = msg.get_pattern::<String>().ok();
            // Prefer a UTF-8 payload; fall back to a lossy conversion for binary.
            let message: String = msg
                .get_payload::<String>()
                .unwrap_or_else(|_| String::from_utf8_lossy(msg.get_payload_bytes()).into_owned());
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);

            let _ = app.emit(
                PUBSUB_EVENT,
                PubSubEvent {
                    connection_id: connection_id.clone(),
                    channel,
                    message,
                    timestamp,
                    pattern,
                },
            );
        }
    })
}
