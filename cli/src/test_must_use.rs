// Test to verify #[must_use] warnings work
#![cfg(test)]

use crate::flows::types::{ActionResult, ExecutionContext};
use std::path::PathBuf;

#[test]
fn test_must_use_warning_compilation() {
    // These should be fine - results are used
    let _result = ActionResult::success();
    let _failure = ActionResult::failure(1, "error".to_string());
    let _ctx = ExecutionContext::new(PathBuf::from("/tmp"));
}

#[test]
fn test_impl_asref_path_ergonomics() {
    // Test that impl AsRef<Path> accepts different types

    // Accept &str
    let _ctx1 = ExecutionContext::new("/tmp");

    // Accept String
    let _ctx2 = ExecutionContext::new(String::from("/tmp"));

    // Accept &String
    let s = String::from("/tmp");
    let _ctx3 = ExecutionContext::new(&s);

    // Accept PathBuf
    let _ctx4 = ExecutionContext::new(PathBuf::from("/tmp"));

    // Accept &PathBuf
    let pb = PathBuf::from("/tmp");
    let _ctx5 = ExecutionContext::new(&pb);

    // Accept &Path
    let _ctx6 = ExecutionContext::new(pb.as_path());
}

// This function intentionally ignores return values to verify #[must_use] works
// It should generate warnings when compiled
#[cfg(any())] // Disabled by default since it would fail CI
#[allow(dead_code)]
fn verify_must_use_triggers_warnings() {
    // These SHOULD trigger unused_must_use warnings
    ActionResult::success(); // unused must_use warning expected
    ActionResult::failure(1, "error".to_string()); // unused must_use warning expected
    ExecutionContext::new(PathBuf::from("/tmp")); // unused must_use warning expected
}
