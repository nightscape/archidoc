use std::fs;
use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand, ValueEnum};

// ===========================================================================
// CLI definition — noun-first convention
// ===========================================================================

#[derive(Parser)]
#[command(name = "archidoc")]
#[command(version)]
#[command(about = "\
Architecture documentation compiler — deterministic JSON metadata for directories.

Archidoc converts any file directory into a structured JSON representation
(ArchitectureIR) that LLMs and tools can consume instantly — no searching
the filesystem each time. It also creates directories deterministically
from JSON templates, so your architecture plans become executable.

What it does:
  1. UNDERSTAND  Scan any directory → JSON snapshot of structure + strategy + health
  2. CREATE      Instantiate directory structures from JSON scaffold templates
  3. ANNOTATE    Add strategy metadata (@c4 level, purpose, patterns) to directories
  4. VALIDATE    Catch architectural drift — diff design intent vs implementation

For greenfield projects:
  archidoc ir compile . --design           declare target architecture
  archidoc scaffold create project --var   create structure from template
  archidoc ir validate arch.json curr.json ensure implementation tracks design

For brownfield projects:
  archidoc ir compile .                    snapshot existing structure → JSON
  archidoc ir render ai-structure .        instant LLM-ready project summary
  archidoc annotate md . --recursive       add strategy metadata to all dirs

Commands (noun-first):
  ir compile [path]                        scan → current.json (or --design → architecture.json)
  ir render <format> <source>              render docs from IR or directory
  ir validate <architecture> <current>     diff target vs actual IR
  ir ls <path> [--depth N]                 list IR directory children (no rescan)
  ir describe <path>                       full details for one IR directory
  ir query [--annotated] [--empty]...      filter IR directories by state
  scaffold compile <template-dir>          folder template → ScaffoldIR JSON
  scaffold create <name> [--var k=v]       instantiate a scaffold template
  scaffold list                            list available templates
  scaffold inspect <name>                  show template metadata
  annotate <lang> [dir]                    add @c4 entry points (rs|ts|js|md)
  annotate --coverage [dir]                show annotation coverage report

Render formats (AI — for LLM consumption):
  ai-files       every file per dir, explicit       for grep/search/file discovery
  ai-structure   compressed tree + strategy + health for cold-start onboarding
  ai-strategy    module narrative + relationships    for architecture decisions

Render formats (Human — for review):
  human-strategy → ARCHITECTURE.md                  full doc with diagrams
  human-structure                                    structure overview (reserved)

Render formats (Diagrams):
  plantuml       → diagrams/c4.puml                 PlantUML C4 diagrams
  drawio         → diagrams/c4.drawio.csv           draw.io CSV import

Quick start:
  archidoc ir compile .                    scan → _context/current.json
  archidoc ir render ai-structure .        instant LLM context (one step)
  archidoc ir render human-strategy _context/current.json
")]
#[command(arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Work with Architecture IR — compile, render, validate, query
    ///
    /// The IR (Intermediate Representation) is a structured JSON snapshot
    /// of a directory's structure, strategy annotations, and health status.
    ///
    /// Subcommands:
    ///   compile   scan directory → IR JSON
    ///   render    IR or directory → documentation
    ///   validate  diff design intent vs implementation
    ///   ls        list directory children from compiled IR
    ///   describe  full details for one directory from compiled IR
    ///   query     filter directories by state from compiled IR
    ///
    /// Examples:
    ///   archidoc ir compile .
    ///   archidoc ir render ai-structure .
    ///   archidoc ir validate _context/architecture.json _context/current.json
    ///   archidoc ir ls . --depth 2
    ///   archidoc ir describe src/api
    ///   archidoc ir query --empty
    Ir(IrArgs),

    /// Work with scaffold templates — compile, create, list, inspect
    ///
    /// Scaffold templates are JSON definitions that create directory structures
    /// with {{variable}} substitution.
    ///
    /// Subcommands:
    ///   compile   folder template → ScaffoldIR JSON
    ///   create    instantiate a template into directories
    ///   list      show available templates
    ///   inspect   show template metadata and variables
    ///
    /// Examples:
    ///   archidoc scaffold compile .archidoc/scaffold-templates/firm/
    ///   archidoc scaffold create client --var client_name=Acme
    ///   archidoc scaffold list
    ///   archidoc scaffold inspect client
    Scaffold(ScaffoldArgs),

    /// Add @c4 annotation entry points to directories
    ///
    /// Usage: archidoc annotate <lang> [dir] [--recursive] [--dry-run] [--force]
    ///        archidoc annotate --coverage [dir|file.json] [--depth N]
    ///
    /// Creates the language-appropriate entry file (mod.rs, index.ts, _index.md)
    /// with a @c4 header and file table pre-populated from the directory contents.
    ///
    /// Languages: rs, ts, js, md
    ///
    /// Examples:
    ///   archidoc annotate rs ./src/auth/
    ///   archidoc annotate ts ./src/api/
    ///   archidoc annotate md . --recursive
    ///   archidoc annotate md . --recursive --depth 2
    ///   archidoc annotate rs ./src/ --dry-run
    ///   archidoc annotate rs ./src/ --force
    ///   archidoc annotate --coverage .
    ///   archidoc annotate --coverage _context/current.json
    ///   archidoc annotate --coverage . --depth 2
    Annotate(AnnotateArgs),
}

// ---------------------------------------------------------------------------
// IR args
// ---------------------------------------------------------------------------

#[derive(Args)]
struct IrArgs {
    #[command(subcommand)]
    command: IrCommand,
}

#[derive(Subcommand)]
enum IrCommand {
    /// Scan source annotations + directory tree → ArchitectureIR JSON
    ///
    /// Outputs current.json (implementation state) by default.
    /// With --design, outputs architecture.json (target truth, all health = planned).
    /// Merges into an existing IR file if one is present at the output path.
    ///
    /// Examples:
    ///   archidoc ir compile .
    ///   archidoc ir compile ./src --design
    ///   archidoc ir compile . --output-dir _context
    Compile {
        /// Directory to scan (defaults to current directory)
        path: Option<PathBuf>,

        /// Directory to write output into
        #[arg(long, default_value = "_context")]
        output_dir: PathBuf,

        /// Emit as architecture (target truth, all health = planned).
        /// Writes to architecture.json instead of current.json.
        #[arg(long)]
        design: bool,
    },

    /// Render documentation from IR or directory
    ///
    /// <source> accepts a .json IR file or a directory path.
    /// If a directory is given, it is scanned on the fly (no file written).
    ///
    /// Formats (AI — for LLM consumption):
    ///   ai-files      → ai-files.md       every file listed explicitly per dir
    ///   ai-structure  → ai-structure.md   directory tree + strategy + health
    ///   ai-strategy   → ai-strategy.md    module narrative + relationships
    ///
    /// Formats (Human — for developer/stakeholder review):
    ///   human-strategy   → ARCHITECTURE.md     full doc with Mermaid diagrams
    ///   human-structure  → human-structure.md   (reserved for future use)
    ///
    /// Formats (Diagrams):
    ///   plantuml  → diagrams/c4.puml         PlantUML C4 diagrams
    ///   drawio    → diagrams/c4.drawio.csv   draw.io CSV import
    ///
    /// Examples:
    ///   archidoc ir render ai-files ./src/
    ///   archidoc ir render ai-structure .
    ///   archidoc ir render ai-strategy _context/current.json
    ///   archidoc ir render human-strategy _context/current.json
    ///   archidoc ir render human-strategy _context/current.json --validate
    ///   archidoc ir render plantuml _context/current.json
    Render {
        /// Output format
        #[arg(value_enum)]
        format: RenderFormat,

        /// Source: path to a .json IR file, or a directory (auto-compiled)
        source: PathBuf,

        /// Directory to write rendered output into
        #[arg(long, default_value = "_context")]
        output_dir: PathBuf,

        /// Maximum directory depth (ai-files and ai-structure formats)
        #[arg(long)]
        depth: Option<usize>,

        /// Check for drift instead of writing (human-strategy format only).
        /// Exits 1 on errors.
        #[arg(long)]
        validate: bool,

        /// Treat warnings as errors when validating (requires --validate)
        #[arg(long, requires = "validate")]
        validate_strict: bool,
    },

    /// Validate architecture conformance (target vs actual)
    ///
    /// Compares two ArchitectureIR JSON files:
    ///   <architecture>  the declared target truth (what SHOULD exist)
    ///   <current>       the implementation state (what DOES exist on disk)
    ///
    /// Findings:
    ///   UNIMPLEMENTED  in architecture but not in current
    ///   UNDOCUMENTED   in current but not in architecture
    ///   DIVERGED       same path but conflicting attributes
    ///   REGRESSION     health moved backward
    ///
    /// Exit codes:
    ///   0  no errors (warnings/infos may appear)
    ///   1  errors found, or warnings found with --strict
    ///
    /// Examples:
    ///   archidoc ir validate _context/architecture.json _context/current.json
    ///   archidoc ir validate _context/architecture.json _context/current.json --strict
    ///   archidoc ir validate _context/architecture.json _context/current.json --log
    Validate {
        /// Path to the architecture IR (target truth — what SHOULD exist)
        architecture: PathBuf,

        /// Path to the current IR (implementation state — what DOES exist on disk)
        current: PathBuf,

        /// Treat warnings as errors (exit 1 on any warning)
        #[arg(long)]
        strict: bool,

        /// Show git log context for regressions and divergences
        #[arg(long)]
        log: bool,
    },

    /// Check declared `@c4 uses` relationships against real crate dependencies
    ///
    /// Reads the actual crate→crate dependency graph from `cargo metadata`
    /// (no extra tooling) and diffs it against the `@c4 uses` arrows declared
    /// in a compiled IR. Reports two kinds of drift:
    ///   missing  — a real dependency with no `@c4 uses` (add the arrow)
    ///   stale    — an `@c4 uses` with no real dependency (remove the arrow)
    ///
    /// Examples:
    ///   archidoc ir check-deps _context/current.json --manifest-dir crates
    ///   archidoc ir check-deps _context/current.json --manifest-dir . --strict
    CheckDeps {
        /// Path to the compiled IR JSON (the declared `@c4 uses` source)
        ir: PathBuf,

        /// Directory containing the Cargo workspace/crate to read deps from
        #[arg(long, default_value = ".")]
        manifest_dir: PathBuf,

        /// Dependency names to ignore (repeatable); `workspace-hack` is always ignored
        #[arg(long)]
        ignore: Vec<String>,

        /// Exit 1 if any drift is found (CI gate)
        #[arg(long)]
        strict: bool,
    },

    /// List directory children from compiled IR (no rescan)
    ///
    /// Reads from _context/current.json by default.
    ///
    /// Examples:
    ///   archidoc ir ls . --depth 2
    ///   archidoc ir ls src/api
    ///   archidoc ir ls . --ir-path _context/architecture.json
    Ls {
        /// Directory path to list (use "." for root)
        path: String,

        /// How many levels deep to show
        #[arg(long, default_value = "1")]
        depth: usize,

        /// Path to IR JSON file (default: _context/current.json)
        #[arg(long)]
        ir_path: Option<PathBuf>,
    },

    /// Show full details for one directory from compiled IR (no rescan)
    ///
    /// Examples:
    ///   archidoc ir describe src/api
    ///   archidoc ir describe . --ir-path _context/architecture.json
    Describe {
        /// Directory path to describe
        path: String,

        /// Path to IR JSON file (default: _context/current.json)
        #[arg(long)]
        ir_path: Option<PathBuf>,
    },

    /// Filter directories by state from compiled IR (no rescan)
    ///
    /// Multiple flags combine with AND logic. No flags = list all.
    ///
    /// Examples:
    ///   archidoc ir query --empty
    ///   archidoc ir query --populated --annotated
    ///   archidoc ir query --scaffold
    Query {
        /// Show only empty directories (no files, no annotated children)
        #[arg(long)]
        empty: bool,

        /// Show only directories that contain files
        #[arg(long)]
        populated: bool,

        /// Show only scaffold directories (files but no health/purpose)
        #[arg(long)]
        scaffold: bool,

        /// Show only annotated directories (have @c4 metadata)
        #[arg(long)]
        annotated: bool,

        /// Path to IR JSON file (default: _context/current.json)
        #[arg(long)]
        ir_path: Option<PathBuf>,
    },
}

// ---------------------------------------------------------------------------
// Scaffold args
// ---------------------------------------------------------------------------

#[derive(Args)]
struct ScaffoldArgs {
    #[command(subcommand)]
    command: ScaffoldCommand,
}

#[derive(Subcommand)]
enum ScaffoldCommand {
    /// Convert a folder template directory → ScaffoldIR JSON
    ///
    /// Reads .archidoc-template.toml for metadata if present.
    /// Auto-detects {{variable}} patterns in file contents.
    /// Output: <output-dir>/scaffolds/<name>.json
    ///
    /// Examples:
    ///   archidoc scaffold compile .archidoc/scaffold-templates/firm/
    ///   archidoc scaffold compile ./my-template/ --output-dir .archidoc
    Compile {
        /// Template directory to compile
        path: PathBuf,

        /// Directory to write output into
        #[arg(long, default_value = "_context")]
        output_dir: PathBuf,
    },

    /// Instantiate a scaffold template into directories
    ///
    /// Looks up a ScaffoldIR JSON template by name and creates the directory
    /// structure with {{variable}} substitution.
    ///
    /// Template lookup order:
    ///   1. Built-in templates (custom-scaffolds, custom-inits, custom-trees)
    ///   2. .archidoc/scaffolds/<name>.json (walk up from cwd, nearest wins)
    ///   3. Direct path if <name> ends in .json or contains / or \
    ///
    /// Examples:
    ///   archidoc scaffold create client --var client_name=Acme
    ///   archidoc scaffold create project --dry-run --var engagement_name=Test
    ///   archidoc scaffold create ./path/to/template.json --var name=foo
    ///   archidoc scaffold create client -t ./output --var client_name=Acme --force
    Create {
        /// Template name, or path to a .json ScaffoldIR file
        name: String,

        /// Target directory (defaults to current directory)
        #[arg(long, short = 't')]
        target: Option<PathBuf>,

        /// Variable assignment (repeatable): --var key=value
        #[arg(long = "var", value_parser = parse_key_value)]
        vars: Vec<(String, String)>,

        /// Show what would be created without writing anything
        #[arg(long)]
        dry_run: bool,

        /// Overwrite existing files
        #[arg(long)]
        force: bool,
    },

    /// List available scaffold templates
    ///
    /// Shows built-in templates and any .archidoc/scaffolds/*.json found
    /// by walking up from the current directory.
    ///
    /// Example:
    ///   archidoc scaffold list
    List,

    /// Show template metadata and required variables
    ///
    /// Examples:
    ///   archidoc scaffold inspect client
    ///   archidoc scaffold inspect ./path/to/template.json
    Inspect {
        /// Template name, or path to a .json ScaffoldIR file
        name: String,
    },
}

// ---------------------------------------------------------------------------
// Render format enum (shared)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum RenderFormat {
    /// Every file listed explicitly per directory — for LLM grep/search (ai-files.md)
    #[value(name = "ai-files")]
    AiFiles,
    /// Compressed tree + strategy annotations + health — for LLM cold-start (ai-structure.md)
    #[value(name = "ai-structure")]
    AiStructure,
    /// Module narrative + relationships — for LLM architecture decisions (ai-strategy.md)
    #[value(name = "ai-strategy")]
    AiStrategy,
    /// Full human-readable doc with Mermaid diagrams (ARCHITECTURE.md)
    #[value(name = "human-strategy")]
    HumanStrategy,
    /// Human-readable structure overview (human-structure.md) — reserved
    #[value(name = "human-structure")]
    HumanStructure,
    /// PlantUML C4 diagrams (diagrams/c4.puml)
    Plantuml,
    /// draw.io CSV import (diagrams/c4.drawio.csv)
    Drawio,
}

// ---------------------------------------------------------------------------
// Annotate args (unchanged)
// ---------------------------------------------------------------------------

#[derive(Args)]
struct AnnotateArgs {
    /// Language: rs, ts, js, md (not required with --coverage)
    lang: Option<String>,

    /// Target directory or .json IR file (defaults to current directory)
    path: Option<PathBuf>,

    /// Annotate all subdirectories within the target as well
    #[arg(long)]
    recursive: bool,

    /// Show what would be written without writing anything
    #[arg(long, conflicts_with = "coverage")]
    dry_run: bool,

    /// Overwrite existing annotations (prepend to rs/ts/js, overwrite for md)
    #[arg(long, conflicts_with = "coverage")]
    force: bool,

    /// Show annotation coverage report instead of annotating
    #[arg(long)]
    coverage: bool,

    /// Maximum directory depth for recursive operations and coverage
    #[arg(long)]
    depth: Option<usize>,
}

fn parse_key_value(s: &str) -> Result<(String, String), String> {
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid KEY=value: no '=' found in '{s}'"))?;
    Ok((s[..pos].to_string(), s[pos + 1..].to_string()))
}

// ===========================================================================
// Main dispatch
// ===========================================================================

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Ir(args) => run_ir(args),
        Commands::Scaffold(args) => run_scaffold(args),
        Commands::Annotate(args) => run_annotate(args),
    }
}

// ===========================================================================
// IR command
// ===========================================================================

fn run_ir(args: IrArgs) {
    match args.command {
        IrCommand::Compile { path, output_dir, design } => {
            run_ir_compile(path, output_dir, design);
        }
        IrCommand::Render { format, source, output_dir, depth, validate, validate_strict } => {
            run_ir_render(format, source, output_dir, depth, validate, validate_strict);
        }
        IrCommand::Validate { architecture, current, strict, log } => {
            run_ir_validate(architecture, current, strict, log);
        }
        IrCommand::CheckDeps { ir, manifest_dir, ignore, strict } => {
            run_ir_check_deps(ir, manifest_dir, ignore, strict);
        }
        IrCommand::Ls { path, depth, ir_path } => {
            let ir = load_ir_default(ir_path);
            match archidoc_engine::ir_query::ls(&ir, &path, depth) {
                Ok(output) => print!("{}", output),
                Err(e) => {
                    eprintln!("error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        IrCommand::Describe { path, ir_path } => {
            let ir = load_ir_default(ir_path);
            match archidoc_engine::ir_query::describe(&ir, &path) {
                Ok(output) => print!("{}", output),
                Err(e) => {
                    eprintln!("error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        IrCommand::Query { empty, populated, scaffold, annotated, ir_path } => {
            let ir = load_ir_default(ir_path);
            let filter = archidoc_engine::ir_query::QueryFilter {
                empty,
                populated,
                scaffold,
                annotated,
            };
            print!("{}", archidoc_engine::ir_query::query(&ir, &filter));
        }
    }
}

fn run_ir_compile(path: Option<PathBuf>, output_dir: PathBuf, design: bool) {
    let base = cwd();
    let scan_root = path
        .map(|p| resolve_path(&base, &p))
        .unwrap_or(base.clone());

    if !scan_root.exists() {
        eprintln!("error: path does not exist: {}", scan_root.display());
        std::process::exit(1);
    }

    let output_dir = resolve_path(&base, &output_dir);

    let modules = archidoc_rust::walker::extract_all_docs(&scan_root);
    let config = archidoc_engine::tree::TreeConfig::load(&scan_root);
    let incoming = archidoc_engine::ir_builder::build_from_scan(&scan_root, modules, &config);

    let ir_filename = if design { "architecture.json" } else { "current.json" };
    let ir_path = output_dir.join(ir_filename);
    let merged = if ir_path.exists() {
        match archidoc_types::ir::ArchitectureIR::load(&ir_path) {
            Ok(existing) => match archidoc_engine::merge::merge_ir(existing, incoming) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("error: {}", e);
                    std::process::exit(1);
                }
            },
            Err(e) => {
                eprintln!("warning: could not read existing IR ({}), overwriting", e);
                incoming
            }
        }
    } else {
        incoming
    };

    let final_ir = if design {
        stamp_design(merged)
    } else {
        merged
    };

    if let Err(e) = final_ir.save(&ir_path) {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }

    let annotated_count = final_ir.annotated_dirs().len();
    let label = if design { "architecture" } else { "current" };
    println!(
        "{} module{}, tree captured → {} ({})",
        annotated_count,
        if annotated_count == 1 { "" } else { "s" },
        ir_path.display(),
        label,
    );
}

// ---------------------------------------------------------------------------
// IR render
// ---------------------------------------------------------------------------

/// Render configuration passed to every renderer function.
struct RenderConfig<'a> {
    ir: &'a archidoc_types::ir::ArchitectureIR,
    output_dir: &'a Path,
    depth: Option<usize>,
    validate: bool,
    validate_strict: bool,
}

/// Renderer function signature — every renderer has this shape.
type RendererFn = fn(&RenderConfig);

/// Declarative dispatch table: format → renderer function.
///
/// Adding a new output format = adding one line here + one function.
const RENDERERS: &[(RenderFormat, RendererFn)] = &[
    (RenderFormat::AiFiles,        render_ai_files),
    (RenderFormat::AiStructure,    render_ai_structure),
    (RenderFormat::AiStrategy,     render_ai_strategy),
    (RenderFormat::HumanStrategy,  render_human_strategy),
    (RenderFormat::HumanStructure, render_human_structure),
    (RenderFormat::Plantuml,       render_plantuml),
    (RenderFormat::Drawio,         render_drawio),
];

fn run_ir_render(
    format: RenderFormat,
    source: PathBuf,
    output_dir: PathBuf,
    depth: Option<usize>,
    validate: bool,
    validate_strict: bool,
) {
    let base = cwd();
    let source = resolve_path(&base, &source);
    let output_dir = resolve_path(&base, &output_dir);
    let ir = resolve_source(&source);

    fs::create_dir_all(&output_dir).unwrap_or_else(|e| {
        eprintln!("error: failed to create output directory: {}", e);
        std::process::exit(1);
    });

    let config = RenderConfig {
        ir: &ir,
        output_dir: &output_dir,
        depth,
        validate,
        validate_strict,
    };

    let renderer = RENDERERS
        .iter()
        .find(|(fmt, _)| *fmt == format)
        .map(|(_, f)| f)
        .expect("all RenderFormat variants must be in RENDERERS table");

    renderer(&config);
}

/// Load an ArchitectureIR from a JSON file or by scanning a directory.
///
/// If `source` is a directory: scan in-memory (no temp file written).
/// If `source` is a file: load as JSON IR.
fn resolve_source(source: &Path) -> archidoc_types::ir::ArchitectureIR {
    if !source.exists() {
        eprintln!("error: source does not exist: {}", source.display());
        std::process::exit(1);
    }

    if source.is_dir() {
        let modules = archidoc_rust::walker::extract_all_docs(source);
        let config = archidoc_engine::tree::TreeConfig::load(source);
        archidoc_engine::ir_builder::build_from_scan(source, modules, &config)
    } else {
        load_ir(source)
    }
}

// ---------------------------------------------------------------------------
// Renderer implementations — all share the same signature: fn(&RenderConfig)
// ---------------------------------------------------------------------------

fn render_ai_files(cfg: &RenderConfig) {
    let content = archidoc_engine::filemap::generate(&cfg.ir.root, cfg.depth);
    write_file(&cfg.output_dir.join("ai-files.md"), &content);
}

fn render_ai_structure(cfg: &RenderConfig) {
    let content = archidoc_engine::context::generate(cfg.ir, cfg.depth);
    write_file(&cfg.output_dir.join("ai-structure.md"), &content);
}

fn render_ai_strategy(cfg: &RenderConfig) {
    let content = archidoc_engine::ai_context::generate(cfg.ir);
    write_file(&cfg.output_dir.join("ai-strategy.md"), &content);
}

fn render_human_strategy(cfg: &RenderConfig) {
    if cfg.validate {
        let arch_file = cfg.output_dir.join("ARCHITECTURE.md");
        let report = archidoc_engine::diagnostics::run(cfg.ir, &arch_file, cfg.output_dir);
        print!("{}", report.format());
        if report.should_fail(cfg.validate_strict) {
            std::process::exit(1);
        }
        return;
    }

    let content = archidoc_engine::architecture::generate(cfg.ir, &[]);
    write_file(&cfg.output_dir.join("ARCHITECTURE.md"), &content);
}

fn render_human_structure(cfg: &RenderConfig) {
    let content = archidoc_engine::context::generate(cfg.ir, cfg.depth);
    write_file(&cfg.output_dir.join("human-structure.md"), &content);
}

fn render_plantuml(cfg: &RenderConfig) {
    let diagrams_dir = cfg.output_dir.join("diagrams");
    fs::create_dir_all(&diagrams_dir).unwrap_or_else(|e| {
        eprintln!("error: failed to create output directory: {}", e);
        std::process::exit(1);
    });

    archidoc_engine::plantuml::generate_container(&diagrams_dir, cfg.ir);
    archidoc_engine::plantuml::generate_component(&diagrams_dir, cfg.ir);
    println!("wrote PlantUML files to {}", diagrams_dir.display());
}

fn render_drawio(cfg: &RenderConfig) {
    let diagrams_dir = cfg.output_dir.join("diagrams");
    fs::create_dir_all(&diagrams_dir).unwrap_or_else(|e| {
        eprintln!("error: failed to create output directory: {}", e);
        std::process::exit(1);
    });

    archidoc_engine::drawio::generate_container_csv(&diagrams_dir, cfg.ir);
    archidoc_engine::drawio::generate_component_csv(&diagrams_dir, cfg.ir);
    println!("wrote draw.io CSV files to {}", diagrams_dir.display());
}

// ---------------------------------------------------------------------------
// IR validate
// ---------------------------------------------------------------------------

fn run_ir_validate(architecture: PathBuf, current: PathBuf, strict: bool, log: bool) {
    let base = cwd();
    let arch_path = resolve_path(&base, &architecture);
    let current_path = resolve_path(&base, &current);

    let architecture = load_ir(&arch_path);
    let current = load_ir(&current_path);

    let report = archidoc_engine::design_validate::validate(&architecture, &current);

    print!("{}", report.format());

    if log && !report.is_clean() {
        print_git_context(&current_path);
    }

    if report.should_fail(strict) {
        std::process::exit(1);
    }
}

fn run_ir_check_deps(ir: PathBuf, manifest_dir: PathBuf, ignore: Vec<String>, strict: bool) {
    use archidoc_rust::cargo_metadata::{
        validate_ir_relationships, workspace_import_graph, DEFAULT_IGNORE,
    };
    use archidoc_rust::cargo_modules::WarningKind;
    use std::collections::HashSet;

    let base = cwd();
    let ir_path = resolve_path(&base, &ir);
    let manifest_dir = resolve_path(&base, &manifest_dir);
    let ir = load_ir(&ir_path);

    let ignore: HashSet<String> = DEFAULT_IGNORE
        .iter()
        .map(|s| s.to_string())
        .chain(ignore)
        .collect();

    let graph = match workspace_import_graph(&manifest_dir, &ignore) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    };

    let warnings = validate_ir_relationships(&ir, &graph, &ignore);

    let missing: Vec<_> = warnings
        .iter()
        .filter(|w| matches!(w.kind, WarningKind::Undeclared))
        .collect();
    let stale: Vec<_> = warnings
        .iter()
        .filter(|w| matches!(w.kind, WarningKind::NoImport))
        .collect();

    if warnings.is_empty() {
        println!("Relationship check passed — every `@c4 uses` matches a real crate dependency.");
        return;
    }

    if !missing.is_empty() {
        println!(
            "Missing `@c4 uses` ({} — real dependency, no declared arrow):",
            missing.len()
        );
        for w in &missing {
            println!(
                "  {} → {}\n      add to {}: //! @c4 uses {} \"<purpose>\" \"Rust\"",
                w.module, w.target, w.module, w.target
            );
        }
    }
    if !stale.is_empty() {
        println!(
            "\nStale `@c4 uses` ({} — declared arrow, no real dependency):",
            stale.len()
        );
        for w in &stale {
            println!("  {} → {}  (remove or fix the `@c4 uses` line)", w.module, w.target);
        }
    }

    if strict {
        std::process::exit(1);
    }
}

fn print_git_context(actual_path: &Path) {
    use std::process::Command;

    let output = Command::new("git")
        .args(["log", "--oneline", "-10", "--"])
        .arg(actual_path)
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let log = String::from_utf8_lossy(&out.stdout);
            if !log.trim().is_empty() {
                println!("Recent changes to {}:", actual_path.display());
                println!("{}", log);
            }
        }
        _ => {}
    }
}

// ===========================================================================
// Scaffold command
// ===========================================================================

fn run_scaffold(args: ScaffoldArgs) {
    match args.command {
        ScaffoldCommand::Compile { path, output_dir } => {
            run_scaffold_compile(path, output_dir);
        }
        ScaffoldCommand::Create { name, target, vars, dry_run, force } => {
            run_scaffold_create(&name, target, &vars, dry_run, force);
        }
        ScaffoldCommand::List => {
            run_scaffold_list();
        }
        ScaffoldCommand::Inspect { name } => {
            run_scaffold_inspect(&name);
        }
    }
}

fn run_scaffold_compile(path: PathBuf, output_dir: PathBuf) {
    let base = cwd();
    let source_dir = resolve_path(&base, &path);

    if !source_dir.is_dir() {
        eprintln!("error: not a directory: {}", source_dir.display());
        std::process::exit(1);
    }

    let ir = match archidoc_engine::scaffold_ir::compile::compile(&source_dir) {
        Ok(ir) => ir,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    let name = ir
        .template
        .as_ref()
        .map(|t| t.name.clone())
        .unwrap_or_else(|| {
            source_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("template")
                .to_string()
        });

    let output_dir = resolve_path(&base, &output_dir);
    let scaffolds_dir = output_dir.join("scaffolds");
    fs::create_dir_all(&scaffolds_dir).unwrap_or_else(|e| {
        eprintln!("error: failed to create output directory: {}", e);
        std::process::exit(1);
    });

    let out_path = scaffolds_dir.join(format!("{}.json", name));
    let json = serde_json::to_string_pretty(&ir).unwrap_or_else(|e| {
        eprintln!("error: failed to serialize: {}", e);
        std::process::exit(1);
    });

    fs::write(&out_path, &json).unwrap_or_else(|e| {
        eprintln!("error: failed to write {}: {}", out_path.display(), e);
        std::process::exit(1);
    });

    let node_count = ir.nodes.len();
    let var_count = ir.template.as_ref().map(|t| t.variables.len()).unwrap_or(0);
    println!(
        "{} nodes, {} variables → {}",
        node_count,
        var_count,
        out_path.display()
    );
}

fn run_scaffold_list() {
    use archidoc_engine::scaffold_ir;

    let base = cwd();
    let disk = scaffold_ir::list(&base);
    let disk_names: Vec<&str> = disk.iter().map(|(n, _, _)| n.as_str()).collect();

    let has_any = !disk.is_empty() || !scaffold_ir::BUILTIN_NAMES.is_empty();
    if !has_any {
        println!("no scaffold templates found.");
        return;
    }

    println!("available scaffold templates:");
    println!();

    for &bname in scaffold_ir::BUILTIN_NAMES {
        if disk_names.contains(&bname) {
            continue;
        }
        if let Some(ir) = scaffold_ir::load_builtin(bname) {
            let desc = ir
                .template
                .as_ref()
                .map(|t| t.description.as_str())
                .unwrap_or("");
            println!("  {}  (built-in)", bname);
            println!("    {}", desc);
            println!();
        }
    }

    for (name, path, ir) in &disk {
        let rel = path.strip_prefix(&base).unwrap_or(path);
        let desc = ir
            .template
            .as_ref()
            .map(|t| t.description.as_str())
            .unwrap_or("");
        let vars = ir
            .template
            .as_ref()
            .map(|t| t.variables.as_slice())
            .unwrap_or(&[]);
        println!("  {}  (from {})", name, rel.display());
        println!("    {}", desc);
        if !vars.is_empty() {
            let var_names: Vec<&str> = vars.iter().map(|v| v.name.as_str()).collect();
            println!("    vars: {}", var_names.join(", "));
        }
        println!();
    }
}

fn run_scaffold_inspect(name: &str) {
    use archidoc_engine::scaffold_ir;

    let base = cwd();

    let (ir, _template_dir) = resolve_scaffold_template(name, &base);

    let ir = match scaffold_ir::resolve(ir, &base) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    let tmpl = ir.template.as_ref();
    let tname = tmpl.map(|t| t.name.as_str()).unwrap_or(name);
    let desc = tmpl.map(|t| t.description.as_str()).unwrap_or("");
    println!("template: {}", tname);
    println!("description: {}", desc);
    println!();
    let vars = tmpl.map(|t| t.variables.as_slice()).unwrap_or(&[]);
    if vars.is_empty() {
        println!("no variables required.");
    } else {
        println!("variables:");
        for var in vars {
            let req = if var.required { "required" } else { "optional" };
            let def = var
                .default
                .as_deref()
                .map(|d| format!(" (default: {})", d))
                .unwrap_or_default();
            println!("  --var {}=<value>  [{}{}]", var.name, req, def);
            println!("    {}", var.description);
        }
    }
    let hooks = tmpl.map(|t| t.post_hooks.as_slice()).unwrap_or(&[]);
    if !hooks.is_empty() {
        println!();
        println!("post-hooks:");
        for hook in hooks {
            println!("  {} — {}", hook.command, hook.description);
        }
    }
}

fn run_scaffold_create(
    name: &str,
    target: Option<PathBuf>,
    vars: &[(String, String)],
    dry_run: bool,
    force: bool,
) {
    use archidoc_engine::scaffold_ir;
    use archidoc_engine::scaffold_ir::{ActionOutcome, ExecuteError};

    let base = cwd();

    let (ir, template_dir) = resolve_scaffold_template(name, &base);

    let ir = match scaffold_ir::resolve(ir, &template_dir) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    let tmpl = ir.template.as_ref();

    let empty_tmpl;
    let tmpl_for_collect = match tmpl {
        Some(t) => t,
        None => {
            empty_tmpl = archidoc_types::scaffold_ir::ScaffoldTemplate {
                name: name.to_string(),
                description: String::new(),
                variables: vec![],
                post_hooks: vec![],
            };
            &empty_tmpl
        }
    };

    let variables = match scaffold_ir::collect(tmpl_for_collect, vars) {
        Ok(v) => v,
        Err(archidoc_engine::scaffold_ir::VariableError::Missing(missing)) => {
            eprintln!(
                "error: missing required variable(s): {}",
                missing.join(", ")
            );
            eprintln!();
            eprintln!("provide them with --var flags:");
            for vname in &missing {
                let desc = tmpl_for_collect
                    .variables
                    .iter()
                    .find(|v| &v.name == vname)
                    .map(|v| v.description.as_str())
                    .unwrap_or("");
                eprintln!("  --var {}=<value>    {}", vname, desc);
            }
            std::process::exit(1);
        }
    };

    let target = target.unwrap_or_else(|| base.clone());
    let target = resolve_path(&base, &target);

    if dry_run {
        let actions = match scaffold_ir::dry_run(&ir, &target, &variables) {
            Ok(a) => a,
            Err(ExecuteError::InvalidNode { detail }) => {
                eprintln!("error: {}", detail);
                std::process::exit(1);
            }
            Err(ExecuteError::Io(e)) => {
                eprintln!("error: {}", e);
                std::process::exit(1);
            }
        };

        println!("dry run — would create:");
        println!();
        for action in &actions {
            let rel = action.path.strip_prefix(&target).unwrap_or(&action.path);
            if action.kind == "dir" {
                println!("  dir   {}/", rel.display());
            } else {
                println!("  file  {}", rel.display());
            }
        }

        let hooks = tmpl.map(|t| t.post_hooks.as_slice()).unwrap_or(&[]);
        if !hooks.is_empty() {
            println!();
            println!("post-hooks:");
            for hook in hooks {
                println!("  {}", hook.command);
            }
        }
        return;
    }

    let result = match scaffold_ir::execute(&ir, &target, &variables, force) {
        Ok(r) => r,
        Err(ExecuteError::InvalidNode { detail }) => {
            eprintln!("error: {}", detail);
            std::process::exit(1);
        }
        Err(ExecuteError::Io(e)) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    let mut created = 0usize;
    let mut skipped = 0usize;
    for outcome in &result.outcomes {
        match outcome {
            ActionOutcome::Created(path) => {
                let rel = path.strip_prefix(&target).unwrap_or(path);
                println!("created  {}", rel.display());
                created += 1;
            }
            ActionOutcome::Skipped { path, .. } => {
                let rel = path.strip_prefix(&target).unwrap_or(path);
                println!("skipped  {}", rel.display());
                skipped += 1;
            }
        }
    }
    println!();
    println!("created: {}  skipped: {}", created, skipped);

    for hr in &result.hook_results {
        if hr.success {
            println!("hook ok: {}", hr.command);
        } else {
            eprintln!("hook failed: {} — {}", hr.command, hr.output.trim());
        }
    }
}

/// Resolve a scaffold template by name — shared by `create` and `inspect`.
fn resolve_scaffold_template(
    name: &str,
    base: &Path,
) -> (archidoc_types::scaffold_ir::ScaffoldIR, PathBuf) {
    use archidoc_engine::scaffold_ir;

    if name.ends_with(".json") || name.contains('/') || name.contains('\\') {
        let path = resolve_path(base, Path::new(name));
        let dir = path.parent().unwrap_or(base).to_path_buf();
        let ir = archidoc_types::scaffold_ir::ScaffoldIR::load(&path).unwrap_or_else(|e| {
            eprintln!("error: {}", e);
            std::process::exit(1);
        });
        (ir, dir)
    } else if scaffold_ir::is_builtin(name) {
        let ir = scaffold_ir::load_builtin(name).unwrap();
        (ir, base.to_path_buf())
    } else {
        match scaffold_ir::find(name, base) {
            Some(path) => {
                let dir = path.parent().unwrap_or(base).to_path_buf();
                let ir =
                    archidoc_types::scaffold_ir::ScaffoldIR::load(&path).unwrap_or_else(|e| {
                        eprintln!("error: {}", e);
                        std::process::exit(1);
                    });
                (ir, dir)
            }
            None => {
                eprintln!("error: template '{}' not found", name);
                eprintln!();
                eprintln!("run `archidoc scaffold list` to see available templates.");
                std::process::exit(1);
            }
        }
    }
}

// ===========================================================================
// Annotate
// ===========================================================================

fn run_annotate(args: AnnotateArgs) {
    if args.coverage {
        run_annotate_coverage(&args);
        return;
    }

    use archidoc_engine::annotate::{self, Lang, Outcome, SkipReason};

    let lang_str = match &args.lang {
        Some(l) => l.clone(),
        None => {
            eprintln!("error: <lang> is required (rs, ts, js, md). Use --coverage for reports.");
            std::process::exit(1);
        }
    };

    let lang = match Lang::from_str(&lang_str) {
        Some(l) => l,
        None => {
            eprintln!(
                "error: unknown language '{}' (supported: rs, ts, js, md)",
                lang_str
            );
            std::process::exit(1);
        }
    };

    let base = cwd();
    let target = args.path.unwrap_or_else(|| base.clone());
    let target = resolve_path(&base, &target);

    if args.dry_run {
        let items = annotate::dry_run(&target, lang, args.recursive, args.depth);
        if items.is_empty() {
            println!("nothing to annotate.");
            return;
        }
        println!(
            "dry run ({} director{}):",
            items.len(),
            if items.len() == 1 { "y" } else { "ies" }
        );
        println!();
        for item in &items {
            let rel = item.path.strip_prefix(&target).unwrap_or(&item.path);
            println!("  {:30}  {}", rel.display(), item.action);
        }
        return;
    }

    let outcomes = if args.recursive {
        annotate::annotate_recursive(&target, lang, args.force, args.depth)
    } else {
        vec![annotate::annotate_dir(&target, lang, args.force)]
    };

    let mut created = 0usize;
    let mut prepended = 0usize;
    let mut skipped = 0usize;
    let mut errors = 0usize;

    for outcome in &outcomes {
        match outcome {
            Outcome::Created(path) => {
                let rel = path.strip_prefix(&target).unwrap_or(path);
                println!("created   {}", rel.display());
                created += 1;
            }
            Outcome::Prepended(path) => {
                let rel = path.strip_prefix(&target).unwrap_or(path);
                println!("prepended {}", rel.display());
                prepended += 1;
            }
            Outcome::Skipped { path, reason } => {
                let rel = path.strip_prefix(&target).unwrap_or(path);
                match reason {
                    SkipReason::AlreadyAnnotated => {
                        println!("skipped   {}  (already annotated)", rel.display());
                    }
                    SkipReason::FileExistsNoForce => {
                        println!(
                            "skipped   {}  (file exists — use --force to prepend)",
                            rel.display()
                        );
                    }
                }
                skipped += 1;
            }
            Outcome::Error { path, error } => {
                let rel = path.strip_prefix(&target).unwrap_or(path);
                eprintln!("error     {}  ({})", rel.display(), error);
                errors += 1;
            }
        }
    }

    println!();
    println!(
        "created: {}  prepended: {}  skipped: {}  errors: {}",
        created, prepended, skipped, errors
    );

    if errors > 0 {
        std::process::exit(1);
    }
}

fn run_annotate_coverage(args: &AnnotateArgs) {
    let base = cwd();
    let source = args.path.clone().unwrap_or_else(|| base.clone());
    let source = resolve_path(&base, &source);
    let ir = resolve_source(&source);
    let report = archidoc_engine::coverage::compute_coverage(&ir, args.depth);
    print!("{}", archidoc_engine::coverage::format_coverage_report(&report));
}

// ===========================================================================
// Helpers
// ===========================================================================

fn cwd() -> PathBuf {
    std::env::current_dir().expect("failed to get current directory")
}

fn resolve_path(base: &Path, p: &Path) -> PathBuf {
    if p.is_absolute() { p.to_path_buf() } else { base.join(p) }
}

fn load_ir(path: &Path) -> archidoc_types::ir::ArchitectureIR {
    archidoc_types::ir::ArchitectureIR::load(path).unwrap_or_else(|e| {
        eprintln!("error: {}", e);
        std::process::exit(1);
    })
}

/// Load IR from --ir-path or default _context/current.json.
fn load_ir_default(ir_path: Option<PathBuf>) -> archidoc_types::ir::ArchitectureIR {
    let base = cwd();
    let ir_path = ir_path.unwrap_or_else(|| base.join("_context/current.json"));
    let ir_path = resolve_path(&base, &ir_path);

    if !ir_path.exists() {
        eprintln!(
            "error: IR file not found: {}\nRun `archidoc ir compile .` first.",
            ir_path.display()
        );
        std::process::exit(1);
    }

    load_ir(&ir_path)
}

fn stamp_design(mut ir: archidoc_types::ir::ArchitectureIR) -> archidoc_types::ir::ArchitectureIR {
    stamp_dir_planned(&mut ir.root);
    ir
}

fn stamp_dir_planned(node: &mut archidoc_types::ir::DirNode) {
    for file in &mut node.files {
        if file.health.is_some() {
            file.health = Some(archidoc_types::annotation::HealthStatus::Planned);
        }
    }
    for child in &mut node.dirs {
        stamp_dir_planned(child);
    }
}

fn write_file(path: &Path, content: &str) {
    fs::write(path, content).unwrap_or_else(|e| {
        eprintln!("error: failed to write {}: {}", path.display(), e);
        std::process::exit(1);
    });
    println!("wrote {}", path.display());
}
