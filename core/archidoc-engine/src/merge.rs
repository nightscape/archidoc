use std::collections::BTreeMap;
use std::fmt;

use archidoc_types::ir::{ArchitectureIR, DirNode, FileNode};

/// Error returned when merge encounters conflicting definitions.
#[derive(Debug)]
pub struct MergeError {
    pub path: String,
    pub message: String,
}

impl fmt::Display for MergeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "merge conflict at '{}': {}", self.path, self.message)
    }
}

/// Merge two `ArchitectureIR` trees into one.
///
/// Rules:
/// - Directories present in both sides: merge strategy fields (incoming wins for non-None).
/// - Directories/files present in only one side: included as-is.
/// - C4 level conflict at same path: returns `MergeError`.
/// - `scan_root`: incoming wins.
/// - Children are sorted by name in output.
pub fn merge_ir(base: ArchitectureIR, incoming: ArchitectureIR) -> Result<ArchitectureIR, MergeError> {
    let root = merge_dir(base.root, incoming.root)?;

    Ok(ArchitectureIR {
        version: "2.0".to_string(),
        scan_root: incoming.scan_root,
        root,
    })
}

/// Recursively merge two DirNodes.
fn merge_dir(base: DirNode, incoming: DirNode) -> Result<DirNode, MergeError> {
    // Check for C4 level conflict
    if let (Some(base_level), Some(inc_level)) = (base.c4_level, incoming.c4_level) {
        if base_level != inc_level {
            return Err(MergeError {
                path: base.path.clone(),
                message: format!(
                    "conflicting C4 levels: existing '{}' vs new '{}'",
                    base_level, inc_level
                ),
            });
        }
    }

    // Strategy fields: incoming wins for non-None
    let merged = DirNode {
        name: incoming.name,
        path: incoming.path,
        c4_level: incoming.c4_level.or(base.c4_level),
        description: incoming.description.or(base.description),
        pattern: incoming.pattern.or(base.pattern),
        pattern_status: incoming.pattern_status.or(base.pattern_status),
        content: incoming.content.or(base.content),
        source_file: incoming.source_file.or(base.source_file),
        parent: incoming.parent.or(base.parent),
        relationships: if incoming.relationships.is_empty() {
            base.relationships
        } else {
            incoming.relationships
        },
        code_elements: if incoming.code_elements.is_empty() {
            base.code_elements
        } else {
            incoming.code_elements
        },
        dirs: Vec::new(), // filled below
        files: Vec::new(), // filled below
    };

    // Merge child dirs by name
    let mut dir_map: BTreeMap<String, DirNode> = BTreeMap::new();
    for d in base.dirs {
        dir_map.insert(d.name.clone(), d);
    }
    for d in incoming.dirs {
        let name = d.name.clone();
        if let Some(existing) = dir_map.remove(&name) {
            dir_map.insert(name, merge_dir(existing, d)?);
        } else {
            dir_map.insert(name, d);
        }
    }

    // Merge files by name (incoming wins entirely on duplicate)
    let mut file_map: BTreeMap<String, FileNode> = BTreeMap::new();
    for f in base.files {
        file_map.insert(f.name.clone(), f);
    }
    for f in incoming.files {
        file_map.insert(f.name.clone(), f);
    }

    let mut result = merged;
    result.dirs = dir_map.into_values().collect();
    result.files = file_map.into_values().collect();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use archidoc_types::ir::{DirNode, FileNode};
    use archidoc_types::C4Level;
    use archidoc_types::annotation::HealthStatus;

    fn make_ir_with_root(root: DirNode) -> ArchitectureIR {
        ArchitectureIR {
            version: "2.0".to_string(),
            scan_root: "/test".to_string(),
            root,
        }
    }

    fn bare_root() -> DirNode {
        DirNode::empty(".", ".")
    }

    fn annotated_dir(name: &str, path: &str, level: C4Level, desc: &str) -> DirNode {
        let mut d = DirNode::empty(name, path);
        d.c4_level = Some(level);
        d.description = Some(desc.to_string());
        d
    }

    #[test]
    fn merge_combines_disjoint_dirs() {
        let mut base_root = bare_root();
        base_root.dirs.push(annotated_dir("api", "api", C4Level::Container, "API"));

        let mut inc_root = bare_root();
        inc_root.dirs.push(annotated_dir("db", "db", C4Level::Component, "Database"));

        let result = merge_ir(
            make_ir_with_root(base_root),
            make_ir_with_root(inc_root),
        ).unwrap();

        assert_eq!(result.root.dirs.len(), 2);
        assert_eq!(result.root.dirs[0].name, "api");
        assert_eq!(result.root.dirs[1].name, "db");
    }

    #[test]
    fn merge_incoming_strategy_wins() {
        let mut base_root = bare_root();
        base_root.dirs.push(annotated_dir("api", "api", C4Level::Container, "Old desc"));

        let mut inc_root = bare_root();
        inc_root.dirs.push(annotated_dir("api", "api", C4Level::Container, "New desc"));

        let result = merge_ir(
            make_ir_with_root(base_root),
            make_ir_with_root(inc_root),
        ).unwrap();

        assert_eq!(result.root.dirs.len(), 1);
        assert_eq!(result.root.dirs[0].description.as_deref(), Some("New desc"));
    }

    #[test]
    fn merge_rejects_c4_level_conflict() {
        let mut base_root = bare_root();
        base_root.dirs.push(annotated_dir("api", "api", C4Level::Container, "API"));

        let mut inc_root = bare_root();
        inc_root.dirs.push(annotated_dir("api", "api", C4Level::Component, "API"));

        let result = merge_ir(
            make_ir_with_root(base_root),
            make_ir_with_root(inc_root),
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.path, "api");
        assert!(err.message.contains("conflicting C4 levels"));
    }

    #[test]
    fn merge_base_preserved_when_incoming_none() {
        let mut base_root = bare_root();
        let mut api = annotated_dir("api", "api", C4Level::Container, "API");
        api.pattern = Some("Mediator".to_string());
        base_root.dirs.push(api);

        let inc_root = bare_root(); // no api dir

        let result = merge_ir(
            make_ir_with_root(base_root),
            make_ir_with_root(inc_root),
        ).unwrap();

        assert_eq!(result.root.dirs.len(), 1);
        assert_eq!(result.root.dirs[0].pattern.as_deref(), Some("Mediator"));
    }

    #[test]
    fn merge_files_incoming_wins() {
        let mut base_root = bare_root();
        base_root.files.push(FileNode {
            name: "lib.rs".to_string(),
            purpose: Some("Old purpose".to_string()),
            health: Some(HealthStatus::Planned),
            ..FileNode::bare("lib.rs")
        });

        let mut inc_root = bare_root();
        inc_root.files.push(FileNode {
            name: "lib.rs".to_string(),
            purpose: Some("New purpose".to_string()),
            health: Some(HealthStatus::Stable),
            ..FileNode::bare("lib.rs")
        });

        let result = merge_ir(
            make_ir_with_root(base_root),
            make_ir_with_root(inc_root),
        ).unwrap();

        assert_eq!(result.root.files.len(), 1);
        assert_eq!(result.root.files[0].purpose.as_deref(), Some("New purpose"));
        assert_eq!(result.root.files[0].health, Some(HealthStatus::Stable));
    }

    #[test]
    fn merge_empty_inputs_returns_empty() {
        let result = merge_ir(
            make_ir_with_root(bare_root()),
            make_ir_with_root(bare_root()),
        ).unwrap();
        assert!(result.root.dirs.is_empty());
        assert!(result.root.files.is_empty());
    }

    #[test]
    fn merge_scan_root_incoming_wins() {
        let base = ArchitectureIR {
            version: "2.0".to_string(),
            scan_root: "/base".to_string(),
            root: bare_root(),
        };
        let incoming = ArchitectureIR {
            version: "2.0".to_string(),
            scan_root: "/incoming".to_string(),
            root: bare_root(),
        };

        let result = merge_ir(base, incoming).unwrap();
        assert_eq!(result.scan_root, "/incoming");
    }
}
