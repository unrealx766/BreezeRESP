use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Stores snapshots of Redis state before sandbox operations.
/// Used for diff computation and rollback.
pub struct ShadowStore {
    snapshots: HashMap<String, Snapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    pub timestamp: i64,
    pub command: String,
    pub before_state: HashMap<String, String>,
    pub after_state: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffResult {
    pub added: Vec<(String, String)>,
    pub modified: Vec<(String, String, String)>, // key, before, after
    pub deleted: Vec<(String, String)>,
}

impl ShadowStore {
    pub fn new() -> Self {
        Self {
            snapshots: HashMap::new(),
        }
    }

    /// Create a snapshot of current key states before executing a command.
    pub fn create_snapshot(
        &mut self,
        id: &str,
        command: &str,
        before: HashMap<String, String>,
    ) -> String {
        let snapshot = Snapshot {
            id: id.to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            command: command.to_string(),
            before_state: before,
            after_state: HashMap::new(),
        };
        let snap_id = snapshot.id.clone();
        self.snapshots.insert(snap_id.clone(), snapshot);
        snap_id
    }

    /// Update the after-state of a snapshot.
    pub fn set_after_state(&mut self, id: &str, after: HashMap<String, String>) {
        if let Some(snap) = self.snapshots.get_mut(id) {
            snap.after_state = after;
        }
    }

    /// Compute diff between before and after states.
    pub fn compute_diff(&self, id: &str) -> Option<DiffResult> {
        let snap = self.snapshots.get(id)?;
        let mut added = Vec::new();
        let mut modified = Vec::new();
        let mut deleted = Vec::new();

        // Check for added and modified
        for (key, after_val) in &snap.after_state {
            match snap.before_state.get(key) {
                Some(before_val) if before_val != after_val => {
                    modified.push((key.clone(), before_val.clone(), after_val.clone()));
                }
                None => {
                    added.push((key.clone(), after_val.clone()));
                }
                _ => {} // unchanged
            }
        }

        // Check for deleted
        for (key, before_val) in &snap.before_state {
            if !snap.after_state.contains_key(key) {
                deleted.push((key.clone(), before_val.clone()));
            }
        }

        Some(DiffResult {
            added,
            modified,
            deleted,
        })
    }

    /// Remove a snapshot.
    pub fn remove(&mut self, id: &str) {
        self.snapshots.remove(id);
    }
}

impl Default for ShadowStore {
    fn default() -> Self {
        Self::new()
    }
}
