//! Built-in functions for the aiki/core flow namespace
//!
//! This module contains native Rust implementations of functions that can be called
//! from flow definitions using the function call syntax.

mod functions;

pub use functions::{
    build_human_metadata, build_human_metadata_post, build_metadata, classify_edits,
    generate_coauthors, get_git_user_function, prepare_separation, restore_original_files,
    separate_edits, write_ai_files,
};
