//! Crate-level dependency edges from `cargo metadata`.
//!
//! Complements [`crate::cargo_modules`] (module-level, needs the external
//! `cargo-modules` tool) with a workspace-native, zero-extra-tooling source:
//! `cargo metadata` ships with every Cargo install. Edges are crate→crate
//! (workspace members only), which lines up with `@c4 component` granularity.
//!
//! The output is the same [`ImportGraph`] the `cargo_modules` path produces,
//! so the existing [`crate::cargo_modules::validate_relationships`] diff and
//! [`RelationshipWarning`] types apply unchanged.

use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

use archidoc_types::ir::ArchitectureIR;

use crate::cargo_modules::{ImportGraph, RelationshipWarning, WarningKind};

/// Default dependency names to ignore (build/tooling artifacts, not architecture).
pub const DEFAULT_IGNORE: &[&str] = &["workspace-hack"];

/// Build a crate-level [`ImportGraph`] from `cargo metadata`.
///
/// Nodes are all workspace member crate names. Edges are normal (non-dev,
/// non-build) dependencies between members. `ignore` names are dropped from
/// both nodes and edges.
///
/// Fails loud: a missing/oversized/garbled `cargo metadata` is returned as an
/// `Err`, never silently swallowed into an empty graph.
pub fn workspace_import_graph(
    manifest_dir: &Path,
    ignore: &HashSet<String>,
) -> Result<ImportGraph, String> {
    let output = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(manifest_dir)
        .output()
        .map_err(|e| format!("failed to run `cargo metadata` in {manifest_dir:?}: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("`cargo metadata` failed: {stderr}"));
    }

    let meta: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("`cargo metadata` produced invalid JSON: {e}"))?;

    let packages = meta["packages"]
        .as_array()
        .ok_or("`cargo metadata` JSON has no `packages` array")?;

    let members: HashSet<String> = packages
        .iter()
        .filter_map(|p| p["name"].as_str())
        .map(str::to_string)
        .filter(|n| !ignore.contains(n))
        .collect();

    let mut graph = ImportGraph::default();
    for member in &members {
        graph.nodes.insert(member.clone());
    }

    for pkg in packages {
        let from = match pkg["name"].as_str() {
            Some(n) if members.contains(n) => n.to_string(),
            _ => continue,
        };
        let deps = match pkg["dependencies"].as_array() {
            Some(d) => d,
            None => continue,
        };
        for dep in deps {
            // `kind` is null for normal deps, "dev"/"build" otherwise.
            if !dep["kind"].is_null() {
                continue;
            }
            let to = match dep["name"].as_str() {
                Some(n) if members.contains(n) && n != from => n.to_string(),
                _ => continue,
            };
            let edge = (from.clone(), to);
            if !graph.edges.contains(&edge) {
                graph.edges.push(edge);
            }
        }
    }

    Ok(graph)
}

/// Diff the `@c4 uses` relationships declared in a compiled IR against the
/// real crate-dependency graph.
///
/// Returns one [`RelationshipWarning`] per drift:
/// - [`WarningKind::NoImport`] — declared `@c4 uses` with no real dependency
///   (a stale arrow that should be removed), and
/// - [`WarningKind::Undeclared`] — real dependency with no `@c4 uses`
///   (a missing arrow that should be added).
///
/// Only annotated crate-level components are compared (their `name` must be a
/// workspace member); sub-module dirs and un-annotated crates are skipped.
pub fn validate_ir_relationships(
    ir: &ArchitectureIR,
    graph: &ImportGraph,
    ignore: &HashSet<String>,
) -> Vec<RelationshipWarning> {
    let mut warnings = Vec::new();

    for dir in ir.annotated_dirs() {
        let crate_name = dir.name.clone();
        if !graph.nodes.contains(&crate_name) || ignore.contains(&crate_name) {
            continue;
        }

        let declared: HashSet<String> = dir
            .relationships
            .iter()
            .map(|r| r.target.clone())
            .filter(|t| !ignore.contains(t))
            .collect();
        let actual: HashSet<String> = graph
            .get_dependencies(&crate_name)
            .into_iter()
            .filter(|t| !ignore.contains(t))
            .collect();

        for target in &declared {
            if !actual.contains(target) {
                warnings.push(RelationshipWarning {
                    module: crate_name.clone(),
                    target: target.clone(),
                    kind: WarningKind::NoImport,
                });
            }
        }
        for target in &actual {
            if !declared.contains(target) {
                warnings.push(RelationshipWarning {
                    module: crate_name.clone(),
                    target: target.clone(),
                    kind: WarningKind::Undeclared,
                });
            }
        }
    }

    warnings.sort_by(|a, b| {
        (&a.module, &a.target, a.kind.clone() as u8)
            .cmp(&(&b.module, &b.target, b.kind.clone() as u8))
    });
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use archidoc_types::ir::{C4Level, DirNode, Relationship};

    fn component(name: &str, uses: &[&str]) -> DirNode {
        let mut d = DirNode::empty(name, name);
        d.c4_level = Some(C4Level::Component);
        d.relationships = uses
            .iter()
            .map(|t| Relationship {
                target: t.to_string(),
                label: String::new(),
                protocol: "Rust".to_string(),
            })
            .collect();
        d
    }

    fn ir_with(components: Vec<DirNode>) -> ArchitectureIR {
        let mut ir = ArchitectureIR::new("crates".to_string());
        ir.root.dirs = components;
        ir
    }

    fn graph(nodes: &[&str], edges: &[(&str, &str)]) -> ImportGraph {
        let mut g = ImportGraph::default();
        for n in nodes {
            g.nodes.insert(n.to_string());
        }
        g.edges = edges
            .iter()
            .map(|(f, t)| (f.to_string(), t.to_string()))
            .collect();
        g
    }

    #[test]
    fn clean_when_declared_matches_real() {
        let ir = ir_with(vec![component("core", &["api"]), component("api", &[])]);
        let g = graph(&["core", "api"], &[("core", "api")]);
        let w = validate_ir_relationships(&ir, &g, &HashSet::new());
        assert!(w.is_empty(), "expected no drift, got {w:?}");
    }

    #[test]
    fn flags_missing_and_stale() {
        // core really depends on api (undeclared) and declares a bogus turso edge (stale).
        let ir = ir_with(vec![
            component("core", &["turso"]),
            component("api", &[]),
            component("turso", &[]),
        ]);
        let g = graph(&["core", "api", "turso"], &[("core", "api")]);
        let w = validate_ir_relationships(&ir, &g, &HashSet::new());

        let missing: Vec<_> = w
            .iter()
            .filter(|x| matches!(x.kind, WarningKind::Undeclared))
            .map(|x| (x.module.as_str(), x.target.as_str()))
            .collect();
        let stale: Vec<_> = w
            .iter()
            .filter(|x| matches!(x.kind, WarningKind::NoImport))
            .map(|x| (x.module.as_str(), x.target.as_str()))
            .collect();

        assert_eq!(missing, vec![("core", "api")]);
        assert_eq!(stale, vec![("core", "turso")]);
    }

    #[test]
    fn ignore_list_suppresses_both_directions() {
        let ir = ir_with(vec![component("core", &["workspace-hack"])]);
        let g = graph(&["core"], &[("core", "workspace-hack")]);
        let ignore: HashSet<String> = ["workspace-hack".to_string()].into_iter().collect();
        let w = validate_ir_relationships(&ir, &g, &ignore);
        assert!(w.is_empty(), "ignored dep must not drift, got {w:?}");
    }
}
