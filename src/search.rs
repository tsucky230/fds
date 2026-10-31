//! Incremental filename search in the current directory (P3-14). Pure logic.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Backward,
}

/// Finds the index of the next name containing `query` (case-insensitive, Unicode lowercase).
///
/// Behavior:
/// - Empty `query` -> None.
/// - Forward: check `start`, `start + 1`, ..., wrapping to 0, and back to `start - 1`.
///   Backward: `start`, `start - 1`, ..., wrapping to the last index.
///   `start` itself is checked first (so typing more letters keeps the cursor if it still matches).
/// - `start >= names.len()` is treated as 0. Empty `names` -> None.
pub fn find(names: &[&str], start: usize, query: &str, direction: Direction) -> Option<usize> {
    if query.is_empty() || names.is_empty() {
        return None;
    }
    let query = query.to_lowercase();
    let mut index = if start < names.len() { start } else { 0 };
    for _ in 0..names.len() {
        if names
            .get(index)
            .is_some_and(|name| name.to_lowercase().contains(&query))
        {
            return Some(index);
        }
        index = match direction {
            Direction::Forward => {
                if index == names.len() - 1 {
                    0
                } else {
                    index + 1
                }
            }
            Direction::Backward => {
                if index == 0 {
                    names.len() - 1
                } else {
                    index - 1
                }
            }
        };
    }
    None
}
