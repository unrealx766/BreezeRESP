use std::collections::HashMap;

/// Maximum number of snapshots to keep in memory to prevent memory leaks.
const MAX_SNAPSHOTS: usize = 100;

/// Stores snapshots of Redis state before sandbox operations.
/// Used for diff computation and rollback.
pub struct ShadowStore {
    snapshots: HashMap<String, Snapshot>,
    /// Ordered list of snapshot IDs (oldest first) for eviction.
    order: Vec<String>,

    // ── Pending state for cumulative sandbox previews ──
    // Tracks the "committed sandbox view" so that successive previews
    // build on each other instead of starting from scratch.
    //
    // original_*  = Redis state BEFORE any pending preview (for cancel/restore)
    // pending_*   = state AFTER all pending previews applied (baseline for next preview)
    /// Key → serialised value as it was in real Redis before the first pending preview.
    pub(crate) original_state: HashMap<String, String>,
    /// Key → Redis type string as it was in real Redis before the first pending preview.
    pub(crate) original_key_types: HashMap<String, String>,
    /// Key → serialised value after all pending previews (None = key was deleted).
    pub(crate) pending_state: HashMap<String, Option<String>>,
    /// Key → Redis type string after all pending previews.
    pub(crate) pending_key_types: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub id: String,
    pub before_state: HashMap<String, String>,
    pub after_state: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct DiffResult {
    pub added: Vec<(String, String)>,
    pub modified: Vec<(String, String, String)>, // key, before, after
    pub deleted: Vec<(String, String)>,
    pub unchanged: Vec<(String, String)>, // key, value
}

impl ShadowStore {
    pub fn new() -> Self {
        Self {
            snapshots: HashMap::new(),
            order: Vec::new(),
            original_state: HashMap::new(),
            original_key_types: HashMap::new(),
            pending_state: HashMap::new(),
            pending_key_types: HashMap::new(),
        }
    }

    /// Evict oldest snapshots when the limit is exceeded.
    fn evict_if_needed(&mut self) {
        while self.snapshots.len() > MAX_SNAPSHOTS {
            if let Some(oldest_id) = self.order.first().cloned() {
                self.snapshots.remove(&oldest_id);
                self.order.remove(0);
            } else {
                break;
            }
        }
    }

    /// Create a snapshot of current key states before executing a command.
    pub fn create_snapshot(
        &mut self,
        id: &str,
        before: HashMap<String, String>,
    ) -> String {
        let snapshot = Snapshot {
            id: id.to_string(),
            before_state: before,
            after_state: HashMap::new(),
        };
        let snap_id = snapshot.id.clone();
        self.snapshots.insert(snap_id.clone(), snapshot);
        self.order.push(snap_id.clone());
        self.evict_if_needed();
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
        let mut unchanged = Vec::new();

        // Check for added, modified, and unchanged
        for (key, after_val) in &snap.after_state {
            match snap.before_state.get(key) {
                Some(before_val) if before_val != after_val => {
                    modified.push((key.clone(), before_val.clone(), after_val.clone()));
                }
                Some(before_val) => {
                    unchanged.push((key.clone(), before_val.clone()));
                }
                None => {
                    added.push((key.clone(), after_val.clone()));
                }
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
            unchanged,
        })
    }

    // ── Pending state management ──────────────────────────────────────

    /// Whether there are any pending (uncommitted) sandbox changes.
    pub fn has_pending(&self) -> bool {
        !self.pending_state.is_empty()
    }

    /// Clear all pending state (called after apply or cancel).
    pub fn clear_pending(&mut self) {
        self.original_state.clear();
        self.original_key_types.clear();
        self.pending_state.clear();
        self.pending_key_types.clear();
    }
}

impl Default for ShadowStore {
    fn default() -> Self {
        Self::new()
    }
}
