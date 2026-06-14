use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use archidoc_types::ir::{ArchitectureIR, DirNode};
use archidoc_types::C4Level;

/// Generate PlantUML C4 container diagram.
pub fn generate_container(output_dir: &Path, ir: &ArchitectureIR) {
    let filepath = output_dir.join("c4-container.puml");

    let all_annotated = ir.annotated_dirs();
    let containers: Vec<&DirNode> = all_annotated
        .iter()
        .copied()
        .filter(|d| d.c4_level == Some(C4Level::Container))
        .collect();

    let mut container_defs = String::new();
    for dir in &containers {
        let id = to_puml_id(&dir.path);
        let name = to_title_case(&dir.name);
        let pattern = dir.pattern.as_deref().unwrap_or("--");
        let desc = dir.description.as_deref().unwrap_or("");
        container_defs.push_str(&format!(
            "    Container({}, \"{}\", \"{}\", \"{}\")\n",
            id, name, pattern, desc
        ));
    }

    // `@c4 system` nodes are external systems the containers talk to — render
    // them outside the boundary so cross-level `Rel(...)` arrows resolve.
    let mut system_defs = String::new();
    for dir in systems_of(ir) {
        let id = to_puml_id(&dir.path);
        let name = to_title_case(&dir.name);
        let desc = dir.description.as_deref().unwrap_or("");
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
                from_id, to_id, rel.label, rel.protocol
            ));
        }
    }

    let content = format!(
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
    );

    fs::write(&filepath, content).expect("Failed to write c4-container.puml");
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
        let name = to_title_case(&dir.name);
        let desc = dir.description.as_deref().unwrap_or("");
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
                from_id, to_id, rel.label, rel.protocol
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

    let mut grouped: BTreeMap<String, Vec<&DirNode>> = BTreeMap::new();
    for dir in &components {
        let parent = dir
            .parent
            .clone()
            .unwrap_or_else(|| "other".to_string());
        grouped.entry(parent).or_default().push(dir);
    }

    let mut boundary_defs = String::new();
    for (parent, component_dirs) in &grouped {
        let parent_id = to_puml_id(parent);
        let parent_name = to_title_case(parent.split('/').last().unwrap_or(parent));
        boundary_defs.push_str(&format!(
            "Container_Boundary({}_boundary, \"{}\") {{\n",
            parent_id, parent_name
        ));
        for dir in component_dirs {
            let id = to_puml_id(&dir.path);
            let name = &dir.name;
            let pattern = dir.pattern.as_deref().unwrap_or("--");
            let desc = dir.description.as_deref().unwrap_or("");
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
                from_id, to_id, rel.label, rel.protocol
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
