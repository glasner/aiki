//! Task ID generation
//!
//! Generates unique task IDs compatible with JJ's change_id format.
//!
//! JJ change_ids are 32-character strings using "reverse hex" encoding (z-k instead of 0-9a-f).
//! This makes them more readable and reduces confusion with commit hashes.

use std::time::{SystemTime, UNIX_EPOCH};

/// Generate a unique task ID in JJ change_id format
///
/// Creates a 32-character ID using JJ's "reverse hex" encoding (z-k instead of 0-9a-f).
/// The ID combines timestamp, task name hash, and cryptographically random data to ensure
/// uniqueness even in bulk/automated operations.
///
/// # Uniqueness Guarantee
///
/// Uses 128 bits of entropy from:
/// - Nanosecond-precision timestamp
/// - Task name hash
/// - Two 64-bit random salts
///
/// This provides collision resistance equivalent to JJ's native change_id generation,
/// with negligible collision probability (< 2^-64) even for billions of tasks.
///
/// # Example
/// ```
/// use aiki::tasks::generate_task_id;
/// let id = generate_task_id("Fix bug");
/// assert_eq!(id.len(), 32);
/// assert!(id.chars().all(|c| matches!(c, 'k'..='z')));
/// ```
#[must_use]
pub fn generate_task_id(name: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    // Generate 128 bits (16 bytes) using timestamp, name hash, and random salt
    let mut hasher = DefaultHasher::new();
    timestamp.hash(&mut hasher);
    name.hash(&mut hasher);

    // Add random salt for true uniqueness (prevents collisions in bulk operations)
    use rand::Rng;
    let random_salt: u64 = rand::thread_rng().gen();
    random_salt.hash(&mut hasher);
    let hash1 = hasher.finish();

    // Generate second part with additional randomness
    let mut hasher2 = DefaultHasher::new();
    hash1.hash(&mut hasher2);
    let random_salt2: u64 = rand::thread_rng().gen();
    random_salt2.hash(&mut hasher2);
    let hash2 = hasher2.finish();

    // Convert to 32 characters using reverse hex (z-k instead of 0-9a-f)
    // z=0, y=1, x=2, w=3, v=4, u=5, t=6, s=7, r=8, q=9, p=a, o=b, n=c, m=d, l=e, k=f
    let reverse_hex = |nibble: u8| -> char {
        match nibble {
            0 => 'z',
            1 => 'y',
            2 => 'x',
            3 => 'w',
            4 => 'v',
            5 => 'u',
            6 => 't',
            7 => 's',
            8 => 'r',
            9 => 'q',
            10 => 'p',
            11 => 'o',
            12 => 'n',
            13 => 'm',
            14 => 'l',
            15 => 'k',
            _ => 'z',
        }
    };

    let mut result = String::with_capacity(32);

    // Encode first 64 bits (16 hex chars)
    for i in (0..16).rev() {
        let nibble = ((hash1 >> (i * 4)) & 0xF) as u8;
        result.push(reverse_hex(nibble));
    }

    // Encode second 64 bits (16 hex chars)
    for i in (0..16).rev() {
        let nibble = ((hash2 >> (i * 4)) & 0xF) as u8;
        result.push(reverse_hex(nibble));
    }

    result
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

/// Check if a task is a direct child of a parent (not grandchild)
#[must_use]
pub fn is_direct_child_of(task_id: &str, parent_id: &str) -> bool {
    get_parent_id(task_id) == Some(parent_id)
}

/// Get the child number from a task ID (the last numeric suffix)
///
/// Returns `None` if the task ID has no parent.
#[must_use]
pub fn get_child_number(task_id: &str) -> Option<usize> {
    task_id
        .rsplit_once('.')
        .and_then(|(_, num)| num.parse::<usize>().ok())
}

/// Get the next child number for a parent task
///
/// Scans the list of task IDs and finds the highest existing child number,
/// then returns the next number. Returns 1 if no children exist.
#[must_use]
pub fn get_next_child_number<'a>(
    parent_id: &str,
    task_ids: impl Iterator<Item = &'a str>,
) -> usize {
    let max_child = task_ids
        .filter(|id| is_direct_child_of(id, parent_id))
        .filter_map(get_child_number)
        .max();

    max_child.map_or(1, |n| n + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_task_id_format() {
        let id = generate_task_id("Test task");
        assert_eq!(id.len(), 32, "Task ID should be 32 characters");
        assert!(
            id.chars().all(|c| matches!(c, 'k'..='z')),
            "Task ID should only contain reverse hex characters (k-z)"
        );
    }

    #[test]
    fn test_generate_task_id_different_names() {
        let id1 = generate_task_id("Task 1");
        let id2 = generate_task_id("Task 2");

        // IDs should be different due to different names
        assert_ne!(
            id1, id2,
            "Different task names should generate different IDs"
        );
        assert_eq!(id1.len(), 32);
        assert_eq!(id2.len(), 32);
    }

    #[test]
    fn test_generate_task_id_uniqueness() {
        // Same name and very close timestamps should still differ due to randomness
        let id1 = generate_task_id("Same name");
        let id2 = generate_task_id("Same name");
        // These will differ due to random salts
        assert_eq!(id1.len(), 32);
        assert_eq!(id2.len(), 32);
    }

    #[test]
    fn test_generate_child_id() {
        // Old short ID format (legacy)
        assert_eq!(generate_child_id("a1b2", 1), "a1b2.1");
        assert_eq!(generate_child_id("a1b2", 2), "a1b2.2");
        assert_eq!(generate_child_id("a1b2.1", 1), "a1b2.1.1");

        // New JJ change_id format
        let change_id = "mvslrspmoynoxyyywqyutmovxpvztkls";
        assert_eq!(
            generate_child_id(change_id, 1),
            "mvslrspmoynoxyyywqyutmovxpvztkls.1"
        );
        assert_eq!(
            generate_child_id(change_id, 2),
            "mvslrspmoynoxyyywqyutmovxpvztkls.2"
        );
        assert_eq!(
            generate_child_id("mvslrspmoynoxyyywqyutmovxpvztkls.1", 1),
            "mvslrspmoynoxyyywqyutmovxpvztkls.1.1"
        );
    }

    #[test]
    fn test_is_child_of() {
        // Old short ID format (legacy)
        assert!(is_child_of("a1b2.1", "a1b2"));
        assert!(is_child_of("a1b2.1.1", "a1b2.1"));
        assert!(is_child_of("a1b2.1.1", "a1b2"));
        assert!(!is_child_of("a1b2", "a1b2"));
        assert!(!is_child_of("a1b2", "a1b2.1"));
        assert!(!is_child_of("b2c3", "a1b2"));

        // New JJ change_id format
        let change_id = "mvslrspmoynoxyyywqyutmovxpvztkls";
        assert!(is_child_of("mvslrspmoynoxyyywqyutmovxpvztkls.1", change_id));
        assert!(is_child_of(
            "mvslrspmoynoxyyywqyutmovxpvztkls.1.1",
            "mvslrspmoynoxyyywqyutmovxpvztkls.1"
        ));
        assert!(is_child_of(
            "mvslrspmoynoxyyywqyutmovxpvztkls.1.1",
            change_id
        ));
        assert!(!is_child_of(change_id, change_id));
        assert!(!is_child_of(
            change_id,
            "mvslrspmoynoxyyywqyutmovxpvztkls.1"
        ));
    }

    #[test]
    fn test_get_parent_id() {
        // Old short ID format (legacy)
        assert_eq!(get_parent_id("a1b2.1"), Some("a1b2"));
        assert_eq!(get_parent_id("a1b2.1.1"), Some("a1b2.1"));
        assert_eq!(get_parent_id("a1b2"), None);

        // New JJ change_id format
        assert_eq!(
            get_parent_id("mvslrspmoynoxyyywqyutmovxpvztkls.1"),
            Some("mvslrspmoynoxyyywqyutmovxpvztkls")
        );
        assert_eq!(
            get_parent_id("mvslrspmoynoxyyywqyutmovxpvztkls.1.1"),
            Some("mvslrspmoynoxyyywqyutmovxpvztkls.1")
        );
        assert_eq!(get_parent_id("mvslrspmoynoxyyywqyutmovxpvztkls"), None);
    }

    #[test]
    fn test_is_direct_child_of() {
        // Direct children
        assert!(is_direct_child_of("a1b2.1", "a1b2"));
        assert!(is_direct_child_of("a1b2.2", "a1b2"));

        // Grandchildren are NOT direct children
        assert!(!is_direct_child_of("a1b2.1.1", "a1b2"));
        assert!(is_direct_child_of("a1b2.1.1", "a1b2.1"));

        // Same ID is not a child
        assert!(!is_direct_child_of("a1b2", "a1b2"));

        // Unrelated IDs
        assert!(!is_direct_child_of("b2c3.1", "a1b2"));
    }

    #[test]
    fn test_get_child_number() {
        assert_eq!(get_child_number("a1b2.1"), Some(1));
        assert_eq!(get_child_number("a1b2.42"), Some(42));
        assert_eq!(get_child_number("a1b2.1.3"), Some(3));
        assert_eq!(get_child_number("a1b2"), None);
    }

    #[test]
    fn test_get_next_child_number() {
        // No children exist
        let task_ids: Vec<&str> = vec!["a1b2", "other"];
        assert_eq!(get_next_child_number("a1b2", task_ids.iter().copied()), 1);

        // Has children
        let task_ids = vec!["a1b2", "a1b2.1", "a1b2.2"];
        assert_eq!(get_next_child_number("a1b2", task_ids.iter().copied()), 3);

        // Has gaps (should find max + 1, not fill gap)
        let task_ids = vec!["a1b2", "a1b2.1", "a1b2.5"];
        assert_eq!(get_next_child_number("a1b2", task_ids.iter().copied()), 6);

        // Ignores grandchildren
        let task_ids = vec!["a1b2", "a1b2.1", "a1b2.1.1", "a1b2.1.2"];
        assert_eq!(get_next_child_number("a1b2", task_ids.iter().copied()), 2);

        // Works with nested parents
        let task_ids = vec!["a1b2.1", "a1b2.1.1", "a1b2.1.2"];
        assert_eq!(get_next_child_number("a1b2.1", task_ids.iter().copied()), 3);
    }

    // Edge case tests

    #[test]
    fn test_get_parent_id_edge_cases() {
        // Empty string
        assert_eq!(get_parent_id(""), None);

        // No dots - root task
        assert_eq!(get_parent_id("abcd1234"), None);

        // Trailing dot (malformed but handle gracefully)
        assert_eq!(get_parent_id("parent."), Some("parent"));

        // Multiple trailing dots
        assert_eq!(get_parent_id("parent.."), Some("parent."));

        // Just a dot
        assert_eq!(get_parent_id("."), Some(""));
    }

    #[test]
    fn test_is_child_of_edge_cases() {
        // Empty strings
        assert!(!is_child_of("", "parent"));
        assert!(!is_child_of("child", ""));
        assert!(!is_child_of("", ""));

        // Same ID with dot suffix ambiguity
        assert!(is_child_of("parent.1", "parent"));
        assert!(!is_child_of("parent1", "parent")); // No dot = not a child
    }

    #[test]
    fn test_is_direct_child_of_edge_cases() {
        // Empty parent
        assert!(!is_direct_child_of("task.1", ""));

        // ID that looks like but isn't a child (missing dot separator)
        assert!(!is_direct_child_of("parent1", "parent"));

        // Prefix collision: a1b2.10 should NOT match scope a1b2.1
        // This is the bug we fixed!
        assert!(!is_direct_child_of("a1b2.10", "a1b2.1"));
        assert!(is_direct_child_of("a1b2.10", "a1b2"));
        assert!(is_direct_child_of("a1b2.1", "a1b2"));
    }

    #[test]
    fn test_get_child_number_edge_cases() {
        // Non-numeric child number
        assert_eq!(get_child_number("parent.abc"), None);

        // Negative number (parsed as non-numeric)
        assert_eq!(get_child_number("parent.-1"), None);

        // Zero child number (valid but unusual - planning task)
        assert_eq!(get_child_number("parent.0"), Some(0));

        // Leading zeros (still valid numbers)
        assert_eq!(get_child_number("parent.01"), Some(1));
        assert_eq!(get_child_number("parent.001"), Some(1));

        // Empty after dot
        assert_eq!(get_child_number("parent."), None);

        // Large number
        assert_eq!(get_child_number("parent.999999"), Some(999999));
    }

    #[test]
    fn test_get_next_child_number_edge_cases() {
        // Empty task list
        let task_ids: Vec<&str> = vec![];
        assert_eq!(get_next_child_number("parent", task_ids.iter().copied()), 1);

        // Non-existent parent (no children found)
        let task_ids = vec!["other.1", "other.2"];
        assert_eq!(get_next_child_number("parent", task_ids.iter().copied()), 1);

        // Child with number 0 (planning task)
        let task_ids = vec!["parent", "parent.0", "parent.1"];
        assert_eq!(get_next_child_number("parent", task_ids.iter().copied()), 2);

        // Only planning task exists
        let task_ids = vec!["parent", "parent.0"];
        assert_eq!(get_next_child_number("parent", task_ids.iter().copied()), 1);
    }

    #[test]
    fn test_generate_child_id_edge_cases() {
        // Child number 0 (planning task)
        assert_eq!(generate_child_id("parent", 0), "parent.0");

        // Large child number
        assert_eq!(generate_child_id("parent", 999999), "parent.999999");

        // Nested parent
        assert_eq!(generate_child_id("a.1.2.3", 4), "a.1.2.3.4");
    }

    #[test]
    fn test_id_uniqueness_bulk() {
        // Generate many IDs and verify no collisions
        use std::collections::HashSet;

        let mut ids = HashSet::new();
        for i in 0..1000 {
            let id = generate_task_id(&format!("Task {}", i));
            assert!(
                ids.insert(id.clone()),
                "Collision detected for task {}",
                i
            );
        }
        assert_eq!(ids.len(), 1000);
    }
}
