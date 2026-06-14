use std::fs;
use std::path::Path;

use archidoc_types::ModuleDoc;
use ignore::WalkBuilder;

use crate::parser;
use crate::path_resolver;

/// Walk a source tree and extract ModuleDocs from all module entry files.
///
/// Finds `lib.rs`, `mod.rs`, and flat `.rs` module files with archidoc annotations,
/// extracts `//!` doc comments, and builds ModuleDoc structs from the parsed annotations.
///
/// The walk honours `.gitignore` / `.ignore` files (and skips hidden dirs like
/// `.git`, `.jj`, `target`) via the `ignore` crate, so build output, VCS
/// metadata, and other ignored trees are never surfaced as modules.
///
/// Flat module support: A `.rs` file that is not `mod.rs` or `lib.rs` is included
/// if it contains archidoc annotations (C4 markers: `@c4 container` or `@c4 component`).
pub fn extract_all_docs(root: &Path) -> Vec<ModuleDoc> {
    let mut docs_map = std::collections::HashMap::<String, (ModuleDoc, bool)>::new();

    for entry in WalkBuilder::new(root).build().filter_map(|e| e.ok()) {
        let path = entry.path();

        // Only process .rs files
        let filename = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) if name.ends_with(".rs") => name,
            _ => continue,
        };

        // Extract archidoc content
        let content = match parser::archidoc_from_file(path) {
            Some(c) if !c.trim().is_empty() => c,
            _ => continue,
        };

        // For non-standard entry files, require C4 markers
        let is_standard_entry = filename == "lib.rs" || filename == "mod.rs";
        if !is_standard_entry {
            let has_c4_marker = content.contains("@c4 container")
                || content.contains("@c4 component");
            if !has_c4_marker {
                continue;
            }
        }

        let module_path = path_resolver::path_to_module_name(path, root, filename);

        // When both foo/mod.rs and foo.rs exist, mod.rs takes priority.
        // Use is_standard_entry as the priority flag — mod.rs/lib.rs always win
        // over flat modules regardless of WalkDir traversal order.
        let is_priority = is_standard_entry;
        if let Some((_, existing_is_priority)) = docs_map.get(&module_path) {
            if *existing_is_priority && !is_priority {
                // Existing entry is mod.rs/lib.rs, skip this flat module
                continue;
            }
            if !*existing_is_priority && !is_priority {
                // Both are flat modules with same path — keep first
                continue;
            }
            // Otherwise this is a priority entry replacing a flat module — fall through
        }

        let c4_level = parser::extract_c4_level(&content);
        let pattern = parser::extract_pattern(&content);
        let pattern_status = parser::extract_pattern_status(&content);
        let description = parser::extract_description(&content);
        let parent_container = parser::extract_parent_container(&module_path);
        let relationships = parser::extract_relationships(&content);
        let files = parser::extract_file_table(&content);

        docs_map.insert(module_path.clone(), (ModuleDoc {
            module_path,
            content,
            source_file: path.to_string_lossy().to_string(),
            c4_level,
            pattern,
            pattern_status,
            description,
            parent_container,
            relationships,
            files,
        }, is_priority));
    }

    let mut docs: Vec<ModuleDoc> = docs_map.into_values().map(|(doc, _)| doc).collect();
    docs.sort_by(|a, b| a.module_path.cmp(&b.module_path));
    docs
}

/// Read all `.rs` source files in a directory and return their contents.
///
/// Returns a vec of `(filename, source_code)` pairs. Skips files that
/// cannot be read. Used by pattern heuristics to scan a module directory
/// for structural evidence without coupling AST analysis to filesystem I/O.
pub fn read_rs_sources(dir: &Path) -> Vec<(String, String)> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    entries
        .filter_map(|e| e.ok())
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                let filename = path.file_name()?.to_string_lossy().to_string();
                let source = fs::read_to_string(&path).ok()?;
                Some((filename, source))
            } else {
                None
            }
        })
        .collect()
}
