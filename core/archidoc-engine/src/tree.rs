use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

/// Built-in directories to skip. Extended (not replaced) by config.tree.json exclude_dirs.
const DEFAULT_SKIP_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "target",
    "dist",
    ".ragd",
    ".vite",
    ".claude",
    "_archive",
    "_context",
];

/// File extensions to include. Replaced by config.tree.json include_extensions if non-empty.
const DEFAULT_INCLUDE_EXTENSIONS: &[&str] = &[
    ".md", ".rs", ".ts", ".js", ".toml", ".yaml", ".yml", ".json", ".py",
    ".sh", ".ps1", ".drawio", ".csv",
];

/// Files to always skip. Extended by config.tree.json exclude_files.
const DEFAULT_SKIP_FILES: &[&str] = &[
    "package-lock.json",
    "yarn.lock",
    "Cargo.lock",
    ".DS_Store",
];

/// Dirs with more than this many files get a count summary instead of inline listing.
const DEFAULT_INLINE_THRESHOLD: usize = 6;

// ── Config ────────────────────────────────────────────────────────────────────

/// Per-extension icon mapping for human-readable output.
pub struct IconConfig {
    /// Emoji for directories.
    pub directory: String,
    /// Fallback emoji for files with no specific mapping.
    pub file: String,
    /// Map from `.ext` (with leading dot) to emoji string.
    pub by_ext: HashMap<String, String>,
}

impl Default for IconConfig {
    fn default() -> Self {
        let by_ext = [
            (".md",    "📖"),
            (".rs",    "🔷"),
            (".ts",    "🟦"),
            (".js",    "🟨"),
            (".json",  "⚙️"),
            (".toml",  "⚙️"),
            (".yaml",  "🗂️"),
            (".yml",   "🗂️"),
            (".py",    "🐍"),
            (".sh",    "📜"),
            (".ps1",   "📜"),
            (".csv",   "📊"),
            (".drawio","🖊️"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

        Self {
            directory: "📁".to_string(),
            file: "📄".to_string(),
            by_ext,
        }
    }
}

/// Runtime configuration for tree generation, resolved from built-in defaults
/// merged with `.archidoc/config.tree.json` at the project root.
pub struct TreeConfig {
    /// Merged exclusion set: DEFAULT_SKIP_DIRS + user additions.
    /// Supports glob patterns: `*suffix`, `prefix*`, or exact name.
    pub exclude_dirs: Vec<String>,
    /// Merged exclusion set: DEFAULT_SKIP_FILES + user additions.
    /// Supports glob patterns: `*suffix`, `prefix*`, or exact name.
    pub exclude_files: Vec<String>,
    /// If config specifies extensions, replaces defaults. Otherwise uses DEFAULT_INCLUDE_EXTENSIONS.
    pub include_extensions: Vec<String>,
    /// Dirs with more files than this emit a count summary. Default: 6.
    pub inline_threshold: usize,
    /// Icon config for human-readable output (`--human`).
    pub icons: IconConfig,
}

impl TreeConfig {
    /// Returns a config populated entirely from built-in defaults.
    pub fn defaults() -> Self {
        Self {
            exclude_dirs: DEFAULT_SKIP_DIRS.iter().map(|s| s.to_string()).collect(),
            exclude_files: DEFAULT_SKIP_FILES.iter().map(|s| s.to_string()).collect(),
            include_extensions: DEFAULT_INCLUDE_EXTENSIONS.iter().map(|s| s.to_string()).collect(),
            inline_threshold: DEFAULT_INLINE_THRESHOLD,
            icons: IconConfig::default(),
        }
    }

    /// Load from `root/.archidoc/config.tree.json`, merging with built-in defaults.
    ///
    /// - Missing file → returns defaults silently.
    /// - Invalid JSON → prints a warning and returns defaults.
    /// - `exclude_dirs` / `exclude_files` → **additive** (built-ins always included).
    ///   Supports glob patterns: `*.jsonl`, `__pycache__`, etc.
    /// - `include_extensions` → **replaces** defaults if the array is non-empty.
    /// - `inline_threshold` → **overrides** the default if present.
    /// - `icons` → **merges** per-extension overrides over defaults.
    pub fn load(root: &Path) -> Self {
        let config_path = root.join(".archidoc").join("config.tree.json");
        let mut config = Self::defaults();

        let Ok(content) = fs::read_to_string(&config_path) else {
            return config;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else {
            eprintln!("warning: .archidoc/config.tree.json is not valid JSON — using defaults");
            return config;
        };

        // Additive: merge user exclude_dirs into the built-in set
        if let Some(arr) = value.get("exclude_dirs").and_then(|v| v.as_array()) {
            for item in arr {
                if let Some(s) = item.as_str() {
                    let s = s.to_string();
                    if !config.exclude_dirs.contains(&s) {
                        config.exclude_dirs.push(s);
                    }
                }
            }
        }

        // Additive: merge user exclude_files into the built-in set
        if let Some(arr) = value.get("exclude_files").and_then(|v| v.as_array()) {
            for item in arr {
                if let Some(s) = item.as_str() {
                    let s = s.to_string();
                    if !config.exclude_files.contains(&s) {
                        config.exclude_files.push(s);
                    }
                }
            }
        }

        // Replacing: non-empty include_extensions replaces defaults entirely
        if let Some(arr) = value.get("include_extensions").and_then(|v| v.as_array()) {
            if !arr.is_empty() {
                config.include_extensions = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect();
            }
        }

        // Override: inline_threshold replaces the default
        if let Some(n) = value.get("inline_threshold").and_then(|v| v.as_u64()) {
            config.inline_threshold = n as usize;
        }

        // Icons: merge per-extension overrides; top-level directory/file keys replace defaults
        if let Some(icons_obj) = value.get("icons") {
            if let Some(dir_icon) = icons_obj.get("directory").and_then(|v| v.as_str()) {
                config.icons.directory = dir_icon.to_string();
            }
            if let Some(file_icon) = icons_obj.get("file").and_then(|v| v.as_str()) {
                config.icons.file = file_icon.to_string();
            }
            if let Some(by_ext) = icons_obj.get("by_ext").and_then(|v| v.as_object()) {
                for (ext, icon) in by_ext {
                    if let Some(icon_str) = icon.as_str() {
                        config.icons.by_ext.insert(ext.clone(), icon_str.to_string());
                    }
                }
            }
        }

        config
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Generate a brace-expanded compact dirs-only tree (AI-optimized).
///
/// Each line shows one branch directory and its immediate subdirectory children
/// in a `{child1, child2}` brace list. Leaf directories appear only in their
/// parent's brace list, never as standalone lines.
///
/// ```text
/// ./ {0_White, 1_Yellow, 2_Blue, 3_Red, todo, tools}
/// 0_White/ {Framework, ICP, ICP-Workflows}
/// 0_White/Framework/ {-1_worldview, 0_terrain-thesis, 1_gestalt_methods}
/// ```
pub fn compact_dirs_tree(root: &Path, max_depth: Option<usize>, config: &TreeConfig) -> String {
    let mut out = String::new();
    let visible_dirs = visible_dir_set(root);
    brace_walk(&mut out, root, root, 0, max_depth, config, &visible_dirs);
    out
}

/// Generate a compact dirs+files tree (AI-optimized).
///
/// - Root files: `[root] file1, file2`
/// - ≤ inline_threshold files → inline: `path/ {file1, file2}`
/// - > inline_threshold files → count: `path/ [12 files: 10.md 2.rs]`
/// - Sibling dirs with identical file sets (≥ 3) → collapsed:
///   `parent/{d1, d2, d3}/ [each: f1, f2, f3]`
pub fn compact_files_tree(root: &Path, max_depth: Option<usize>, config: &TreeConfig) -> String {
    let mut out = String::new();

    let root_files = collect_files(root, config);
    if !root_files.is_empty() {
        out.push_str(&format!("[root] {}\n", root_files.join(", ")));
    }

    let visible_dirs = visible_dir_set(root);
    walk_files(&mut out, root, root, 0, max_depth, config, &visible_dirs);
    out
}

/// Build a bare `DirNode` tree from a live directory tree.
///
/// Uses the same filtering rules as `compact_dirs_tree` / `compact_files_tree`.
/// Strategy fields are left as `None` — populated later by annotation overlay.
pub fn build_dir_tree(
    root: &Path,
    config: &TreeConfig,
) -> archidoc_types::ir::DirNode {
    use archidoc_types::ir::{DirNode, FileNode};

    // Prune ignored / hidden directories from the tree.
    let visible_dirs = visible_dir_set(root);

    fn build_node(
        root: &Path,
        dir: &Path,
        config: &TreeConfig,
        visible_dirs: &HashSet<PathBuf>,
    ) -> DirNode {
        let name = if dir == root {
            ".".to_string()
        } else {
            dir.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
                .to_string()
        };

        let path = if dir == root {
            ".".to_string()
        } else {
            dir.strip_prefix(root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| name.clone())
        };

        let file_names = collect_files(dir, config);
        let files: Vec<FileNode> = file_names.into_iter().map(|n| FileNode::bare(&n)).collect();

        let mut subdirs = read_subdirs(dir, config, visible_dirs);
        subdirs.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

        let dirs: Vec<DirNode> = subdirs
            .iter()
            .map(|subdir| build_node(root, subdir, config, visible_dirs))
            .collect();

        DirNode {
            name,
            path,
            c4_level: None,
            description: None,
            pattern: None,
            pattern_status: None,
            content: None,
            source_file: None,
            parent: None,
            relationships: Vec::new(),
            dirs,
            files,
        }
    }

    build_node(root, root, config, &visible_dirs)
}

/// Render a dirs-only compact tree from a `DirNode`.
///
/// Produces the same format as `compact_dirs_tree` but reads from the
/// pre-built IR tree rather than the live filesystem.
pub fn render_dirs_tree(
    root: &archidoc_types::ir::DirNode,
    max_depth: Option<usize>,
) -> String {
    use archidoc_types::ir::DirNode;

    fn walk(out: &mut String, node: &DirNode, depth: usize, max_depth: Option<usize>) {
        if node.dirs.is_empty() {
            return; // leaf — appears only in parent's brace list
        }
        let child_names: Vec<String> = node.dirs.iter().map(|c| c.name.clone()).collect();
        out.push_str(&format!("{}/ {{{}}}\n", node.path, child_names.join(", ")));

        if let Some(max) = max_depth {
            if depth >= max {
                return;
            }
        }

        for child in &node.dirs {
            walk(out, child, depth + 1, max_depth);
        }
    }

    let mut out = String::new();
    walk(&mut out, root, 0, max_depth);
    out
}

/// Render a dirs+files compact tree from a `DirNode`.
///
/// Produces the same format as `compact_files_tree` but reads from the
/// pre-built IR tree rather than the live filesystem.
pub fn render_files_tree(
    root: &archidoc_types::ir::DirNode,
    max_depth: Option<usize>,
    config: &TreeConfig,
) -> String {
    use archidoc_types::ir::DirNode;

    fn file_names(node: &DirNode) -> Vec<String> {
        node.files.iter().map(|f| f.name.clone()).collect()
    }

    fn try_node_collapse(children: &[DirNode]) -> Option<SiblingCollapse> {
        if children.len() < 3 {
            return None;
        }
        for child in children {
            if !child.dirs.is_empty() {
                return None; // not a leaf
            }
            if child.files.is_empty() {
                return None;
            }
        }
        let first = file_names(&children[0]);
        if !children.iter().all(|c| file_names(c) == first) {
            return None;
        }
        Some(SiblingCollapse {
            names: children.iter().map(|c| c.name.clone()).collect(),
            files: first,
        })
    }

    fn walk(
        out: &mut String,
        node: &DirNode,
        depth: usize,
        max_depth: Option<usize>,
        config: &TreeConfig,
    ) {
        if let Some(max) = max_depth {
            if depth >= max {
                return;
            }
        }

        if let Some(collapse) = try_node_collapse(&node.dirs) {
            out.push_str(&format!(
                "{}/{{{}}}/  [each: {}]\n",
                node.path,
                collapse.names.join(", "),
                collapse.files.join(", ")
            ));
            return;
        }

        for child in &node.dirs {
            let child_files = file_names(child);
            let suffix = format_file_suffix(&child_files, config);
            if suffix.is_empty() {
                out.push_str(&format!("{}/\n", child.path));
            } else {
                out.push_str(&format!("{}/{}\n", child.path, suffix));
            }
            walk(out, child, depth + 1, max_depth, config);
        }
    }

    let mut out = String::new();

    let root_files = file_names(root);
    if !root_files.is_empty() {
        out.push_str(&format!("[root] {}\n", root_files.join(", ")));
    }

    walk(&mut out, root, 0, max_depth, config);
    out
}

// ── Glob pattern matching ─────────────────────────────────────────────────────

/// Match a name against a pattern.
///
/// Supports three forms:
/// - `*suffix` — name ends with suffix (e.g. `*.jsonl`)
/// - `prefix*` — name starts with prefix (e.g. `__pycache__*`)
/// - exact    — name == pattern (e.g. `node_modules`)
fn matches_pattern(pattern: &str, name: &str) -> bool {
    if let Some(suffix) = pattern.strip_prefix('*') {
        name.ends_with(suffix)
    } else if let Some(prefix) = pattern.strip_suffix('*') {
        name.starts_with(prefix)
    } else {
        pattern == name
    }
}

// ── Internal walkers ──────────────────────────────────────────────────────────

fn brace_walk(
    out: &mut String,
    root: &Path,
    dir: &Path,
    depth: usize,
    max_depth: Option<usize>,
    config: &TreeConfig,
    visible_dirs: &HashSet<PathBuf>,
) {
    let mut subdirs = read_subdirs(dir, config, visible_dirs);
    if subdirs.is_empty() {
        return; // leaf — appears only in parent's brace list
    }
    subdirs.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

    let rel = dir_rel(root, dir);
    let children: Vec<String> = subdirs
        .iter()
        .filter_map(|s| s.file_name().and_then(|n| n.to_str()).map(str::to_string))
        .collect();

    out.push_str(&format!("{}/ {{{}}}\n", rel, children.join(", ")));

    if let Some(max) = max_depth {
        if depth >= max {
            return;
        }
    }

    for subdir in &subdirs {
        brace_walk(out, root, subdir, depth + 1, max_depth, config, visible_dirs);
    }
}

fn walk_files(
    out: &mut String,
    root: &Path,
    dir: &Path,
    depth: usize,
    max_depth: Option<usize>,
    config: &TreeConfig,
    visible_dirs: &HashSet<PathBuf>,
) {
    if let Some(max) = max_depth {
        if depth >= max {
            return;
        }
    }

    let mut subdirs = read_subdirs(dir, config, visible_dirs);
    subdirs.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

    // Sibling-collapse: if all subdirs are leaves with identical non-empty file sets,
    // emit one compressed line instead of N individual lines.
    if let Some(collapse) = try_sibling_collapse(&subdirs, config, visible_dirs) {
        let prefix = dir_rel(root, dir);
        out.push_str(&format!(
            "{}/{{{}}}/  [each: {}]\n",
            prefix,
            collapse.names.join(", "),
            collapse.files.join(", ")
        ));
        return;
    }

    for subdir in subdirs {
        let rel_str = subdir
            .strip_prefix(root)
            .unwrap_or(&subdir)
            .to_string_lossy()
            .replace('\\', "/");
        let files = collect_files(&subdir, config);

        let suffix = format_file_suffix(&files, config);
        if suffix.is_empty() {
            out.push_str(&format!("{}/\n", rel_str));
        } else {
            out.push_str(&format!("{}/{}\n", rel_str, suffix));
        }

        walk_files(out, root, &subdir, depth + 1, max_depth, config, visible_dirs);
    }
}

// ── Sibling-collapse ──────────────────────────────────────────────────────────

struct SiblingCollapse {
    /// Sorted directory names.
    names: Vec<String>,
    /// The common file list shared by every sibling.
    files: Vec<String>,
}

/// Check whether all `subdirs` qualify for a single collapsed line.
///
/// Conditions (matching dir-tree compact mode):
/// 1. At least 3 siblings.
/// 2. Every sibling is a leaf dir (no nested subdirs of its own).
/// 3. Every sibling has an identical, non-empty file set.
fn try_sibling_collapse(
    subdirs: &[PathBuf],
    config: &TreeConfig,
    visible_dirs: &HashSet<PathBuf>,
) -> Option<SiblingCollapse> {
    if subdirs.len() < 3 {
        return None;
    }

    let mut signatures: Vec<Vec<String>> = Vec::with_capacity(subdirs.len());
    for subdir in subdirs {
        // Must be a leaf — no nested subdirs
        if !read_subdirs(subdir, config, visible_dirs).is_empty() {
            return None;
        }
        let files = collect_files(subdir, config);
        if files.is_empty() {
            return None; // non-empty signature required
        }
        signatures.push(files);
    }

    // All signatures must be identical
    let first = &signatures[0];
    if !signatures.iter().all(|s| s == first) {
        return None;
    }

    let names: Vec<String> = subdirs
        .iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_string))
        .collect();

    Some(SiblingCollapse {
        names,
        files: first.clone(),
    })
}

// ── File collection and formatting ────────────────────────────────────────────

fn collect_files(dir: &Path, config: &TreeConfig) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };

    let mut files: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            if !path.is_file() {
                return None;
            }
            let name = path.file_name()?.to_string_lossy().to_string();
            if config.exclude_files.iter().any(|p| matches_pattern(p, &name)) {
                return None;
            }
            let included = config.include_extensions.iter().any(|ext| name.ends_with(ext.as_str()));
            if included { Some(name) } else { None }
        })
        .collect();

    files.sort();
    files
}

/// Format the file suffix for a directory line.
///
/// - Empty               → empty string
/// - ≤ inline_threshold  → ` {file1, file2, ...}`
/// - > inline_threshold  → ` [N files: Xmd Yrs ...]`
fn format_file_suffix(files: &[String], config: &TreeConfig) -> String {
    if files.is_empty() {
        return String::new();
    }
    if files.len() <= config.inline_threshold {
        return format!(" {{{}}}", files.join(", "));
    }
    let mut ext_counts: HashMap<String, usize> = HashMap::new();
    for file in files {
        let ext = Path::new(file)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("other")
            .to_string();
        *ext_counts.entry(ext).or_default() += 1;
    }
    let mut ext_list: Vec<(String, usize)> = ext_counts.into_iter().collect();
    ext_list.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let breakdown: Vec<String> = ext_list
        .iter()
        .map(|(ext, count)| format!("{}.{}", count, ext))
        .collect();
    format!(" [{} files: {}]", files.len(), breakdown.join(" "))
}

/// The set of directories the `ignore` crate considers visible under `root`:
/// it honours `.gitignore` / `.ignore` files and skips hidden dirs (`.git`,
/// `.jj`, `target`, …). Used to prune ignored trees from every walk.
fn visible_dir_set(root: &Path) -> HashSet<PathBuf> {
    WalkBuilder::new(root)
        .build()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.into_path())
        .collect()
}

fn read_subdirs(
    dir: &Path,
    config: &TreeConfig,
    visible_dirs: &HashSet<PathBuf>,
) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };

    entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            if !path.is_dir() {
                return None;
            }
            // Pruned by `.gitignore` / `.ignore` or hidden (.git, .jj, target, …).
            if !visible_dirs.contains(&path) {
                return None;
            }
            let name = e.file_name();
            let name_str = name.to_string_lossy();
            if config.exclude_dirs.iter().any(|p| matches_pattern(p, name_str.as_ref())) {
                return None;
            }
            Some(path)
        })
        .collect()
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Relative path string from root to dir, using `/` separators. Root itself → `"."`.
fn dir_rel(root: &Path, dir: &Path) -> String {
    if dir == root {
        ".".to_string()
    } else {
        dir.strip_prefix(root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| dir.to_string_lossy().replace('\\', "/"))
    }
}

