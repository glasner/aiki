//! Spec management — SpecGraph and metadata parsing
//!
//! Provides first-class spec management with:
//! - O(1) reverse index from spec files to implementing tasks
//! - Spec metadata parsing from markdown files
//! - Status inference (Draft → Planned → Implementing → Implemented)

pub mod graph;
pub mod parser;

pub use graph::{normalize_spec_path, Spec, SpecGraph, SpecStatus};
pub use parser::{parse_spec_metadata, SpecMetadata};
