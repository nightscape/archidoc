use std::fs;
use std::path::PathBuf;

use archidoc_types::ir::ArchitectureIR;
use archidoc_types::{HealthReport, ModuleDoc, ValidationReport, DriftReport};
use tempfile::TempDir;

use crate::drivers::protocol_driver::ArchitectureDriver;
use crate::fakes::fake_source_tree::FakeSourceTree;

/// In-memory architecture driver for unit tests.
///
/// Combines the full pipeline: creates annotated files in a temp dir,
/// runs the real parser, then generates ARCHITECTURE.md content in memory.
/// Assertions check the parsed IR and the generated ARCHITECTURE.md string.
pub struct InMemoryArchitectureDriver {
    source_tree: FakeSourceTree,
    results: Vec<ModuleDoc>,
    ir: Option<ArchitectureIR>,
    architecture_content: Option<String>,
    output_dir: TempDir,
    compiled: bool,
    ir_json: Option<String>,
    ir_snapshots: std::collections::HashMap<String, String>,
    merged_ir: Option<ArchitectureIR>,
}

impl InMemoryArchitectureDriver {
    pub fn new() -> Self {
        Self {
            source_tree: FakeSourceTree::new(),
            results: Vec::new(),
            ir: None,
            architecture_content: None,
            output_dir: TempDir::new().expect("failed to create output temp dir"),
            compiled: false,
            ir_json: None,
            ir_snapshots: std::collections::HashMap::new(),
            merged_ir: None,
        }
    }

    fn find_module(&self, name: &str) -> &ModuleDoc {
        self.results
            .iter()
            .find(|doc| doc.module_path == name)
            .unwrap_or_else(|| {
                panic!(
                    "element '{}' not found. Available: {:?}",
                    name,
                    self.results.iter().map(|d| &d.module_path).collect::<Vec<_>>()
                )
            })
    }

    fn arch_content(&self) -> &str {
        self.architecture_content
            .as_deref()
            .expect("ARCHITECTURE.md not generated — call compile() first")
    }

    fn arch_file_path(&self) -> PathBuf {
        self.output_dir.path().join("ARCHITECTURE.md")
    }

    fn generate_architecture(&mut self) {
        let ir = self.ir.as_ref().expect("IR not built");
        let content = archidoc_engine::architecture::generate(ir, &[]);
        fs::write(self.arch_file_path(), &content)
            .expect("failed to write ARCHITECTURE.md");
        self.architecture_content = Some(content);
    }

    fn current_ir(&self) -> &ArchitectureIR {
        self.ir.as_ref().expect("IR not built — call compile() first")
    }
}

/// Convert an ArchitectureIR back to Vec<ModuleDoc> for legacy protocol driver methods.
/// This is a test-only helper that replicates the deleted `to_module_docs()`.
fn ir_to_module_docs(ir: &ArchitectureIR) -> Vec<ModuleDoc> {
    use archidoc_types::module_doc::{FileEntry, Relationship as ModRelationship};

    let mut docs = Vec::new();

    fn collect(
        node: &archidoc_types::ir::DirNode,
        scan_root: &str,
        is_root: bool,
        out: &mut Vec<ModuleDoc>,
    ) {
        if node.is_annotated() {
            let module_path = if is_root {
                "_lib".to_string()
            } else {
                node.path.replace('/', ".")
            };

            let source_file = match &node.source_file {
                Some(rel) => {
                    let root = std::path::Path::new(scan_root);
                    let native_rel = rel.replace('/', std::path::MAIN_SEPARATOR_STR);
                    root.join(native_rel).to_string_lossy().to_string()
                }
                None => String::new(),
            };

            let files: Vec<FileEntry> = node
                .files
                .iter()
                .filter(|f| f.health.is_some() || f.purpose.is_some() || f.pattern.is_some())
                .map(|f| FileEntry {
                    name: f.name.clone(),
                    pattern: f.pattern.clone().unwrap_or_else(|| "--".to_string()),
                    pattern_status: f.pattern_status.unwrap_or_default(),
                    purpose: f.purpose.clone().unwrap_or_default(),
                    health: f.health.unwrap_or_default(),
                    extra: f.extra.clone(),
                })
                .collect();

            let relationships: Vec<ModRelationship> = node
                .relationships
                .iter()
                .map(|r| ModRelationship {
                    target: r.target.clone(),
                    label: r.label.clone(),
                    protocol: r.protocol.clone(),
                })
                .collect();

            let parent_container = node.parent.as_ref().map(|p| {
                if p == "." { "_lib".to_string() } else { p.replace('/', ".") }
            });

            out.push(ModuleDoc {
                module_path,
                content: node.content.clone().unwrap_or_default(),
                source_file,
                c4_level: node.c4_level.unwrap_or(archidoc_types::C4Level::Unknown),
                pattern: node.pattern.clone().unwrap_or_else(|| "--".to_string()),
                pattern_status: node.pattern_status.unwrap_or_default(),
                description: node.description.clone().unwrap_or_default(),
                parent_container,
                relationships,
                files,
                code_elements: Vec::new(),
            });
        }

        for child in &node.dirs {
            collect(child, scan_root, false, out);
        }
    }

    collect(&ir.root, &ir.scan_root, true, &mut docs);
    docs.sort_by(|a, b| a.module_path.cmp(&b.module_path));
    docs
}

impl ArchitectureDriver for InMemoryArchitectureDriver {
    fn create_annotated_source(&mut self, name: &str, content: &str) {
        self.source_tree.create_module(name, content);
    }

    fn compile(&mut self) {
        let src_dir = self.source_tree.root().join("src");
        self.results = archidoc_rust::walker::extract_all_docs(&src_dir);
        // Build IR from results
        let ir = archidoc_engine::ir_builder::build_ir(
            &src_dir,
            archidoc_types::ir::DirNode::empty(".", "."),
            self.results.clone(),
        );
        self.ir = Some(ir);
        self.generate_architecture();
        self.compiled = true;
    }

    fn compiled_modules(&self) -> &[ModuleDoc] {
        &self.results
    }

    // --- ARCHITECTURE.md ---

    fn confirm_architecture_produced(&self) {
        assert!(
            self.architecture_content.is_some(),
            "ARCHITECTURE.md not produced — was compile() called?"
        );
    }

    fn confirm_architecture_contains(&self, expected: &str) {
        let content = self.arch_content();
        assert!(
            content.contains(expected),
            "ARCHITECTURE.md does not contain '{}'. Content:\n{}",
            expected, content
        );
    }

    fn confirm_index_lists(&self, name: &str) {
        let content = self.arch_content();
        assert!(
            content.contains(name),
            "component index does not list '{}'. Content:\n{}",
            name, content
        );
    }

    // --- Diagrams (inline in ARCHITECTURE.md) ---

    fn confirm_diagram_shows_container(&self, container: &str) {
        let content = self.arch_content();
        let mermaid_id = container.replace('.', "_").replace('/', "_");
        assert!(
            content.contains(&mermaid_id),
            "container diagram does not show '{}' (id: '{}'). Content:\n{}",
            container, mermaid_id, content
        );
    }

    fn confirm_diagram_shows_component(&self, component: &str, inside: &str) {
        let content = self.arch_content();
        let mermaid_id = component.replace('.', "_").replace('/', "_");
        assert!(
            content.contains(&mermaid_id),
            "component diagram does not show '{}' (id: '{}'). Content:\n{}",
            component, mermaid_id, content
        );
        if !inside.is_empty() {
            assert!(
                content.contains(inside),
                "component diagram does not show container '{}'. Content:\n{}",
                inside, content
            );
        }
    }

    fn confirm_diagram_shows_dependency(&self, from: &str, to: &str) {
        let content = self.arch_content();
        let from_id = from.replace('.', "_").replace('/', "_");
        let to_id = to.replace('.', "_").replace('/', "_");
        let rel_pattern = format!("Rel({}, {}", from_id, to_id);
        assert!(
            content.contains(&rel_pattern),
            "no dependency arrow from '{}' to '{}' in ARCHITECTURE.md (looked for '{}')",
            from, to, rel_pattern
        );
    }

    // --- Architecture structure ---

    fn confirm_element_level(&self, name: &str, expected_level: &str) {
        let doc = self.find_module(name);
        assert_eq!(
            doc.c4_level.to_string(), expected_level,
            "element '{}': expected level '{}', got '{}'",
            name, expected_level, doc.c4_level
        );
    }

    fn confirm_design_pattern(&self, name: &str, expected_pattern: &str) {
        let doc = self.find_module(name);
        assert_eq!(
            doc.pattern, expected_pattern,
            "element '{}': expected pattern '{}', got '{}'",
            name, expected_pattern, doc.pattern
        );
    }

    fn confirm_pattern_confidence(&self, name: &str, expected_confidence: &str) {
        let doc = self.find_module(name);
        assert_eq!(
            doc.pattern_status.to_string(), expected_confidence,
            "element '{}': expected confidence '{}', got '{}'",
            name, expected_confidence, doc.pattern_status
        );
    }

    fn confirm_containment(&self, component: &str, inside: &str) {
        let doc = self.find_module(component);
        let actual = doc.parent_container.as_deref().unwrap_or("");
        assert_eq!(
            actual, inside,
            "element '{}': expected inside '{}', got '{}'",
            component, inside, actual
        );
    }

    fn confirm_top_level(&self, name: &str) {
        let doc = self.find_module(name);
        assert!(
            doc.parent_container.is_none(),
            "element '{}': expected top-level but found parent '{}'",
            name, doc.parent_container.as_deref().unwrap_or("")
        );
    }

    fn confirm_total_elements(&self, expected: usize) {
        assert_eq!(
            self.results.len(), expected,
            "expected {} elements, got {}. Elements: {:?}",
            expected, self.results.len(),
            self.results.iter().map(|d| &d.module_path).collect::<Vec<_>>()
        );
    }

    // --- File catalog ---

    fn confirm_catalog_entry(
        &self,
        element: &str,
        filename: &str,
        design_pattern: &str,
        responsibility: &str,
        maturity: &str,
    ) {
        let doc = self.find_module(element);
        let entry = doc
            .files
            .iter()
            .find(|f| f.name == filename)
            .unwrap_or_else(|| {
                panic!(
                    "element '{}': catalog entry '{}' not found. Available: {:?}",
                    element, filename,
                    doc.files.iter().map(|f| &f.name).collect::<Vec<_>>()
                )
            });

        if !design_pattern.is_empty() {
            let actual = if design_pattern.contains('(') {
                format!("{} ({})", entry.pattern, entry.pattern_status)
            } else {
                entry.pattern.clone()
            };
            assert_eq!(
                actual, design_pattern,
                "element '{}', file '{}': expected pattern '{}', got '{}'",
                element, filename, design_pattern, actual
            );
        }
        if !responsibility.is_empty() {
            assert_eq!(
                entry.purpose, responsibility,
                "element '{}', file '{}': expected responsibility '{}', got '{}'",
                element, filename, responsibility, entry.purpose
            );
        }
        if !maturity.is_empty() {
            assert_eq!(
                entry.health.to_string(), maturity,
                "element '{}', file '{}': expected maturity '{}', got '{}'",
                element, filename, maturity, entry.health
            );
        }
    }

    fn confirm_catalog_size(&self, element: &str, expected_count: usize) {
        let doc = self.find_module(element);
        assert_eq!(
            doc.files.len(), expected_count,
            "element '{}': expected {} catalog entries, got {}",
            element, expected_count, doc.files.len()
        );
    }

    // --- Dependencies ---

    fn confirm_dependency(
        &self,
        from: &str,
        to: &str,
        label: &str,
        protocol: &str,
    ) {
        let doc = self.find_module(from);
        let found = doc.relationships.iter().any(|r| {
            r.target == to
                && (label.is_empty() || r.label == label)
                && (protocol.is_empty() || r.protocol == protocol)
        });
        assert!(
            found,
            "element '{}': dependency to '{}' (label: '{}', protocol: '{}') not found. Available: {:?}",
            from, to, label, protocol,
            doc.relationships.iter().map(|r| format!("{}/{}/{}", r.target, r.label, r.protocol)).collect::<Vec<_>>()
        );
    }

    // =========================================================================
    // Phase B — Health reporting
    // =========================================================================

    fn request_health_report(&self) -> HealthReport {
        archidoc_engine::health::aggregate_health(self.current_ir())
    }

    fn confirm_health_file_count(&self, maturity: &str, expected: usize) {
        let report = self.request_health_report();
        let actual = match maturity {
            "planned" => report.files_planned,
            "active" => report.files_active,
            "stable" => report.files_stable,
            _ => panic!("unknown maturity: '{}'", maturity),
        };
        assert_eq!(
            actual, expected,
            "health report: expected {} '{}' files, got {}",
            expected, maturity, actual
        );
    }

    fn confirm_health_pattern_count(&self, confidence: &str, expected: usize) {
        let report = self.request_health_report();
        let actual = match confidence {
            "planned" => report.patterns_planned,
            "verified" => report.patterns_verified,
            _ => panic!("unknown confidence: '{}'", confidence),
        };
        assert_eq!(
            actual, expected,
            "health report: expected {} '{}' patterns, got {}",
            expected, confidence, actual
        );
    }

    fn confirm_health_total_files(&self, expected: usize) {
        let report = self.request_health_report();
        assert_eq!(
            report.total_files, expected,
            "health report: expected {} total files, got {}",
            expected, report.total_files
        );
    }

    // =========================================================================
    // Phase B — Validation (ghost/orphan detection)
    // =========================================================================

    fn place_file_on_disk(&mut self, element: &str, filename: &str) {
        self.source_tree.place_extra_file(element, filename);
    }

    fn remove_file_from_disk(&mut self, element: &str, filename: &str) {
        self.source_tree.remove_file(element, filename);
    }

    fn validate(&self) -> ValidationReport {
        archidoc_engine::validate::validate_file_tables(self.current_ir())
    }

    fn confirm_ghost(&self, element: &str, filename: &str) {
        let report = self.validate();
        let found = report.ghosts.iter().any(|g| g.element == element && g.filename == filename);
        assert!(
            found,
            "expected ghost '{}' in element '{}'. Ghosts: {:?}",
            filename, element,
            report.ghosts.iter().map(|g| format!("{}/{}", g.element, g.filename)).collect::<Vec<_>>()
        );
    }

    fn confirm_orphan(&self, element: &str, filename: &str) {
        let report = self.validate();
        let found = report.orphans.iter().any(|o| o.element == element && o.filename == filename);
        assert!(
            found,
            "expected orphan '{}' in element '{}'. Orphans: {:?}",
            filename, element,
            report.orphans.iter().map(|o| format!("{}/{}", o.element, o.filename)).collect::<Vec<_>>()
        );
    }

    fn confirm_validation_clean(&self) {
        let report = self.validate();
        assert!(
            report.is_clean(),
            "expected clean validation but found {} ghosts and {} orphans",
            report.ghosts.len(), report.orphans.len()
        );
    }

    // =========================================================================
    // Phase B — Drift detection
    // =========================================================================

    fn modify_source_annotation(&mut self, name: &str, new_purpose: &str) {
        let content = format!(
            "@c4 container\n\n# {}\n\n{}\n",
            name.split('.').last().unwrap_or(name),
            new_purpose
        );
        self.source_tree.create_module(name, &content);
    }

    fn check_for_drift(&self) -> DriftReport {
        let src_dir = self.source_tree.root().join("src");
        let fresh_docs = archidoc_rust::walker::extract_all_docs(&src_dir);
        let fresh_ir = archidoc_engine::ir_builder::build_ir(
            &src_dir,
            archidoc_types::ir::DirNode::empty(".", "."),
            fresh_docs,
        );
        let link_base = self.output_dir.path().to_path_buf();
        archidoc_engine::check::check_drift(&fresh_ir, &self.arch_file_path(), &link_base)
    }

    fn confirm_drift_detected(&self) {
        let report = self.check_for_drift();
        assert!(
            report.has_drift(),
            "expected drift but documentation appears up to date"
        );
    }

    fn confirm_no_drift(&self) {
        let report = self.check_for_drift();
        assert!(
            !report.has_drift(),
            "expected no drift but found: {} drifted, {} missing, {} extra",
            report.drifted_files.len(),
            report.missing_files.len(),
            report.extra_files.len()
        );
    }

    // =========================================================================
    // Phase D — Portable IR
    // =========================================================================

    fn emit_ir(&mut self) {
        assert!(self.compiled, "must compile before emitting IR");
        let ir = self.current_ir();
        self.ir_json = Some(archidoc_engine::ir::serialize_ir(ir));
    }

    fn ir_json(&self) -> &str {
        self.ir_json
            .as_deref()
            .expect("IR not emitted yet — call emit_ir() first")
    }

    fn compile_from_ir(&mut self) {
        let json = self.ir_json().to_string();
        let ir = archidoc_engine::ir::deserialize_ir(&json)
            .expect("failed to deserialize IR");
        self.results = ir_to_module_docs(&ir);
        self.ir = Some(ir);
        self.generate_architecture();
        self.compiled = true;
    }

    fn confirm_ir_contains_element(&self, name: &str, level: &str) {
        let json = self.ir_json();
        let ir = archidoc_engine::ir::deserialize_ir(json)
            .expect("failed to parse IR for assertion");
        let docs = ir_to_module_docs(&ir);
        let found = docs.iter().any(|d| d.module_path == name && d.c4_level.to_string() == level);
        assert!(
            found,
            "IR does not contain element '{}' at level '{}'. Elements: {:?}",
            name, level,
            docs.iter().map(|d| format!("{} ({})", d.module_path, d.c4_level)).collect::<Vec<_>>()
        );
    }

    fn confirm_ir_round_trip_fidelity(&self) {
        let json = self.ir_json();
        let ir = archidoc_engine::ir::deserialize_ir(json)
            .expect("failed to deserialize IR for round-trip check");
        let round_tripped = ir_to_module_docs(&ir);

        assert_eq!(
            self.results.len(),
            round_tripped.len(),
            "round-trip changed element count: {} -> {}",
            self.results.len(),
            round_tripped.len()
        );

        for (original, restored) in self.results.iter().zip(round_tripped.iter()) {
            assert_eq!(
                original, restored,
                "round-trip fidelity lost for element '{}'",
                original.module_path
            );
        }
    }

    fn confirm_ir_schema_valid(&self) {
        let json = self.ir_json();
        archidoc_engine::ir::deserialize_ir(json)
            .expect("IR schema validation failed");
    }

    fn confirm_ir_rejects(&self, json: &str) {
        let result = archidoc_engine::ir::deserialize_ir(json);
        assert!(
            result.is_err(),
            "expected malformed IR to be rejected but it was accepted"
        );
    }

    fn write_ir_to_file(&mut self) {
        let json = self.ir_json().to_string();
        let path = self.output_dir.path().join("ir_export.json");
        fs::write(&path, &json).expect("failed to write IR to file");
    }

    fn compile_from_ir_file(&mut self) {
        let path = self.output_dir.path().join("ir_export.json");
        let json = fs::read_to_string(&path)
            .expect("failed to read IR from file — was write_ir_to_file called?");
        let ir = archidoc_engine::ir::deserialize_ir(&json)
            .expect("failed to deserialize IR from file");
        self.results = ir_to_module_docs(&ir);
        self.ir = Some(ir);
        self.generate_architecture();
        self.compiled = true;
    }

    fn confirm_ir_idempotent(&mut self) {
        let first_ir = self.ir_json().to_string();
        let ir = archidoc_engine::ir::deserialize_ir(&first_ir)
            .expect("failed to deserialize first IR");
        let second_ir = archidoc_engine::ir::serialize_ir(&ir);
        assert_eq!(
            first_ir, second_ir,
            "IR is not idempotent — second emission differs from first"
        );
    }

    // =========================================================================
    // Phase H — Pattern validation
    // =========================================================================

    fn create_code_file(&mut self, element: &str, filename: &str, code: &str) {
        self.source_tree.create_code_file(element, filename, code);
    }

    fn verify_patterns(&mut self) {
        assert!(self.compiled, "must compile before verifying patterns");
        archidoc_rust::promote::auto_promote(&mut self.results);
    }

    fn confirm_fitness_passes(&self, fitness_name: &str) {
        let result = archidoc_rust::fitness::run_fitness(fitness_name, &self.results)
            .unwrap_or_else(|| panic!("unknown fitness function: '{}'", fitness_name));
        assert!(
            result.passed,
            "expected fitness '{}' to pass but {} failure(s): {:?}",
            fitness_name,
            result.failures.len(),
            result.failures.iter().map(|f| &f.module_path).collect::<Vec<_>>()
        );
    }

    fn confirm_fitness_fails(&self, fitness_name: &str, failing_module: &str) {
        let result = archidoc_rust::fitness::run_fitness(fitness_name, &self.results)
            .unwrap_or_else(|| panic!("unknown fitness function: '{}'", fitness_name));
        assert!(
            !result.passed,
            "expected fitness '{}' to fail but it passed ({} checked)",
            fitness_name, result.checked
        );
        let has_module = result
            .failures
            .iter()
            .any(|f| f.module_path == failing_module);
        assert!(
            has_module,
            "expected '{}' in fitness failures but found: {:?}",
            failing_module,
            result.failures.iter().map(|f| &f.module_path).collect::<Vec<_>>()
        );
    }

    // =========================================================================
    // Phase L — IR merging
    // =========================================================================

    fn save_ir_snapshot(&mut self, name: &str) {
        assert!(self.compiled, "must compile before saving IR snapshot");
        let ir = self.current_ir();
        let json = archidoc_engine::ir::serialize_ir(ir);
        self.ir_snapshots.insert(name.to_string(), json);
    }

    fn merge_ir_snapshots(&mut self, names: &[&str]) {
        let mut irs: Vec<ArchitectureIR> = names.iter().map(|name| {
            let json = self.ir_snapshots.get(*name)
                .unwrap_or_else(|| panic!("IR snapshot '{}' not found", name));
            archidoc_engine::ir::deserialize_ir(json)
                .unwrap_or_else(|e| panic!("failed to deserialize snapshot '{}': {}", name, e))
        }).collect();

        let mut merged = irs.remove(0);
        for ir in irs {
            match archidoc_engine::merge::merge_ir(merged, ir) {
                Ok(m) => merged = m,
                Err(e) => panic!("merge failed: {}", e),
            }
        }
        self.merged_ir = Some(merged);
    }

    fn confirm_merged_element_count(&self, expected: usize) {
        let merged_ir = self.merged_ir.as_ref()
            .expect("no merged results — call merge_ir_snapshots first");
        let merged = ir_to_module_docs(merged_ir);
        assert_eq!(
            merged.len(), expected,
            "expected {} merged elements, got {}. Elements: {:?}",
            expected, merged.len(),
            merged.iter().map(|d| &d.module_path).collect::<Vec<_>>()
        );
    }

    fn confirm_merged_contains(&self, name: &str, level: &str) {
        let merged_ir = self.merged_ir.as_ref()
            .expect("no merged results — call merge_ir_snapshots first");
        let merged = ir_to_module_docs(merged_ir);
        let found = merged.iter().any(|d| d.module_path == name && d.c4_level.to_string() == level);
        assert!(
            found,
            "merged IR does not contain '{}' at level '{}'. Elements: {:?}",
            name, level,
            merged.iter().map(|d| format!("{} ({})", d.module_path, d.c4_level)).collect::<Vec<_>>()
        );
    }
}
