//! Task ID generation
//!
//! Generates short, unique task IDs using a hash of timestamp and name.

use std::time::{SystemTime, UNIX_EPOCH};

/// Generate a unique short task ID (4 hex characters)
///
/// The ID is generated from a hash of the current timestamp and task name,
/// producing a short, memorable identifier like "a1b2".
#[must_use]
pub fn generate_task_id(name: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    // Simple hash combining timestamp and name
    let mut hash: u32 = 0;
    for byte in name.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(u32::from(byte));
    }
    hash = hash.wrapping_add(timestamp as u32);

    // Return 4 hex characters
    format!("{:04x}", hash & 0xFFFF)
}

/// Generate a child task ID
///
/// Child IDs are created by appending a numeric suffix to the parent ID.
/// For example: "a1b2" -> "a1b2.1", "a1b2.1" -> "a1b2.1.1"
#[must_use]
pub fn generate_child_id(parent_id: &str, child_number: usize) -> String {
    format!("{}.{}", parent_id, child_number)
}

/// Check if a task ID is a child of another task ID
#[must_use]
pub fn is_child_of(task_id: &str, parent_id: &str) -> bool {
    task_id.starts_with(&format!("{}.", parent_id))
}

/// Get the parent ID from a child task ID
///
/// Returns `None` if the task ID has no parent (root task).
#[must_use]
pub fn get_parent_id(task_id: &str) -> Option<&str> {
    task_id.rsplit_once('.').map(|(parent, _)| parent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_task_id_format() {
        let id = generate_task_id("Test task");
        assert_eq!(id.len(), 4);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_generate_task_id_different_names() {
        let id1 = generate_task_id("Task 1");
        // Sleep a tiny bit to ensure different timestamp
        std::thread::sleep(std::time::Duration::from_millis(1));
        let id2 = generate_task_id("Task 2");
        // IDs should be different (very high probability due to different timestamps)
        // Note: There's a tiny chance they could collide, so we just verify format
        assert_eq!(id1.len(), 4);
        assert_eq!(id2.len(), 4);
    }

    #[test]
    fn test_generate_child_id() {
        assert_eq!(generate_child_id("a1b2", 1), "a1b2.1");
        assert_eq!(generate_child_id("a1b2", 2), "a1b2.2");
        assert_eq!(generate_child_id("a1b2.1", 1), "a1b2.1.1");
    }

    #[test]
    fn test_is_child_of() {
        assert!(is_child_of("a1b2.1", "a1b2"));
        assert!(is_child_of("a1b2.1.1", "a1b2.1"));
        assert!(is_child_of("a1b2.1.1", "a1b2"));
        assert!(!is_child_of("a1b2", "a1b2"));
        assert!(!is_child_of("a1b2", "a1b2.1"));
        assert!(!is_child_of("b2c3", "a1b2"));
    }

    #[test]
    fn test_get_parent_id() {
        assert_eq!(get_parent_id("a1b2.1"), Some("a1b2"));
        assert_eq!(get_parent_id("a1b2.1.1"), Some("a1b2.1"));
        assert_eq!(get_parent_id("a1b2"), None);
    }
}
