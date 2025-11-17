//! Built-in functions for the aiki/core flow namespace
//!
//! This module contains native Rust implementations of functions that can be called
//! from flow definitions using the function call syntax.

pub mod build_description;
pub mod generate_coauthors;

pub use build_description::build_description;
pub use generate_coauthors::generate_coauthors;
