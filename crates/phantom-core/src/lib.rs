//! Core types and protocol definitions for [phantom](https://github.com/alexpasmantier/phantom).
//!
//! This crate contains the shared types used by both the phantom CLI and daemon:
//!
//! - [`protocol`] — JSON request/response enums for daemon communication
//! - [`types`] — Session, cursor, screen, and input data structures
//! - [`exit_codes`] — Standardized process exit codes

pub mod exit_codes;
pub mod protocol;
pub mod types;
