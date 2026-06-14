#![allow(rustdoc::invalid_html_tags)]
//! @c4 container
//! # Extract Docs Rust
//!
//! Rust language adapter — parses `//!` annotations from mod.rs/lib.rs files.
//!
//! | File | Pattern | Purpose | Health |
//! |------|---------|---------|--------|
//! | `walker.rs` | -- | Directory tree walker | planned |
//! | `parser.rs` | -- | Annotation parser | planned |
//! | `path_resolver.rs` | -- | File path to module path conversion | planned |
//! | `pattern_heuristic.rs` | Strategy | Structural GoF pattern detection | planned |
//! | `fitness.rs` | -- | Architectural fitness functions | planned |
//! | `promote.rs` | -- | Auto-promote planned to verified | planned |
//! | `cargo_modules.rs` | -- | cargo-modules integration (optional) | planned |

pub mod cargo_metadata;
pub mod cargo_modules;
pub mod fitness;
pub mod parser;
pub mod path_resolver;
pub mod pattern_heuristic;
pub mod promote;
pub mod walker;
