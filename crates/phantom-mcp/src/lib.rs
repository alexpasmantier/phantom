//! phantom-mcp — MCP server exposing phantom for AI agents.
//!
//! This crate's primary artifact is the `phantom-mcp` binary (see `src/main.rs`),
//! which speaks the Model Context Protocol over stdio. The library surface is
//! exposed so integration tests can drive the server without going through
//! the wire protocol.

pub mod observer;
pub mod render;
pub mod server;
pub mod tmux;
