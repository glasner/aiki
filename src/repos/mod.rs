pub mod detector;
pub mod id;

// Re-export commonly used items for convenience
pub use detector::RepoDetector;
pub use id::{
    compute_repo_id, ensure_repo_id,
};
