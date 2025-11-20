//! Built-in functions for the aiki/core flow namespace
//!
//! This module contains native Rust implementations of functions that can be called
//! from flow definitions using the function call syntax.

pub mod build_metadata;
pub mod classify_edits;
pub mod generate_coauthors;
pub mod separate_edits;

pub use build_metadata::build_metadata;
pub use classify_edits::classify_edits;
pub use generate_coauthors::generate_coauthors;
pub use separate_edits::separate_edits;
