use std::collections::HashMap;
use std::fs;
use std::path::Path;

use archidoc_types::{
    C4Level, FileEntry, HealthStatus, PatternStatus, Relationship,
};

/// Extract `//!` doc comments from a Rust source file.
///
/// Returns the joined content of all leading `//!` lines, with prefixes stripped.
pub fn archidoc_from_file(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;

    let doc_lines: Vec<&str> = content
        .lines()
        .take_while(|line| {
            let trimmed = line.trim();
            trimmed.starts_with("//!") || trimmed.is_empty()
        })
        .filter(|line| line.trim().starts_with("//!"))
        .map(|line| {
            let trimmed = line.trim();
            if trimmed == "//!" {
                ""
            } else if let Some(rest) = trimmed.strip_prefix("//! ") {
                rest
            } else {
                trimmed.strip_prefix("//!").unwrap_or("")
            }
        })
        .collect();

    if doc_lines.is_empty() {
        None
    } else {
        Some(doc_lines.join("\n"))
    }
}

/// Extract the C4 level marker from doc content.
///
/// Uses `@c4 container` / `@c4 component` syntax.
pub fn extract_c4_level(content: &str) -> C4Level {
    if content.contains("@c4 container") {
        C4Level::Container
    } else if content.contains("@c4 component") {
        C4Level::Component
    } else {
        C4Level::Unknown
    }
}

/// Extract the primary GoF pattern name from doc content.
///
/// Priority order:
/// 1. Explicit `Pattern:` line in the annotation (structured, reliable)
/// 2. Substring search for known pattern names in content (heuristic fallback)
/// 3. "--" if nothing found
pub fn extract_pattern(content: &str) -> String {
    // Priority 1: explicit "Pattern:" annotation line
    if let Some(pattern) = extract_explicit_pattern(content) {
        return pattern;
    }

    // Priority 2: heuristic substring search on description lines only
    // (excludes file table rows and @c4 markers to avoid false positives)
    // Order matters — longer/more specific patterns first to avoid
    // partial matches (e.g. "Chain of Responsibility" before "Command")
    let description_text: String = content
        .lines()
        .filter(|l| {
            let t = l.trim();
            !t.starts_with('|') && !t.starts_with("@c4 ") && !t.starts_with("Pattern:")
        })
        .collect::<Vec<_>>()
        .join("\n");

    let patterns = [
        "Chain of Responsibility",
        "Active Object",
        "Template Method",
        "Abstract Factory",
        "Value Object",
        "Mediator",
        "Observer",
        "Strategy",
        "Facade",
        "Adapter",
        "Repository",
        "Singleton",
        "Factory",
        "Builder",
        "Decorator",
        "Memento",
        "Command",
        "Iterator",
        "Composite",
        "Interpreter",
        "Flyweight",
        "Publisher",
        "Prototype",
        "Bridge",
        "Proxy",
        "State",
        "Visitor",
        "Core",
    ];

    for name in patterns {
        if description_text.contains(name) {
            return name.to_string();
        }
    }

    "--".to_string()
}

/// Extract an explicit "Pattern: X" or "Pattern: X (verified)" line from doc content.
///
/// Looks for a line starting with "Pattern:" and extracts the pattern name.
/// Handles optional status suffix like "(verified)" or "(planned)".
fn extract_explicit_pattern(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Pattern:") {
            let rest = rest.trim();
            if rest.is_empty() || rest == "--" {
                continue;
            }
            // Strip optional status suffix: "Facade (verified)" -> "Facade"
            let pattern = if let Some(idx) = rest.find('(') {
                rest[..idx].trim()
            } else {
                rest
            };
            if !pattern.is_empty() {
                return Some(pattern.to_string());
            }
        }
    }
    None
}

/// Extract pattern status from doc content.
///
/// Checks for status on the explicit `Pattern:` line first (e.g. "Pattern: Facade (verified)"),
/// then falls back to searching for "(verified)" anywhere in content. Defaults to Planned.
pub fn extract_pattern_status(content: &str) -> PatternStatus {
    // Priority 1: status on the "Pattern:" line
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Pattern:") {
            let rest = rest.trim();
            if let Some(idx) = rest.find('(') {
                let status_str = rest[idx + 1..]
                    .trim_end_matches(')')
                    .trim();
                return PatternStatus::parse(status_str);
            }
        }
    }

    // Priority 2: "(verified)" anywhere in content (legacy fallback)
    if content.contains("(verified)") {
        PatternStatus::Verified
    } else {
        PatternStatus::Planned
    }
}

/// True for lines that are structure, not prose: the `@c4` directive, Markdown
/// headings, table rows, and the `Pattern:` / `GoF:` metadata lines.
fn is_marker_line(trimmed: &str) -> bool {
    trimmed.starts_with('#')
        || trimmed.starts_with("@c4 ")
        || trimmed.starts_with('|')
        || trimmed.starts_with("GoF:")
        || trimmed.starts_with("Pattern:")
}

/// Extract the human-readable description: the first **paragraph** of prose.
///
/// A paragraph is a run of consecutive non-empty, non-marker lines. Leading
/// blank and marker lines (`@c4 …`, `#`, `|`, `Pattern:`, `GoF:`) are skipped;
/// collection then stops at the first blank or marker line. The collected lines
/// are joined with a single space, so a sentence wrapped across several `//!`
/// lines is reassembled rather than truncated at the first wrap.
///
/// This generalises the previous first-line behaviour: a one-line description
/// followed by a blank line yields exactly that line, unchanged. Only a
/// description whose *first paragraph spans multiple lines* changes — it is now
/// joined instead of cut at the first line break.
pub fn extract_description(content: &str) -> String {
    let mut paragraph: Vec<&str> = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || is_marker_line(trimmed) {
            if paragraph.is_empty() {
                continue; // still skipping leading blanks / markers
            }
            break; // prose has ended
        }
        paragraph.push(trimmed);
    }

    if paragraph.is_empty() {
        "*No description*".to_string()
    } else {
        paragraph.join(" ")
    }
}

/// Extract the parent container from a dot-notation module path.
///
/// "bus.calc.indicators" -> Some("bus")
/// "bus" -> None
pub fn extract_parent_container(module_path: &str) -> Option<String> {
    if module_path.contains('.') {
        Some(
            module_path
                .split('.')
                .next()
                .unwrap_or(module_path)
                .to_string(),
        )
    } else {
        None
    }
}

/// Known file table column names mapped to the field they populate.
///
/// Lookup is case-insensitive by lowercase name. Any column not in this
/// table is treated as an unknown column and stored in `FileEntry::extra`.
const KNOWN_FILE_COLUMNS: &[(&str, FileCol)] = &[
    ("file", FileCol::Name),
    ("name", FileCol::Name),
    ("pattern", FileCol::Pattern),
    ("purpose", FileCol::Purpose),
    ("description", FileCol::Purpose),
    ("health", FileCol::Health),
    ("status", FileCol::Health),
];

#[derive(Clone, Copy)]
enum FileCol {
    Name,
    Pattern,
    Purpose,
    Health,
}

fn classify_header(col: &str) -> Option<FileCol> {
    let lower = col.trim().to_lowercase();
    KNOWN_FILE_COLUMNS
        .iter()
        .find(|(name, _)| *name == lower.as_str())
        .map(|(_, kind)| *kind)
}

/// Parse the markdown file table into FileEntry structs.
///
/// Detects the header row by the presence of a "File" or "Name" column.
/// Column order is driven by the header, not hardcoded positions.
/// Unknown column names are stored in `FileEntry::extra`.
pub fn extract_file_table(content: &str) -> Vec<FileEntry> {
    let mut entries = Vec::new();
    let mut col_kinds: Vec<Option<FileCol>> = Vec::new();
    let mut col_names: Vec<String> = Vec::new();
    let mut in_table = false;
    let mut header_seen = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if !in_table {
            if trimmed.starts_with('|') {
                let header_cells: Vec<&str> = trimmed
                    .split('|')
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| s.trim())
                    .collect();
                let has_file_col = header_cells.iter().any(|c| {
                    let lower = c.to_lowercase();
                    lower == "file" || lower == "name"
                });
                if has_file_col {
                    col_kinds = header_cells.iter().map(|c| classify_header(c)).collect();
                    col_names = header_cells.iter().map(|c| c.to_lowercase()).collect();
                    in_table = true;
                    continue;
                }
            }
        } else if !header_seen {
            if trimmed.starts_with('|') && trimmed.contains("---") {
                header_seen = true;
                continue;
            }
        } else {
            if !trimmed.starts_with('|') {
                break;
            }

            let cells: Vec<&str> = trimmed
                .split('|')
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.trim())
                .collect();

            let mut name = String::new();
            let mut pattern = "--".to_string();
            let mut pattern_status = PatternStatus::Planned;
            let mut purpose = String::new();
            let mut health = HealthStatus::Planned;
            let mut extra: HashMap<String, String> = HashMap::new();

            for (i, &cell) in cells.iter().enumerate() {
                match col_kinds.get(i) {
                    Some(Some(FileCol::Name)) => {
                        name = cell.trim_matches('`').trim().to_string();
                    }
                    Some(Some(FileCol::Pattern)) => {
                        let (p, ps) = parse_pattern_field(cell);
                        pattern = p;
                        pattern_status = ps;
                    }
                    Some(Some(FileCol::Purpose)) => {
                        purpose = cell.trim().to_string();
                    }
                    Some(Some(FileCol::Health)) => {
                        health = HealthStatus::parse(cell);
                    }
                    _ => {
                        if let Some(col_name) = col_names.get(i) {
                            extra.insert(col_name.clone(), cell.trim().to_string());
                        }
                    }
                }
            }

            if !name.is_empty() {
                entries.push(FileEntry {
                    name,
                    pattern,
                    pattern_status,
                    purpose,
                    health,
                    extra,
                });
            }
        }
    }

    entries
}

/// Parse a pattern field like "Strategy (verified)" into (pattern, status).
fn parse_pattern_field(field: &str) -> (String, PatternStatus) {
    let trimmed = field.trim();

    if let Some(idx) = trimmed.find('(') {
        let pattern = trimmed[..idx].trim().to_string();
        let status_str = trimmed[idx + 1..]
            .trim_end_matches(')')
            .trim();
        (pattern, PatternStatus::parse(status_str))
    } else {
        (trimmed.to_string(), PatternStatus::Planned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_pattern_single_word() {
        let content = "@c4 component\n\nSome description.\n\nPattern: Facade";
        assert_eq!(extract_pattern(content), "Facade");
    }

    #[test]
    fn explicit_pattern_multi_word() {
        let content = "@c4 component\n\nSome description.\n\nPattern: Value Object";
        assert_eq!(extract_pattern(content), "Value Object");
    }

    #[test]
    fn explicit_pattern_chain_of_responsibility() {
        let content = "@c4 component\n\nValidation pipeline.\n\nPattern: Chain of Responsibility";
        assert_eq!(extract_pattern(content), "Chain of Responsibility");
    }

    #[test]
    fn explicit_pattern_with_verified_status() {
        let content = "@c4 component\n\nEntry point.\n\nPattern: Strategy (verified)";
        assert_eq!(extract_pattern(content), "Strategy");
        assert_eq!(extract_pattern_status(content), PatternStatus::Verified);
    }

    #[test]
    fn explicit_pattern_overrides_heuristic() {
        // Description contains "Factory" but Pattern line says "Builder"
        let content = "@c4 component\n\nFactory for creating widgets.\n\nPattern: Builder";
        assert_eq!(extract_pattern(content), "Builder");
    }

    #[test]
    fn heuristic_fallback_when_no_pattern_line() {
        let content = "@c4 container\n\nCentral Facade for the module.";
        assert_eq!(extract_pattern(content), "Facade");
    }

    #[test]
    fn heuristic_ignores_file_table_patterns() {
        // File table contains "Value Object" but description doesn't mention a pattern
        let content = "@c4 container\n\nShared domain types.\n\n| File | Pattern | Purpose | Health |\n|------|---------|---------|--------|\n| `types.rs` | Value Object | Domain types | stable |";
        assert_eq!(extract_pattern(content), "--");
    }

    #[test]
    fn no_false_positive_from_registry_in_description() {
        // "RegistryEntry" in description should not match "Registry"
        // (Registry was removed from the heuristic pattern list)
        let content = "@c4 component\n\nPattern: Value Object\n\nDomain types: RegistryEntry, Primitive.";
        assert_eq!(extract_pattern(content), "Value Object");
    }

    #[test]
    fn description_skips_pattern_line() {
        let content = "@c4 component\n\nPattern: Facade\n\nActual description here.";
        assert_eq!(extract_description(content), "Actual description here.");
    }

    #[test]
    fn description_single_line_unchanged() {
        // The common case: one line, then a blank line, then body prose.
        let content = "@c4 component\n\nCore traits for datasources.\n\nMore detail here.";
        assert_eq!(extract_description(content), "Core traits for datasources.");
    }

    #[test]
    fn description_joins_wrapped_paragraph() {
        // A sentence wrapped across lines is reassembled, not truncated.
        let content = "@c4 component\n\nCross-PBT transition traits shared between\n`a` and `b`.\n\nDetail.";
        assert_eq!(
            extract_description(content),
            "Cross-PBT transition traits shared between `a` and `b`."
        );
    }

    #[test]
    fn description_stops_at_marker_without_blank() {
        // A marker line ends the paragraph even with no intervening blank line.
        let content = "@c4 component\n\nShort summary.\nPattern: Facade";
        assert_eq!(extract_description(content), "Short summary.");
    }

    #[test]
    fn description_empty_is_placeholder() {
        assert_eq!(extract_description("@c4 component\n"), "*No description*");
    }

    #[test]
    fn pattern_dash_dash_is_skipped() {
        let content = "@c4 component\n\nSome description.\n\nPattern: --";
        assert_eq!(extract_pattern(content), "--");
    }
}

/// Parse `@c4 uses target "label" "protocol"` markers from content.
pub fn extract_relationships(content: &str) -> Vec<Relationship> {
    let mut rels = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("@c4 uses ") {
            // Parse: target "label" "protocol"
            // Split on first quote to get target, then extract quoted strings
            if let Some(quote_start) = rest.find('"') {
                let target = rest[..quote_start].trim().to_string();
                let quoted_part = &rest[quote_start..];
                let quotes: Vec<&str> = quoted_part
                    .split('"')
                    .filter(|s| !s.trim().is_empty())
                    .collect();
                if quotes.len() >= 2 {
                    rels.push(Relationship {
                        target,
                        label: quotes[0].to_string(),
                        protocol: quotes[1].to_string(),
                    });
                }
            }
        }
    }

    rels
}
