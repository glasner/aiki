//! ACP (Agent Client Protocol) proxy implementation
//!
//! This module implements a transparent proxy between IDEs (like Zed) and AI agents
//! (like Claude Code). It intercepts the ACP protocol stream to:
//!
//! - Track file changes made by the AI agent
//! - Record provenance metadata for each change
//! - Support autoreply workflows (automated follow-up prompts)
//!
//! ## Architecture
//!
//! The proxy uses a 3-thread architecture:
//! - **IDE→Agent thread**: Forwards messages from IDE to agent
//! - **Agent→IDE thread**: Forwards messages from agent to IDE, firing events
//! - **Main thread**: Coordinates startup and shutdown
//!
//! ## Module Structure
//!
//! - `protocol`: ACP protocol types and JSON-RPC handling
//! - `state`: State management (autoreply counters, tool call context)
//! - `handlers`: Event firing and session management
//!
//! The main proxy entry point is in `commands/acp.rs`.

pub mod handlers;
pub mod protocol;
pub mod state;
