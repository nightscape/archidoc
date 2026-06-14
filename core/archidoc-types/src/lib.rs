#![allow(rustdoc::invalid_html_tags)]
//! @c4 component
//! # Extract Docs Types
//!
//! Shared type definitions for the archidoc toolchain.
//!
//! | File | Pattern | Purpose | Health |
//! |------|---------|---------|--------|
//! | `module_doc.rs` | -- | Core data structures | planned |
//! | `annotation.rs` | -- | Annotation spec enums | planned |

pub mod annotation;
pub mod ir;
pub mod module_doc;
pub mod report;
pub mod scaffold_ir;

pub use annotation::{HealthStatus, PatternStatus};
pub use ir::{ArchitectureIR, C4Level, DirNode, FileNode};
pub use scaffold_ir::{ScaffoldIR, ScaffoldNode, ScaffoldPostHook, ScaffoldTemplate, ScaffoldVariable};
pub use module_doc::{CodeElement, FileEntry, ModuleDoc, Relationship};
pub use report::{
    AnnotationStatus, CoverageReport, DirCoverage, DriftReport, DriftedFile, ElementHealth,
    GhostEntry, HealthReport, OrphanEntry, ValidationReport,
};
