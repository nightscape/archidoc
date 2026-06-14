use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use archidoc_types::ir::{ArchitectureIR, DirNode};
use archidoc_types::C4Level;

/// Generate PlantUML C4 container diagram.
pub fn generate_container(output_dir: &Path, ir: &ArchitectureIR) {
    let filepath = output_dir.join("c4-container.puml");
    fs::write(&filepath, container_diagram(ir)).expect("Failed to write c4-container.puml");
}

/// Build the PlantUML C4 container diagram body.
///
/// When any container declares an `@c4 layer`, the containers are grouped into
/// nested `Container_Boundary` blocks (one per layer, falling back to the
/// directory-derived parent, then "Other") inside the `System` boundary —
/// mirroring [`generate_component`]. With no layers declared the System holds a
/// flat container list, exactly as before.
fn container_diagram(ir: &ArchitectureIR) -> String {
    let all_annotated = ir.annotated_dirs();
    let containers: Vec<&DirNode> = all_annotated
        .iter()
        .copied()
        .filter(|d| d.c4_level == Some(C4Level::Container))
        .collect();

    let container_line = |dir: &DirNode| {
        format!(
            "Container({}, \"{}\", \"{}\", \"{}\")",
            to_puml_id(&dir.path),
            escape_label(&to_title_case(&dir.name)),
            escape_label(dir.pattern.as_deref().unwrap_or("--")),
            escape_label(dir.description.as_deref().unwrap_or(""))
        )
    };

    let mut container_defs = String::new();
    if containers.iter().any(|d| d.layer.is_some()) {
        let mut grouped: BTreeMap<String, Vec<&DirNode>> = BTreeMap::new();
        for dir in &containers {
            let group = dir
                .layer
                .clone()
                .or_else(|| dir.parent.clone())
                .unwrap_or_else(|| "Other".to_string());
            grouped.entry(group).or_default().push(dir);
        }
        for (layer, layer_dirs) in &grouped {
            let layer_id = to_puml_id(layer);
            let layer_name = to_title_case(layer.split('/').last().unwrap_or(layer));
            container_defs.push_str(&format!(
                "    Container_Boundary({}_layer, \"{}\") {{\n",
                layer_id, layer_name
            ));
            for dir in layer_dirs {
                container_defs.push_str(&format!("        {}\n", container_line(dir)));
            }
            container_defs.push_str("    }\n");
        }
    } else {
        for dir in &containers {
            container_defs.push_str(&format!("    {}\n", container_line(dir)));
        }
    }

    // `@c4 system` nodes are external systems the containers talk to — render
    // them outside the boundary so cross-level `Rel(...)` arrows resolve.
    let mut system_defs = String::new();
    for dir in systems_of(ir) {
        let id = to_puml_id(&dir.path);
        let name = escape_label(&to_title_case(&dir.name));
        let desc = escape_label(dir.description.as_deref().unwrap_or(""));
        system_defs.push_str(&format!(
            "System_Ext({}, \"{}\", \"{}\")\n",
            id, name, desc
        ));
    }

    let mut rel_defs = String::new();
    for dir in &containers {
        let from_id = to_puml_id(&dir.path);
        for rel in &dir.relationships {
            let to_id = to_puml_id(&rel.target);
            rel_defs.push_str(&format!(
                "Rel({}, {}, \"{}\", \"{}\")\n",
                from_id,
                to_id,
                escape_label(&rel.label),
                escape_label(&rel.protocol)
            ));
        }
    }

    format!(
        r#"@startuml c4-container
!include https://raw.githubusercontent.com/plantuml-stdlib/C4-PlantUML/master/C4_Container.puml

title Container Diagram

System_Boundary(sys, "System") {{
{}}}

{}
{}
@enduml
"#,
        container_defs, system_defs, rel_defs
    )
}

/// Annotated `@c4 system` nodes.
fn systems_of(ir: &ArchitectureIR) -> Vec<&DirNode> {
    ir.annotated_dirs()
        .into_iter()
        .filter(|d| d.c4_level == Some(C4Level::System))
        .collect()
}

/// Generate the PlantUML C4 system-context diagram from `@c4 system` nodes.
///
/// Renders one `System(...)` per `@c4 system` annotation plus every relationship
/// declared on those nodes. Emits nothing if no system-level node exists, so the
/// diagram only appears once a project declares its context.
pub fn generate_context(output_dir: &Path, ir: &ArchitectureIR) {
    let systems = systems_of(ir);
    if systems.is_empty() {
        return;
    }

    let mut system_defs = String::new();
    for dir in &systems {
        let id = to_puml_id(&dir.path);
        let name = escape_label(&to_title_case(&dir.name));
        let desc = escape_label(dir.description.as_deref().unwrap_or(""));
        system_defs.push_str(&format!(
            "System({}, \"{}\", \"{}\")\n",
            id, name, desc
        ));
    }

    let mut rel_defs = String::new();
    for dir in &systems {
        let from_id = to_puml_id(&dir.path);
        for rel in &dir.relationships {
            let to_id = to_puml_id(&rel.target);
            rel_defs.push_str(&format!(
                "Rel({}, {}, \"{}\", \"{}\")\n",
                from_id,
                to_id,
                escape_label(&rel.label),
                escape_label(&rel.protocol)
            ));
        }
    }

    let content = format!(
        r#"@startuml c4-context
!include https://raw.githubusercontent.com/plantuml-stdlib/C4-PlantUML/master/C4_Context.puml

title System Context Diagram

{}
{}
@enduml
"#,
        system_defs, rel_defs
    );

    fs::write(output_dir.join("c4-context.puml"), content)
        .expect("Failed to write c4-context.puml");
}

/// Generate PlantUML C4 component diagram.
pub fn generate_component(output_dir: &Path, ir: &ArchitectureIR) {
    let filepath = output_dir.join("c4-component.puml");

    let all_annotated = ir.annotated_dirs();
    let components: Vec<&DirNode> = all_annotated
        .iter()
        .copied()
        .filter(|d| d.c4_level == Some(C4Level::Component))
        .collect();

    // Group by explicit `@c4 layer` when set, else by directory-derived parent.
    let mut grouped: BTreeMap<String, Vec<&DirNode>> = BTreeMap::new();
    for dir in &components {
        let group = dir
            .layer
            .clone()
            .or_else(|| dir.parent.clone())
            .unwrap_or_else(|| "other".to_string());
        grouped.entry(group).or_default().push(dir);
    }

    let mut boundary_defs = String::new();
    for (parent, component_dirs) in &grouped {
        let parent_id = to_puml_id(parent);
        let parent_name = escape_label(&to_title_case(parent.split('/').last().unwrap_or(parent)));
        boundary_defs.push_str(&format!(
            "Container_Boundary({}_boundary, \"{}\") {{\n",
            parent_id, parent_name
        ));
        for dir in component_dirs {
            let id = to_puml_id(&dir.path);
            let name = escape_label(&dir.name);
            let pattern = escape_label(dir.pattern.as_deref().unwrap_or("--"));
            let desc = escape_label(dir.description.as_deref().unwrap_or(""));
            boundary_defs.push_str(&format!(
                "    Component({}, \"{}\", \"{}\", \"{}\")\n",
                id, name, pattern, desc
            ));
        }
        boundary_defs.push_str("}\n\n");
    }

    let mut rel_defs = String::new();
    for dir in &components {
        let from_id = to_puml_id(&dir.path);
        for rel in &dir.relationships {
            let to_id = to_puml_id(&rel.target);
            rel_defs.push_str(&format!(
                "Rel({}, {}, \"{}\", \"{}\")\n",
                from_id,
                to_id,
                escape_label(&rel.label),
                escape_label(&rel.protocol)
            ));
        }
    }

    let content = format!(
        r#"@startuml c4-component
!include https://raw.githubusercontent.com/plantuml-stdlib/C4-PlantUML/master/C4_Component.puml

title Component Diagram (GoF Patterns)

{}{}
@enduml
"#,
        boundary_defs, rel_defs
    );

    fs::write(&filepath, content).expect("Failed to write c4-component.puml");
}

/// Generate the PlantUML C4 code diagram from `@c4 code` elements.
///
/// Each annotated component that declares code elements becomes a
/// `Container_Boundary`, with one `Component(...)` per element (kind shown as
/// the technology tag) and any `@c4 uses` relationships as `Rel(...)` arrows.
/// Emits nothing when no code element exists.
pub fn generate_code(output_dir: &Path, ir: &ArchitectureIR) {
    let owners: Vec<&DirNode> = ir
        .annotated_dirs()
        .into_iter()
        .filter(|d| !d.code_elements.is_empty())
        .collect();
    if owners.is_empty() {
        return;
    }

    // Map a code element's bare name to its qualified puml id so that
    // intra-component `@c4 uses StorageEntity` arrows land on the defined node
    // instead of auto-creating a bare one.
    let mut by_name: BTreeMap<&str, String> = BTreeMap::new();
    for owner in &owners {
        for el in &owner.code_elements {
            by_name.insert(
                el.name.as_str(),
                to_puml_id(&format!("{}__{}", owner.path, el.name)),
            );
        }
    }

    let mut boundary_defs = String::new();
    let mut rel_defs = String::new();
    for owner in &owners {
        let boundary_id = to_puml_id(&owner.path);
        boundary_defs.push_str(&format!(
            "Container_Boundary({}_code, \"{}\") {{\n",
            boundary_id,
            escape_label(&owner.name)
        ));
        for el in &owner.code_elements {
            let id = to_puml_id(&format!("{}__{}", owner.path, el.name));
            let desc = escape_label(el.description.as_deref().unwrap_or(""));
            boundary_defs.push_str(&format!(
                "    Component({}, \"{}\", \"{}\", \"{}\")\n",
                id,
                escape_label(&el.name),
                escape_label(&el.kind),
                desc
            ));
            for rel in &el.relationships {
                let to_id = by_name
                    .get(rel.target.as_str())
                    .cloned()
                    .unwrap_or_else(|| to_puml_id(&rel.target));
                rel_defs.push_str(&format!(
                    "Rel({}, {}, \"{}\", \"{}\")\n",
                    id,
                    to_id,
                    escape_label(&rel.label),
                    escape_label(&rel.protocol)
                ));
            }
        }
        boundary_defs.push_str("}\n\n");
    }

    let content = format!(
        r#"@startuml c4-code
!include https://raw.githubusercontent.com/plantuml-stdlib/C4-PlantUML/master/C4_Component.puml

title Code Diagram (@c4 code elements)

{}{}
@enduml
"#,
        boundary_defs, rel_defs
    );

    fs::write(output_dir.join("c4-code.puml"), content)
        .expect("Failed to write c4-code.puml");
}

fn to_puml_id(s: &str) -> String {
    s.replace('.', "_").replace('/', "_").replace('-', "_")
}

fn to_title_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().to_string() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Sanitize a string for embedding inside a PlantUML double-quoted argument.
///
/// C4/PlantUML macro arguments are double-quoted and have no escape sequence for
/// an embedded `"`, so a quote in a description or label (e.g. a doc comment
/// reading `e.g. "block"`) would prematurely close the string and corrupt the
/// diagram. Replace any `"` with `'` and flatten newlines to spaces so arbitrary
/// annotation text renders safely.
fn escape_label(s: &str) -> String {
    s.replace('"', "'").replace(['\n', '\r'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use archidoc_types::ir::{ArchitectureIR, DirNode};
    use archidoc_types::C4Level;

    fn container(name: &str, layer: Option<&str>) -> DirNode {
        let mut d = DirNode::empty(name, name);
        d.c4_level = Some(C4Level::Container);
        d.layer = layer.map(|s| s.to_string());
        d
    }

    fn ir_with(containers: Vec<DirNode>) -> ArchitectureIR {
        let mut ir = ArchitectureIR::new("/scan".to_string());
        ir.root.dirs = containers;
        ir
    }

    #[test]
    fn escape_label_neutralizes_quotes_and_newlines() {
        assert_eq!(escape_label(r#"e.g. "block", "doc""#), "e.g. 'block', 'doc'");
        assert_eq!(escape_label("line1\nline2"), "line1 line2");
        assert_eq!(escape_label("plain"), "plain");
    }

    #[test]
    fn component_description_with_quotes_does_not_break_the_string() {
        let mut node = DirNode::empty("api", "api");
        node.c4_level = Some(C4Level::Component);
        node.description = Some(r#"Typed name (e.g. "block")"#.to_string());
        let mut ir = ArchitectureIR::new("/scan".to_string());
        ir.root.dirs = vec![node];

        let dir = std::env::temp_dir().join("archidoc_escape_dev_test");
        std::fs::create_dir_all(&dir).unwrap();
        generate_component(&dir, &ir);
        let out = std::fs::read_to_string(dir.join("c4-component.puml")).unwrap();

        assert!(out.contains(r#"Component(api, "api", "--", "Typed name (e.g. 'block')")"#));
        assert!(!out.contains(r#"(e.g. "block")"#));
    }

    #[test]
    fn container_diagram_groups_by_layer() {
        let ir = ir_with(vec![
            container("gpui", Some("UI")),
            container("tui", Some("UI")),
            container("mcp", Some("Services")),
        ]);
        let out = container_diagram(&ir);

        // One nested boundary per declared layer, inside the System boundary.
        assert!(out.contains("System_Boundary(sys, \"System\")"));
        assert!(out.contains("Container_Boundary(UI_layer, \"UI\") {"));
        assert!(out.contains("Container_Boundary(Services_layer, \"Services\") {"));
        // Containers land inside their layer boundary.
        assert!(out.contains("Container(gpui, \"Gpui\""));
        assert!(out.contains("Container(tui, \"Tui\""));
        assert!(out.contains("Container(mcp, \"Mcp\""));
    }

    #[test]
    fn container_diagram_stays_flat_without_layers() {
        let ir = ir_with(vec![container("gpui", None), container("tui", None)]);
        let out = container_diagram(&ir);

        // No layer declared → no nested boundary, flat list as before.
        assert!(!out.contains("Container_Boundary"));
        assert!(out.contains("System_Boundary(sys, \"System\") {"));
        assert!(out.contains("    Container(gpui, \"Gpui\""));
    }
}
