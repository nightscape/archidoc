//! cargo-modules integration for import graph validation and orphan detection.
//!
//! Provides optional integration with the cargo-modules tool for:
//! - Extracting actual module dependency graph from Rust code
//! - Validating declared relationships against actual imports
//! - Detecting orphaned modules (undocumented modules)
//!
//! All functionality gracefully degrades if cargo-modules is not installed.

use archidoc_types::ModuleDoc;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;

/// Check if cargo-modules is available on the system.
///
/// Returns true if `cargo modules --version` succeeds.
pub fn check_cargo_modules_available() -> bool {
    Command::new("cargo")
        .args(["modules", "--version"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Import graph extracted from cargo-modules.
///
/// Contains nodes (module paths) and edges (dependencies between modules).
#[derive(Debug, Clone, Default)]
pub struct ImportGraph {
    /// Module paths that exist in the crate
    pub nodes: HashSet<String>,
    /// Dependencies: (from_module, to_module)
    pub edges: Vec<(String, String)>,
}

impl ImportGraph {
    /// Check if a dependency exists from one module to another.
    pub fn has_dependency(&self, from: &str, to: &str) -> bool {
        self.edges.iter().any(|(f, t)| f == from && t == to)
    }

    /// Get all dependencies of a module.
    pub fn get_dependencies(&self, module: &str) -> Vec<String> {
        self.edges
            .iter()
            .filter(|(f, _)| f == module)
            .map(|(_, t)| t.clone())
            .collect()
    }
}

/// Extract the import graph by running cargo-modules and parsing DOT output.
///
/// Returns Ok(graph) if cargo-modules succeeds, Err(message) otherwise.
pub fn extract_import_graph(root: &Path) -> Result<ImportGraph, String> {
    if !check_cargo_modules_available() {
        return Err("cargo-modules is not installed".to_string());
    }

    let output = Command::new("cargo")
        .args(["modules", "dependencies", "--layout", "dot"])
        .current_dir(root)
        .output()
        .map_err(|e| format!("Failed to run cargo modules: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("cargo modules failed: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_dot_output(&stdout)
}

/// Parse DOT format output from cargo-modules.
///
/// Expected format:
/// ```dot
/// digraph {
///   "crate_name" -> "module_a"
///   "module_a" -> "module_b"
///   ...
/// }
/// ```
fn parse_dot_output(dot: &str) -> Result<ImportGraph, String> {
    let mut graph = ImportGraph::default();

    for line in dot.lines() {
        let trimmed = line.trim();

        // Match edge: "from" -> "to"
        if let Some(arrow_pos) = trimmed.find("->") {
            let from_part = trimmed[..arrow_pos].trim();
            let to_part = trimmed[arrow_pos + 2..].trim();

            // Extract quoted strings
            let from = extract_quoted(from_part)?;
            let to = extract_quoted(to_part)?;

            // Convert crate::path to dot notation
            let from_module = crate_path_to_module(&from);
            let to_module = crate_path_to_module(&to);

            graph.nodes.insert(from_module.clone());
            graph.nodes.insert(to_module.clone());
            graph.edges.push((from_module, to_module));
        }
    }

    Ok(graph)
}

/// Extract content from quoted string.
fn extract_quoted(s: &str) -> Result<String, String> {
    if let Some(start) = s.find('"') {
        if let Some(end) = s[start + 1..].find('"') {
            return Ok(s[start + 1..start + 1 + end].to_string());
        }
    }
    Err(format!("Failed to extract quoted string from: {}", s))
}

/// Convert cargo-modules path format to dot notation.
///
/// Examples:
/// - "crate_name" -> "crate_name"
/// - "crate_name::module_a" -> "module_a"
/// - "crate_name::module_a::module_b" -> "module_a.module_b"
fn crate_path_to_module(path: &str) -> String {
    let parts: Vec<&str> = path.split("::").collect();

    if parts.len() <= 1 {
        // Top-level or crate root
        return parts[0].to_string();
    }

    // Skip crate name, join rest with dots
    parts[1..].join(".")
}

/// Warning about a relationship that doesn't match the import graph.
#[derive(Debug, Clone)]
pub struct RelationshipWarning {
    pub module: String,
    pub target: String,
    pub kind: WarningKind,
}

#[derive(Debug, Clone)]
pub enum WarningKind {
    /// Declared relationship but no actual import found
    NoImport,
    /// Import exists but no relationship declared
    Undeclared,
}

/// Validate declared relationships against the actual import graph.
///
/// Returns warnings for mismatches between documentation and code.
pub fn validate_relationships(
    docs: &[ModuleDoc],
    graph: &ImportGraph,
) -> Vec<RelationshipWarning> {
    let mut warnings = Vec::new();

    // Build a map of declared relationships
    let mut declared: HashMap<String, HashSet<String>> = HashMap::new();
    for doc in docs {
        let targets: HashSet<String> = doc
            .relationships
            .iter()
            .map(|r| r.target.clone())
            .collect();
        declared.insert(doc.module_path.clone(), targets);
    }

    // Check each documented module
    for doc in docs {
        let module = &doc.module_path;
        let actual_deps: HashSet<String> = graph
            .get_dependencies(module)
            .into_iter()
            .collect();

        let declared_deps = declared.get(module).cloned().unwrap_or_default();

        // Check for declared but not imported
        for target in &declared_deps {
            if !actual_deps.contains(target) {
                warnings.push(RelationshipWarning {
                    module: module.clone(),
                    target: target.clone(),
                    kind: WarningKind::NoImport,
                });
            }
        }

        // Check for imported but not declared
        for target in &actual_deps {
            if !declared_deps.contains(target) {
                warnings.push(RelationshipWarning {
                    module: module.clone(),
                    target: target.clone(),
                    kind: WarningKind::Undeclared,
                });
            }
        }
    }

    warnings
}

/// Detect orphaned modules (exist in code but not documented).
///
/// Returns module paths that exist in the import graph but have no documentation.
pub fn detect_orphans(docs: &[ModuleDoc], graph: &ImportGraph) -> Vec<String> {
    let documented: HashSet<String> = docs
        .iter()
        .map(|d| d.module_path.clone())
        .collect();

    graph
        .nodes
        .iter()
        .filter(|node| !documented.contains(*node))
        .cloned()
        .collect()
}

/// Detect orphaned modules by running cargo-modules orphans command.
///
/// Returns list of module paths that are orphaned (not imported by anything).
pub fn detect_orphans_cmd(root: &Path) -> Result<Vec<String>, String> {
    if !check_cargo_modules_available() {
        return Err("cargo-modules is not installed".to_string());
    }

    let output = Command::new("cargo")
        .args(["modules", "orphans"])
        .current_dir(root)
        .output()
        .map_err(|e| format!("Failed to run cargo modules: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("cargo modules failed: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse output - typically one module per line
    let orphans: Vec<String> = stdout
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .map(|line| crate_path_to_module(line))
        .collect();

    Ok(orphans)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crate_path_to_module() {
        assert_eq!(crate_path_to_module("my_crate"), "my_crate");
        assert_eq!(crate_path_to_module("my_crate::core"), "core");
        assert_eq!(
            crate_path_to_module("my_crate::core::types"),
            "core.types"
        );
    }

    #[test]
    fn test_extract_quoted() {
        assert_eq!(
            extract_quoted(r#""my_crate::core""#).unwrap(),
            "my_crate::core"
        );
        assert_eq!(extract_quoted(r#"  "module"  "#).unwrap(), "module");
    }

    #[test]
    fn test_parse_dot_output() {
        let dot = r#"
digraph {
  "my_crate" -> "my_crate::core"
  "my_crate::core" -> "my_crate::utils"
}
"#;

        let graph = parse_dot_output(dot).unwrap();
        assert!(graph.nodes.contains("core"));
        assert!(graph.nodes.contains("utils"));
        assert!(graph.has_dependency("core", "utils"));
    }

    #[test]
    fn test_import_graph_operations() {
        let mut graph = ImportGraph::default();
        graph.nodes.insert("core".to_string());
        graph.nodes.insert("utils".to_string());
        graph.edges.push(("core".to_string(), "utils".to_string()));

        assert!(graph.has_dependency("core", "utils"));
        assert!(!graph.has_dependency("utils", "core"));

        let deps = graph.get_dependencies("core");
        assert_eq!(deps, vec!["utils"]);
    }

    #[test]
    fn test_validate_relationships_no_import() {
        use archidoc_types::{C4Level, PatternStatus, Relationship};

        let docs = vec![ModuleDoc {
            module_path: "core".to_string(),
            content: "test".to_string(),
            source_file: "test.rs".to_string(),
            c4_level: C4Level::Component,
            pattern: "--".to_string(),
            pattern_status: PatternStatus::Planned,
            description: "test".to_string(),
            parent_container: None,
            relationships: vec![Relationship {
                target: "utils".to_string(),
                label: "test".to_string(),
                protocol: "Rust".to_string(),
            }],
            files: vec![],
            code_elements: vec![],
        }];

        let graph = ImportGraph::default(); // Empty graph

        let warnings = validate_relationships(&docs, &graph);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].module, "core");
        assert_eq!(warnings[0].target, "utils");
        assert!(matches!(warnings[0].kind, WarningKind::NoImport));
    }

    #[test]
    fn test_detect_orphans() {
        use archidoc_types::{C4Level, PatternStatus};

        let docs = vec![ModuleDoc {
            module_path: "core".to_string(),
            content: "test".to_string(),
            source_file: "test.rs".to_string(),
            c4_level: C4Level::Component,
            pattern: "--".to_string(),
            pattern_status: PatternStatus::Planned,
            description: "test".to_string(),
            parent_container: None,
            relationships: vec![],
            files: vec![],
            code_elements: vec![],
        }];

        let mut graph = ImportGraph::default();
        graph.nodes.insert("core".to_string());
        graph.nodes.insert("utils".to_string());
        graph.nodes.insert("database".to_string());

        let orphans = detect_orphans(&docs, &graph);
        assert_eq!(orphans.len(), 2);
        assert!(orphans.contains(&"utils".to_string()));
        assert!(orphans.contains(&"database".to_string()));
    }

    #[test]
    fn test_check_cargo_modules_available() {
        // This test will pass/fail based on whether cargo-modules is installed
        // It's informational - won't fail the build
        let available = check_cargo_modules_available();
        println!("cargo-modules available: {}", available);
    }
}
