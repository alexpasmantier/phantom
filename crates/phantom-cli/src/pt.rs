// `pt` — short alias for `phantom`
// This is a separate binary target to avoid cargo warnings about
// shared source files. It simply delegates to the same entrypoint.
include!("main.rs");
