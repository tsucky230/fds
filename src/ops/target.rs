//! Which entries an operation applies to (R-400, P3-1). Pure logic.

use std::path::PathBuf;

use crate::pane::Pane;

/// Targets for Copy / Move / Delete.
///
/// Behavior:
/// - If any visible entry is marked, the marked entries' paths in list order.
/// - Otherwise the cursor entry's path.
/// - The `[..]` entry is never a target (it cannot be marked, and a cursor on it yields nothing).
/// - Empty when there is nothing to operate on.
pub fn targets(pane: &Pane) -> Vec<PathBuf> {
    let marked: Vec<PathBuf> = pane
        .marked_entries()
        .into_iter()
        .filter(|e| !e.is_parent_link())
        .map(|e| e.path.clone())
        .collect();
    if !marked.is_empty() {
        return marked;
    }
    pane.cursor_entry()
        .filter(|e| !e.is_parent_link())
        .map(|e| vec![e.path.clone()])
        .unwrap_or_default()
}
