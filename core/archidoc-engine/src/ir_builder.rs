//! IR Builder — converts extracted modules + filesystem tree into ArchitectureIR v2.0.
//!
//! The builder performs three steps:
//! 1. Build bare tree from filesystem (structure only)
//! 2. Overlay module annotations onto matching directory nodes
//! 3. Resolve parent relationships (nearest annotated ancestor)

use std::path::Path;

use archidoc_types::ir::{ArchitectureIR, DirNode, FileNode, Relationship};
use archidoc_types::module_doc::ModuleDoc;

use crate::tree::TreeConfig;

/// Build a complete ArchitectureIR from a bare tree and extracted modules.
pub fn build_ir(
    scan_root: &Path,
    bare_tree: DirNode,
    modules: Vec<ModuleDoc>,
) -> ArchitectureIR {
    let mut root = bare_tree;

    // Overlay each module's annotations onto the matching tree node
    for module in &modules {
        let dir_path = module_path_to_dir_path(module, scan_root);
        overlay_module(&mut root, &dir_path, module, scan_root);
    }

    // Resolve parent pointers (nearest annotated ancestor)
    resolve_parents(&mut root, None);

    ArchitectureIR {
        version: "2.0".to_string(),
        scan_root: scan_root.to_string_lossy().to_string(),
        root,
    }
}

/// Build an ArchitectureIR from scratch: scan filesystem + overlay modules.
pub fn build_from_scan(
    scan_root: &Path,
    modules: Vec<ModuleDoc>,
    config: &TreeConfig,
) -> ArchitectureIR {
    let bare_tree = crate::tree::build_dir_tree(scan_root, config);
    build_ir(scan_root, bare_tree, modules)
}

// ---------------------------------------------------------------------------
// Module path → directory path conversion
// ---------------------------------------------------------------------------

/// Convert a ModuleDoc's source_file to a relative directory path.
///
/// The source_file is an absolute path; we strip scan_root to get relative,
/// then take the parent directory (since source_file points to mod.rs/lib.rs/etc).
/// Falls back to using the module_path (dot→slash) if source_file can't be relativized.
fn module_path_to_dir_path(module: &ModuleDoc, scan_root: &Path) -> String {
    // Special case: _lib module maps to root
    if module.module_path == "_lib" {
        return ".".to_string();
    }

    let source = Path::new(&module.source_file);
    let filename = source.file_name().and_then(|n| n.to_str()).unwrap_or("");

    // Try to strip scan_root prefix
    if !scan_root.as_os_str().is_empty() {
        if let Ok(relative) = source.strip_prefix(scan_root) {
            let mut dir = relative.parent().unwrap_or(Path::new("."));
            // A crate-root `lib.rs` represents its crate, not the `src/` directory
            // it physically lives in. Collapse `<crate>/src/lib.rs` to `<crate>` so
            // the annotation lands on the crate node.
            if filename == "lib.rs"
                && dir.file_name().and_then(|n| n.to_str()) == Some("src")
            {
                dir = dir.parent().unwrap_or(Path::new("."));
            }
            let dir_str = dir.to_string_lossy().replace('\\', "/");
            if !dir_str.is_empty() {
                return dir_str;
            }
            return ".".to_string();
        }
    }

    // Otherwise derive from module_path (dot → slash)
    let path = module.module_path.replace('.', "/");
    if path.is_empty() { ".".to_string() } else { path }
}

// ---------------------------------------------------------------------------
// Annotation overlay
// ---------------------------------------------------------------------------

/// Overlay a ModuleDoc's annotations onto the matching DirNode.
/// If the directory doesn't exist in the tree, creates it.
fn overlay_module(tree: &mut DirNode, target_path: &str, module: &ModuleDoc, scan_root: &Path) {
    // Ensure the directory exists in the tree
    ensure_dir_exists(tree, target_path);

    let node = match find_dir_mut(tree, target_path) {
        Some(n) => n,
        None => return, // should not happen after ensure_dir_exists
    };

    // Set strategy fields
    node.c4_level = Some(module.c4_level);
    node.description = if module.description.is_empty() {
        None
    } else {
        Some(module.description.clone())
    };
    node.pattern = if module.pattern == "--" {
        None
    } else {
        Some(module.pattern.clone())
    };
    node.pattern_status = Some(module.pattern_status);
    node.content = if module.content.is_empty() {
        None
    } else {
        Some(module.content.clone())
    };

    // Source file stored as relative path (forward slashes)
    let source = Path::new(&module.source_file);
    if !scan_root.as_os_str().is_empty() {
        if let Ok(rel) = source.strip_prefix(scan_root) {
            node.source_file = Some(rel.to_string_lossy().replace('\\', "/"));
        } else {
            node.source_file = source.file_name().map(|n| n.to_string_lossy().to_string());
        }
    } else if target_path == "." {
        node.source_file = source.file_name().map(|n| n.to_string_lossy().to_string());
    } else {
        node.source_file = source
            .file_name()
            .map(|n| format!("{}/{}", target_path, n.to_string_lossy()));
    }

    // Relationships — convert module_path targets to directory paths
    // For now, store them as-is (the module_path format); full resolution requires
    // a second pass with the complete module set
    node.relationships = module
        .relationships
        .iter()
        .map(|r| Relationship {
            target: r.target.clone(),
            label: r.label.clone(),
            protocol: r.protocol.clone(),
        })
        .collect();

    // Code-level elements (@c4 code)
    node.code_elements = module
        .code_elements
        .iter()
        .map(|c| archidoc_types::ir::CodeElement {
            name: c.name.clone(),
            kind: c.kind.clone(),
            description: if c.description.is_empty() || c.description == "*No description*" {
                None
            } else {
                Some(c.description.clone())
            },
            relationships: c
                .relationships
                .iter()
                .map(|r| Relationship {
                    target: r.target.clone(),
                    label: r.label.clone(),
                    protocol: r.protocol.clone(),
                })
                .collect(),
        })
        .collect();

    if module.layer.is_some() {
        node.layer = module.layer.clone();
    }

    // Overlay file table entries onto existing FileNodes
    for file_entry in &module.files {
        let file_node = node
            .files
            .iter_mut()
            .find(|f| f.name == file_entry.name);

        if let Some(fnode) = file_node {
            // Promote bare FileNode with attributes
            fnode.pattern = if file_entry.pattern == "--" {
                None
            } else {
                Some(file_entry.pattern.clone())
            };
            fnode.pattern_status = Some(file_entry.pattern_status);
            fnode.purpose = if file_entry.purpose.is_empty() {
                None
            } else {
                Some(file_entry.purpose.clone())
            };
            fnode.health = Some(file_entry.health);
            fnode.extra = file_entry.extra.clone();
        } else {
            // File in annotation but not on disk (ghost) — still record it
            node.files.push(FileNode {
                name: file_entry.name.clone(),
                pattern: if file_entry.pattern == "--" {
                    None
                } else {
                    Some(file_entry.pattern.clone())
                },
                pattern_status: Some(file_entry.pattern_status),
                purpose: if file_entry.purpose.is_empty() {
                    None
                } else {
                    Some(file_entry.purpose.clone())
                },
                health: Some(file_entry.health),
                extra: file_entry.extra.clone(),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Parent resolution
// ---------------------------------------------------------------------------

/// Walk the tree depth-first, setting `parent` on each annotated node to the
/// nearest annotated ancestor's path.
fn resolve_parents(node: &mut DirNode, nearest_annotated_ancestor: Option<&str>) {
    let my_path = node.path.clone();
    let next_ancestor = if node.is_annotated() {
        node.parent = nearest_annotated_ancestor.map(|s| s.to_string());
        Some(my_path.as_str())
    } else {
        nearest_annotated_ancestor
    };

    // Need to reborrow to satisfy borrow checker
    let ancestor_str = next_ancestor.unwrap_or("").to_string();
    let ancestor_opt = if ancestor_str.is_empty() {
        None
    } else {
        Some(ancestor_str.as_str())
    };

    for child in &mut node.dirs {
        resolve_parents(child, ancestor_opt);
    }
}

// ---------------------------------------------------------------------------
// Tree navigation helpers
// ---------------------------------------------------------------------------

/// Ensure a directory path exists in the tree, creating intermediate nodes as needed.
fn ensure_dir_exists(tree: &mut DirNode, path: &str) {
    if path == "." || path.is_empty() {
        return; // root always exists
    }
    if find_dir_mut(tree, path).is_some() {
        return; // already exists
    }

    // Build path components and create missing nodes
    let parts: Vec<&str> = path.split('/').collect();
    let mut current = tree;

    for (i, part) in parts.iter().enumerate() {
        let partial_path = parts[..=i].join("/");
        let exists = current.dirs.iter().any(|d| d.name == *part);
        if !exists {
            current.dirs.push(DirNode::empty(part, &partial_path));
            current.dirs.sort_by(|a, b| a.name.cmp(&b.name));
        }
        // Navigate into the child
        let idx = current.dirs.iter().position(|d| d.name == *part).unwrap();
        current = &mut current.dirs[idx];
    }
}

fn find_dir_mut<'a>(tree: &'a mut DirNode, path: &str) -> Option<&'a mut DirNode> {
    if tree.path == path {
        return Some(tree);
    }
    for child in &mut tree.dirs {
        if let Some(found) = find_dir_mut(child, path) {
            return Some(found);
        }
    }
    None
}
