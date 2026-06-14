use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use crate::annotation::{HealthStatus, PatternStatus};

/// C4 architecture level for a module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum C4Level {
    System,
    Container,
    Component,
    Unknown,
}

impl fmt::Display for C4Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::System => write!(f, "system"),
            Self::Container => write!(f, "container"),
            Self::Component => write!(f, "component"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

impl C4Level {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "container" => Self::Container,
            "component" => Self::Component,
            _ => Self::Unknown,
        }
    }
}

/// Universal intermediate representation for an architecture scan (v2.0).
///
/// A single nested directory tree carrying structure, strategy, and health.
/// Produced by `archidoc compile ir` and consumed by all other compile targets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectureIR {
    /// Schema version. Must be "2.0".
    pub version: String,
    /// Absolute path that was scanned (used for link resolution).
    pub scan_root: String,
    /// Root directory node. Always `path = "."`.
    pub root: DirNode,
}

/// A directory node in the architecture tree.
///
/// Carries optional strategy fields from `@c4` annotations.
/// Unannotated directories serialize compactly (only `name`, `path`, `dirs`, `files`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirNode {
    /// Directory basename (e.g., "api"). Root is ".".
    pub name: String,
    /// Path relative to scan_root, using `/` separators. Root is ".".
    pub path: String,

    // -- Strategy (from @c4 annotations; None if unannotated) --
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub c4_level: Option<C4Level>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern_status: Option<PatternStatus>,
    /// Raw doc comment content (narrative prose for renderers to filter).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Path to the annotation source file (relative to scan_root).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_file: Option<String>,
    /// Nearest annotated ancestor's path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,

    // -- Relationships --
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<Relationship>,

    // -- Children --
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dirs: Vec<DirNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<FileNode>,
}

/// A file node in the architecture tree.
///
/// Files listed in a `@c4` file table carry typed attributes.
/// Files not in any table appear with only `name` populated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNode {
    /// Filename (basename only).
    pub name: String,

    // -- Attributes (from file table in annotation) --
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern_status: Option<PatternStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<HealthStatus>,
    /// Extension attributes (extra/custom columns from file tables).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub extra: HashMap<String, String>,
}

/// A runtime dependency between directories.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relationship {
    /// Target directory path (relative to scan_root, forward slashes).
    pub target: String,
    pub label: String,
    pub protocol: String,
}

// ---------------------------------------------------------------------------
// Implementations
// ---------------------------------------------------------------------------

impl ArchitectureIR {
    pub fn new(scan_root: String) -> Self {
        Self {
            version: "2.0".to_string(),
            scan_root,
            root: DirNode::empty(".", "."),
        }
    }

    /// Deserialize from a JSON file. Rejects version != "2.x".
    pub fn load(path: &Path) -> Result<Self, String> {
        let json = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;

        // Peek at version before full parse
        let peek: serde_json::Value =
            serde_json::from_str(&json).map_err(|e| format!("invalid JSON: {}", e))?;
        let version = peek
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if version.starts_with("1.") || version.is_empty() {
            return Err(format!(
                "IR version '{}' is no longer supported. Re-run `archidoc compile ir` to regenerate.",
                version
            ));
        }

        serde_json::from_value(peek).map_err(|e| format!("invalid IR: {}", e))
    }

    /// Serialize and write to a JSON file. Creates parent directories as needed.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    format!("failed to create directory {}: {}", parent.display(), e)
                })?;
            }
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("failed to serialize IR: {}", e))?;
        std::fs::write(path, json)
            .map_err(|e| format!("failed to write {}: {}", path.display(), e))
    }

    /// All annotated DirNodes (c4_level.is_some()), depth-first.
    pub fn annotated_dirs(&self) -> Vec<&DirNode> {
        let mut result = Vec::new();
        collect_annotated(&self.root, &mut result);
        result
    }

    /// All DirNodes, depth-first.
    pub fn all_dirs(&self) -> Vec<&DirNode> {
        let mut result = Vec::new();
        collect_all(&self.root, &mut result);
        result
    }

    /// Find a DirNode by its relative path.
    pub fn find_dir(&self, path: &str) -> Option<&DirNode> {
        find_dir_recursive(&self.root, path)
    }
}

impl Default for ArchitectureIR {
    fn default() -> Self {
        Self::new(String::new())
    }
}

impl DirNode {
    pub fn empty(name: &str, path: &str) -> Self {
        Self {
            name: name.to_string(),
            path: path.to_string(),
            c4_level: None,
            description: None,
            pattern: None,
            pattern_status: None,
            content: None,
            source_file: None,
            parent: None,
            relationships: Vec::new(),
            dirs: Vec::new(),
            files: Vec::new(),
        }
    }

    pub fn is_annotated(&self) -> bool {
        self.c4_level.is_some()
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn total_file_count(&self) -> usize {
        self.files.len() + self.dirs.iter().map(|d| d.total_file_count()).sum::<usize>()
    }

    /// Zero files AND zero annotated children
    pub fn is_empty_leaf(&self) -> bool {
        self.files.is_empty()
            && self.dirs.iter().all(|d| !d.is_annotated() && d.is_empty_leaf())
    }

    pub fn is_populated(&self) -> bool {
        !self.files.is_empty()
    }

    /// Has files but all lack health/purpose/pattern_status (bare scaffold)
    pub fn is_scaffold(&self) -> bool {
        !self.files.is_empty()
            && self.files.iter().all(|f| {
                f.health.is_none() && f.purpose.is_none() && f.pattern_status.is_none()
            })
    }

    pub fn is_described(&self) -> bool {
        self.c4_level.is_some() || self.description.is_some()
    }

    /// File counts by extension, sorted: vec of ("ext", count)
    pub fn extension_counts(&self) -> Vec<(String, usize)> {
        use std::collections::BTreeMap;
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for f in &self.files {
            let ext = f.name.rsplit('.').next().unwrap_or("other").to_string();
            *counts.entry(ext).or_default() += 1;
        }
        counts.into_iter().collect()
    }
}

impl FileNode {
    pub fn bare(name: &str) -> Self {
        Self {
            name: name.to_string(),
            pattern: None,
            pattern_status: None,
            purpose: None,
            health: None,
            extra: HashMap::new(),
        }
    }
}


// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn collect_annotated<'a>(node: &'a DirNode, out: &mut Vec<&'a DirNode>) {
    if node.is_annotated() {
        out.push(node);
    }
    for child in &node.dirs {
        collect_annotated(child, out);
    }
}

fn collect_all<'a>(node: &'a DirNode, out: &mut Vec<&'a DirNode>) {
    out.push(node);
    for child in &node.dirs {
        collect_all(child, out);
    }
}

fn find_dir_recursive<'a>(node: &'a DirNode, path: &str) -> Option<&'a DirNode> {
    if node.path == path {
        return Some(node);
    }
    for child in &node.dirs {
        if let Some(found) = find_dir_recursive(child, path) {
            return Some(found);
        }
    }
    None
}
