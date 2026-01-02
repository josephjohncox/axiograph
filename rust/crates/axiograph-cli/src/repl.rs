//! A small interactive shell for PathDB and `.axi` snapshots.
//!
//! By default we use `rustyline` for line editing and tab completion.
//! A minimal stdin-based fallback exists behind `--no-default-features`.

use anyhow::{anyhow, Result};
use colored::Colorize;
#[cfg(feature = "repl-rustyline")]
use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::io::Read;
#[cfg(not(feature = "repl-rustyline"))]
use std::io::Write;
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use roaring::RoaringBitmap;

pub fn cmd_repl(initial_axpd: Option<&PathBuf>) -> Result<()> {
    #[cfg(feature = "repl-rustyline")]
    {
        return cmd_repl_rustyline(initial_axpd);
    }
    #[cfg(not(feature = "repl-rustyline"))]
    {
        return cmd_repl_simple(initial_axpd);
    }
}

pub fn cmd_repl_script(
    initial_axpd: Option<&PathBuf>,
    script: Option<&PathBuf>,
    commands: &[String],
    continue_on_error: bool,
    quiet: bool,
) -> Result<()> {
    let mut state = ReplState::default();

    if let Some(path) = initial_axpd {
        cmd_load_axpd(&mut state, path)?;
    }

    let mut lines: Vec<String> = Vec::new();

    if let Some(script_path) = script {
        let text = if script_path.as_os_str() == "-" {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf)?;
            buf
        } else {
            fs::read_to_string(script_path)?
        };
        for line in text.lines() {
            lines.push(line.to_string());
        }
    }

    for cmd in commands {
        lines.push(cmd.clone());
    }

    for (idx, raw_line) in lines.iter().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('#') || line.starts_with("//") {
            continue;
        }

        if !quiet {
            println!("axiograph> {line}");
        }

        let tokens = tokenize_repl_line(line);
        match dispatch_repl_line_result(&mut state, &tokens) {
            Ok(ReplControl::Continue) => {}
            Ok(ReplControl::Exit) => break,
            Err(e) => {
                if continue_on_error {
                    eprintln!("{} {e}", "error:".red().bold());
                } else {
                    return Err(anyhow!("repl script failed at line {}: {e}", idx + 1));
                }
            }
        }
    }

    Ok(())
}

#[cfg(not(feature = "repl-rustyline"))]
fn cmd_repl_simple(initial_axpd: Option<&PathBuf>) -> Result<()> {
    let mut state = ReplState::default();

    println!("{}", "Axiograph REPL".green().bold());
    println!("Type `help` for commands. Type `exit` to quit.\n");

    if let Some(path) = initial_axpd {
        if let Err(e) = cmd_load_axpd(&mut state, path) {
            eprintln!(
                "{} failed to load {}: {e}",
                "error:".red().bold(),
                path.display()
            );
        }
    }

    let stdin = io::stdin();
    loop {
        print!("{}", "axiograph> ".cyan().bold());
        io::stdout().flush()?;

        let mut line = String::new();
        if stdin.read_line(&mut line)? == 0 {
            break;
        }

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let tokens = tokenize_repl_line(line);
        match dispatch_repl_line_result(&mut state, &tokens) {
            Ok(ReplControl::Continue) => {}
            Ok(ReplControl::Exit) => break,
            Err(e) => eprintln!("{} {e}", "error:".red().bold()),
        }
    }

    Ok(())
}

#[cfg(feature = "repl-rustyline")]
fn cmd_repl_rustyline(initial_axpd: Option<&PathBuf>) -> Result<()> {
    use rustyline::error::ReadlineError;
    use rustyline::Editor;

    let mut state = ReplState::default();

    println!("{}", "Axiograph REPL".green().bold());
    println!("Tab-completion enabled. Type `help` for commands. Type `exit` to quit.\n");

    if let Some(path) = initial_axpd {
        if let Err(e) = cmd_load_axpd(&mut state, path) {
            eprintln!(
                "{} failed to load {}: {e}",
                "error:".red().bold(),
                path.display()
            );
        }
    }

    let completions = std::sync::Arc::new(std::sync::RwLock::new(CompletionData::default()));
    refresh_completion_data(&completions, &state);

    let helper = ReplLineHelper::new(completions.clone());
    let mut rl: Editor<ReplLineHelper, rustyline::history::DefaultHistory> =
        Editor::new().map_err(|e| anyhow!("failed to init rustyline: {e}"))?;
    rl.set_helper(Some(helper));

    loop {
        // Keep completions fresh when the DB changes (load/gen/import).
        refresh_completion_data(&completions, &state);

        let line = match rl.readline("axiograph> ") {
            Ok(l) => l,
            Err(ReadlineError::Eof) => break,
            Err(ReadlineError::Interrupted) => continue,
            Err(e) => return Err(anyhow!("readline error: {e}")),
        };

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        rl.add_history_entry(line)
            .map_err(|e| anyhow!("failed to record history: {e}"))?;

        let tokens = split_command_line(line);
        match dispatch_repl_line_result(&mut state, &tokens) {
            Ok(ReplControl::Continue) => {}
            Ok(ReplControl::Exit) => break,
            Err(e) => eprintln!("{} {e}", "error:".red().bold()),
        }
    }

    Ok(())
}

enum ReplControl {
    Continue,
    Exit,
}

fn dispatch_repl_line_result(state: &mut ReplState, tokens: &[String]) -> Result<ReplControl> {
    if tokens.is_empty() {
        return Ok(ReplControl::Continue);
    }

    let cmd = tokens[0].as_str();
    let args = &tokens[1..];

    match cmd {
        "help" | "?" => {
            print_help();
            Ok(ReplControl::Continue)
        }
        "exit" | "quit" => Ok(ReplControl::Exit),
        "stats" => {
            cmd_stats(state);
            Ok(ReplControl::Continue)
        }
        "analyze" => {
            cmd_analyze(state, args)?;
            Ok(ReplControl::Continue)
        }
        "quality" => {
            cmd_quality(state, args)?;
            Ok(ReplControl::Continue)
        }
        "load" => {
            let p = one_path_arg("load", args)?;
            cmd_load_axpd(state, &p)?;
            Ok(ReplControl::Continue)
        }
        "save" => {
            let p = one_path_arg("save", args)?;
            cmd_save_axpd(state, &p)?;
            Ok(ReplControl::Continue)
        }
        "import_axi" => {
            let p = one_path_arg("import_axi", args)?;
            cmd_import_axi(state, &p)?;
            Ok(ReplControl::Continue)
        }
        "import_proto" => {
            cmd_import_proto(state, args)?;
            Ok(ReplControl::Continue)
        }
        "export_axi" => {
            let p = one_path_arg("export_axi", args)?;
            cmd_export_axi(state, &p)?;
            Ok(ReplControl::Continue)
        }
        "export_axi_module" => {
            cmd_export_axi_module(state, args)?;
            Ok(ReplControl::Continue)
        }
        "build_indexes" => {
            cmd_build_indexes(state)?;
            Ok(ReplControl::Continue)
        }
        "add_entity" => {
            cmd_add_entity(state, args)?;
            Ok(ReplControl::Continue)
        }
        "add_fact" => {
            cmd_add_fact(state, args)?;
            Ok(ReplControl::Continue)
        }
        "add_edge" | "add_relation" => {
            cmd_add_edge(state, args)?;
            Ok(ReplControl::Continue)
        }
        "add_equiv" | "add_equivalence" => {
            cmd_add_equiv(state, args)?;
            Ok(ReplControl::Continue)
        }
        "ctx" | "context" => {
            cmd_ctx(state, args)?;
            Ok(ReplControl::Continue)
        }
        "schema" => {
            cmd_schema(state, args)?;
            Ok(ReplControl::Continue)
        }
        "constraints" | "schema_constraints" => {
            cmd_schema_constraints(state, args)?;
            Ok(ReplControl::Continue)
        }
        "rules" => {
            cmd_rules(state, args)?;
            Ok(ReplControl::Continue)
        }
        "validate_axi" => {
            cmd_validate_axi(state, args)?;
            Ok(ReplControl::Continue)
        }
        "learning_graph" => {
            cmd_learning_graph(state, args)?;
            Ok(ReplControl::Continue)
        }
        "show" => {
            cmd_show(state, args)?;
            Ok(ReplControl::Continue)
        }
        "describe" => {
            cmd_describe(state, args)?;
            Ok(ReplControl::Continue)
        }
        "neigh" | "neighborhood" => {
            cmd_neigh(state, args)?;
            Ok(ReplControl::Continue)
        }
        "open" => {
            cmd_open(state, args)?;
            Ok(ReplControl::Continue)
        }
        "diff" => {
            cmd_diff(state, args)?;
            Ok(ReplControl::Continue)
        }
        "follow" => {
            cmd_follow(state, args)?;
            Ok(ReplControl::Continue)
        }
        "find_by_type" => {
            cmd_find_by_type(state, args)?;
            Ok(ReplControl::Continue)
        }
        "find_paths" => {
            cmd_find_paths(state, args)?;
            Ok(ReplControl::Continue)
        }
        "gen" => {
            cmd_gen(state, args)?;
            Ok(ReplControl::Continue)
        }
        "q" | "axql" => {
            cmd_axql(state, args)?;
            Ok(ReplControl::Continue)
        }
        "sql" => {
            cmd_sqlish(state, args)?;
            Ok(ReplControl::Continue)
        }
        "ask" => {
            cmd_ask(state, args)?;
            Ok(ReplControl::Continue)
        }
        "llm" => {
            cmd_llm(state, args)?;
            Ok(ReplControl::Continue)
        }
        "wm" | "world_model" => {
            cmd_world_model(state, args)?;
            Ok(ReplControl::Continue)
        }
        "match_proto_enterprise" => {
            cmd_match_proto_enterprise(state, args)?;
            Ok(ReplControl::Continue)
        }
        "viz" => {
            cmd_viz(state, args)?;
            Ok(ReplControl::Continue)
        }
        _ => Err(anyhow!("unknown command `{cmd}` (type `help`)")),
    }
}

#[derive(Default)]
struct ReplState {
    db: Option<axiograph_pathdb::PathDB>,
    meta: Option<axiograph_pathdb::axi_semantics::MetaPlaneIndex>,
    llm: crate::llm::LlmState,
    world_model: crate::world_model::WorldModelState,
    snapshot_key: String,
    contexts: Vec<crate::axql::AxqlContextSpec>,
    query_cache: crate::axql::AxqlPreparedQueryCache,
}

fn refresh_meta_plane_index(state: &mut ReplState) -> Result<()> {
    let Some(db) = state.db.as_ref() else {
        state.meta = None;
        return Ok(());
    };
    state.meta = Some(axiograph_pathdb::axi_semantics::MetaPlaneIndex::from_db(
        db,
    )?);
    Ok(())
}

// =============================================================================
// Tab completion (rustyline)
// =============================================================================

#[cfg(feature = "repl-rustyline")]
#[derive(Default, Debug, Clone)]
struct CompletionData {
    commands: Vec<String>,
    llm_subcommands: Vec<String>,
    llm_use_backends: Vec<String>,
    scenarios: Vec<String>,
    types: Vec<String>,
    relations: Vec<String>,
    names: Vec<String>,
    contexts: Vec<String>,
}

#[cfg(feature = "repl-rustyline")]
fn refresh_completion_data(
    data: &std::sync::Arc<std::sync::RwLock<CompletionData>>,
    state: &ReplState,
) {
    let mut types: BTreeSet<String> = BTreeSet::new();
    let mut relations: BTreeSet<String> = BTreeSet::new();
    let mut names: BTreeSet<String> = BTreeSet::new();
    let mut contexts: BTreeSet<String> = BTreeSet::new();

    if let Some(db) = state.db.as_ref() {
        for entity_id in 0..db.entities.len() as u32 {
            let Some(type_id) = db.entities.get_type(entity_id) else {
                continue;
            };
            let Some(name) = db.interner.lookup(type_id) else {
                continue;
            };
            types.insert(name);
        }

        for rel_id in 0..db.relations.len() as u32 {
            let Some(rel) = db.relations.get_relation(rel_id) else {
                continue;
            };
            let Some(name) = db.interner.lookup(rel.rel_type) else {
                continue;
            };
            relations.insert(name);
        }

        let name_key_id = db.interner.id_of("name");
        if let Some(key_id) = name_key_id {
            for entity_id in 0..db.entities.len() as u32 {
                if let Some(value_id) = db.entities.get_attr(entity_id, key_id) {
                    if let Some(value) = db.interner.lookup(value_id) {
                        names.insert(value);
                    }
                }
            }
        }

        if let Some(ctxs) = db.find_by_type("Context") {
            for id in ctxs.iter() {
                if let Some(view) = db.get_entity(id) {
                    if let Some(name) = view.attrs.get("name") {
                        contexts.insert(name.clone());
                    }
                }
            }
        }
    }

    let mut completion_data = data.write().expect("completion lock poisoned");
    completion_data.commands = vec![
        "help".to_string(),
        "?".to_string(),
        "exit".to_string(),
        "quit".to_string(),
        "stats".to_string(),
        "analyze".to_string(),
        "quality".to_string(),
        "load".to_string(),
        "save".to_string(),
        "import_axi".to_string(),
        "import_proto".to_string(),
        "export_axi".to_string(),
        "export_axi_module".to_string(),
        "build_indexes".to_string(),
        "add_entity".to_string(),
        "add_edge".to_string(),
        "add_equiv".to_string(),
        "ctx".to_string(),
        "schema".to_string(),
        "constraints".to_string(),
        "rules".to_string(),
        "validate_axi".to_string(),
        "learning_graph".to_string(),
        "show".to_string(),
        "describe".to_string(),
        "neigh".to_string(),
        "open".to_string(),
        "diff".to_string(),
        "follow".to_string(),
        "find_by_type".to_string(),
        "find_paths".to_string(),
        "gen".to_string(),
        "q".to_string(),
        "axql".to_string(),
        "sql".to_string(),
        "ask".to_string(),
        "llm".to_string(),
        "wm".to_string(),
        "match_proto_enterprise".to_string(),
        "viz".to_string(),
    ];
    completion_data.llm_subcommands = vec![
        "status".to_string(),
        "use".to_string(),
        "model".to_string(),
        "disable".to_string(),
        "ask".to_string(),
        "query".to_string(),
        "answer".to_string(),
        "agent".to_string(),
    ];
    let mut llm_use_backends = vec!["mock".to_string(), "command".to_string()];
    #[cfg(feature = "llm-ollama")]
    {
        llm_use_backends.push("ollama".to_string());
    }
    completion_data.llm_use_backends = llm_use_backends;
    completion_data.scenarios = vec![
        "enterprise".to_string(),
        "enterprise_large_api".to_string(),
        "economic_flows".to_string(),
        "machinist_learning".to_string(),
        "schema_evolution".to_string(),
        "proto_api".to_string(),
        "social_network".to_string(),
        "supply_chain".to_string(),
    ];
    completion_data.types = types.into_iter().collect();
    completion_data.relations = relations.into_iter().collect();
    completion_data.names = names.into_iter().take(512).collect();
    completion_data.contexts = contexts.into_iter().take(256).collect();
}

#[cfg(feature = "repl-rustyline")]
struct ReplLineHelper {
    files: rustyline::completion::FilenameCompleter,
    data: std::sync::Arc<std::sync::RwLock<CompletionData>>,
}

#[cfg(feature = "repl-rustyline")]
impl ReplLineHelper {
    fn new(data: std::sync::Arc<std::sync::RwLock<CompletionData>>) -> Self {
        Self {
            files: rustyline::completion::FilenameCompleter::new(),
            data,
        }
    }

    fn pairs_from_prefix(items: &[String], prefix: &str) -> Vec<rustyline::completion::Pair> {
        let mut pairs = Vec::new();
        for item in items {
            if item.starts_with(prefix) {
                pairs.push(rustyline::completion::Pair {
                    display: item.clone(),
                    replacement: item.clone(),
                });
            }
        }
        pairs
    }
}

#[cfg(feature = "repl-rustyline")]
impl rustyline::Helper for ReplLineHelper {}

#[cfg(feature = "repl-rustyline")]
impl rustyline::highlight::Highlighter for ReplLineHelper {}

#[cfg(feature = "repl-rustyline")]
impl rustyline::hint::Hinter for ReplLineHelper {
    type Hint = String;
    fn hint(&self, _line: &str, _pos: usize, _ctx: &rustyline::Context<'_>) -> Option<String> {
        None
    }
}

#[cfg(feature = "repl-rustyline")]
impl rustyline::validate::Validator for ReplLineHelper {}

#[cfg(feature = "repl-rustyline")]
impl rustyline::completion::Completer for ReplLineHelper {
    type Candidate = rustyline::completion::Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        let start = line[..pos]
            .rfind(|c: char| c.is_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);
        let word = &line[start..pos];
        let prefix_line = &line[..start];
        let tokens: Vec<&str> = prefix_line.split_whitespace().collect();

        let data = self.data.read().expect("completion lock poisoned");

        // Completing the first token => command completion.
        if tokens.is_empty() {
            return Ok((start, Self::pairs_from_prefix(&data.commands, word)));
        }

        let cmd = tokens[0];

        // File-path-ish commands.
        if matches!(
            cmd,
            "load" | "save" | "import_axi" | "import_proto" | "export_axi" | "viz"
        ) {
            return self.files.complete(line, pos, ctx);
        }

        // Type and relation completions from the loaded DB.
        if cmd == "find_by_type" {
            return Ok((start, Self::pairs_from_prefix(&data.types, word)));
        }
        if cmd == "follow" {
            return Ok((start, Self::pairs_from_prefix(&data.relations, word)));
        }
        if cmd == "ctx" {
            if tokens.len() == 1 {
                let items = vec![
                    "show".to_string(),
                    "list".to_string(),
                    "use".to_string(),
                    "add".to_string(),
                    "clear".to_string(),
                ];
                return Ok((start, Self::pairs_from_prefix(&items, word)));
            }
            if tokens.len() == 2 && matches!(tokens[1], "use" | "add") {
                return Ok((start, Self::pairs_from_prefix(&data.contexts, word)));
            }
        }
        if matches!(cmd, "show" | "describe" | "neigh") {
            return Ok((start, Self::pairs_from_prefix(&data.names, word)));
        }
        if cmd == "open" {
            if tokens.len() == 1 {
                let items = vec![
                    "chunk".to_string(),
                    "doc".to_string(),
                    "evidence".to_string(),
                    "entity".to_string(),
                ];
                return Ok((start, Self::pairs_from_prefix(&items, word)));
            }
            if tokens.len() >= 2 && matches!(tokens[1], "chunk" | "doc" | "evidence" | "entity") {
                return Ok((start, Self::pairs_from_prefix(&data.names, word)));
            }
        }
        if cmd == "diff" {
            if tokens.len() == 1 {
                let items = vec!["ctx".to_string()];
                return Ok((start, Self::pairs_from_prefix(&items, word)));
            }
            if tokens.len() >= 2 && tokens[1] == "ctx" {
                return Ok((start, Self::pairs_from_prefix(&data.contexts, word)));
            }
        }

        // `gen` supports both numeric and scenario modes. Offer scenario names
        // and the explicit `scenario` keyword as a convenience.
        if cmd == "gen" {
            if tokens.len() == 1 {
                let mut items = vec!["scenario".to_string()];
                items.extend(data.scenarios.iter().cloned());
                return Ok((start, Self::pairs_from_prefix(&items, word)));
            }
            if tokens.len() == 2 && tokens[1].eq_ignore_ascii_case("scenario") {
                return Ok((start, Self::pairs_from_prefix(&data.scenarios, word)));
            }
        }

        // `llm ...` subcommands
        if cmd == "llm" {
            if tokens.len() == 1 {
                return Ok((start, Self::pairs_from_prefix(&data.llm_subcommands, word)));
            }
            if tokens.len() == 2 && tokens[1].eq_ignore_ascii_case("use") {
                return Ok((start, Self::pairs_from_prefix(&data.llm_use_backends, word)));
            }
            if tokens.len() == 3
                && tokens[1].eq_ignore_ascii_case("use")
                && tokens[2].eq_ignore_ascii_case("command")
            {
                return self.files.complete(line, pos, ctx);
            }
        }

        Ok((start, Vec::new()))
    }
}

fn print_help() {
    println!(
        r#"Commands:
  help | ?                       Show this help
  exit | quit                    Exit the REPL

  load <file.axpd>               Load a PathDB snapshot
  save <file.axpd>               Save the current PathDB snapshot

  import_axi <file.axi>          Import either a `PathDBExportV1` snapshot or a canonical `axi_schema_v1` module
  import_proto <descriptor.json> [schema_hint]
                                 Import a Buf descriptor set JSON into the current DB
                                 (adds Proto* entities + relations; use `match_proto_enterprise` to link to `enterprise*` scenarios)
  export_axi <file.axi>          Export current PathDB as `PathDBExportV1` `.axi`
  export_axi_module <file.axi> [module_name]
                                 Export a canonical `axi_schema_v1` module from the meta-plane (if imported)
  ctx [show|list|use|add|clear]   Manage optional context/world scoping for queries
  schema [name]                  Inspect imported `.axi` schema/theory metadata (meta-plane)
  constraints <schema> [relation]
                                 Show imported theory constraints (keys/functionals/etc) for a schema/relation
  rules [theory] [rule]          Show imported theory rewrite rules (meta-plane)
  validate_axi                   Type-check imported canonical `.axi` instance data against the meta-plane schema
  learning_graph <schema>        Extract a typed learning graph (Concept prerequisites + links) from the current DB

  gen <entities> <edges> <types> [index_depth] [seed]
                                 Generate a synthetic graph and build indexes
  gen scenario <name> [scale] [index_depth] [seed]
                                 Generate a scenario graph (typed shapes + homotopies)
                                 Scenarios: enterprise | enterprise_large_api | economic_flows | machinist_learning | schema_evolution | proto_api | proto_api_business | social_network | supply_chain
  gen <name> [scale] [index_depth] [seed]
                                 Shorthand for `gen scenario <name> ...`
  build_indexes                  Build PathDB indexes for the current DB
  add_entity <Type> <name> [k=v...]
                                 Mutate the current DB by adding an entity
  add_edge <rel> <src> <dst> [confidence <c>] [k=v...]
                                 Mutate the current DB by adding a relation edge
                                 (`src`/`dst` may be numeric ids or `name` strings)
  add_equiv <left> <right> <equiv_type>
                                 Mutate the current DB by adding an equivalence (dashed in viz)
  stats                          Print current DB stats
  analyze network [out] [opts]   Network analysis over the current DB (tooling)
  quality [out] [opts]           Quality checks over the current DB (tooling)

  show <entity_id>               Show an entity (type + attributes)
  describe <entity>              Show an entity with neighborhood summary (in/out edges, contexts, equivs)
  neigh <entity> [options...]    Summarize a neighborhood (and optionally render a viz file)
  open <subcmd> <arg> [opts]     Open a DocChunk / evidence item for inspection
  diff ctx <c1> <c2> [opts]      Compare fact sets across contexts/worlds
  find_by_type <type_name>       List entities of a given type (first 20)
  follow <start_id> <rel...>     Follow a relation path (prints count + first 20)
  follow <start_id> <path_expr>  Follow an RPQ path expression (e.g. `rel_0/rel_1`, `(a|b)*`)
                                 Optional: `max_hops N`
  find_paths <from> <to> <depth> Find paths between entities (prints first 10)

  q <AxQL query>                 Pattern-match query language (datalog-ish)
                                 Prints cache hit/miss + elapsed time
  q --elaborate <AxQL query>     Typecheck + show elaborated query (inferred types)
  q --typecheck <AxQL query>     Typecheck only (no execution)
  sql <SQL query>                SQL-ish dialect compiled into the same query core
  ask <query>                    Natural-language-ish templates compiled into AxQL
  llm <subcommand>               LLM-assisted query translation / answering
  wm <subcommand>                World-model proposal generation (untrusted)
  match_proto_enterprise          Add heuristic links from `Service` → `ProtoService` (and reverse), so enterprise graphs can traverse into imported proto surfaces
  viz <out> [options...]         Export a neighborhood visualization (dot/html/json)
                                 Options:
                                   format dot|html|json
                                   plane data|meta|both
                                   focus <entity_id> | focus_name <name>
                                   focus_type <TypeName>
                                   hops <n> | max_nodes <n> | max_edges <n>
                                   direction out|in|both
                                   include_meta | no_equivalences

AxQL examples:
  q select ?y where 0 -rel_0/rel_1-> ?y
  q select ?y where 0 -(rel_0|rel_1)-> ?y
  q select ?x where ?x : Node, has(?x, rel_0), attrs(?x, name="node_42")
  q select ?x where ?x {{ is Node, rel_0, name="node_42" }}
  q select ?x where ?x -rel_0-> name("b")
  q select ?x where ?x -rel_0-> b
  q select ?f where ?f = Flow(from=a, to=b)
  q select ?f where ?f = Flow(from=a, to=b) in Accepted
  q select ?x where ?x : Node, contains(?x, "name", "b")
  q select ?c where ?c : DocChunk, fts(?c, "text", "capture payment")
  q select ?x where ?x : Material, fuzzy(?x, "name", "titainum", 2)
  q select ?x ?y where ?x : Node, ?x -rel_0-> ?y limit 5
  q select ?x where ?x : Node, attr(?x, "name", "node_42")

SQL-ish examples:
  sql SELECT y FROM Node AS y WHERE FOLLOW(0, 'rel_0/rel_1', y) LIMIT 10;
  sql SELECT x FROM Node AS x WHERE HAS(x, 'rel_0') AND ATTR(x, 'name') = 'a' LIMIT 5;

Ask examples:
  ask find Node named b
  ask find nodes has rel_0
  ask from 0 follow rel_0/rel_1 max_hops 5

LLM examples:
  llm use mock
  llm use ollama llama3.2
  llm ask find Node named b
  llm answer from 0 follow rel_0 then rel_1 max hops 5
  llm agent what RPCs does acme.svc0.v1.Service0 have?
"#
    );
}

fn one_path_arg(cmd: &str, args: &[String]) -> Result<PathBuf> {
    if args.len() != 1 {
        return Err(anyhow!("usage: {cmd} <path>"));
    }
    Ok(PathBuf::from(&args[0]))
}

fn require_db(state: &ReplState) -> Result<&axiograph_pathdb::PathDB> {
    state
        .db
        .as_ref()
        .ok_or_else(|| anyhow!("no database loaded (use `load`, `import_axi`, or `gen`)"))
}

fn require_db_mut(state: &mut ReplState) -> Result<&mut axiograph_pathdb::PathDB> {
    state
        .db
        .as_mut()
        .ok_or_else(|| anyhow!("no database loaded (use `load`, `import_axi`, or `gen`)"))
}

fn set_snapshot_key(state: &mut ReplState, key: String) {
    state.snapshot_key = key;
    state.query_cache.clear();
}

fn chain_snapshot_key(prev: &str, op: &str, extra: &str) -> String {
    let mut bytes: Vec<u8> = Vec::new();
    bytes.extend_from_slice(prev.as_bytes());
    bytes.extend_from_slice(b"|");
    bytes.extend_from_slice(op.as_bytes());
    bytes.extend_from_slice(b"|");
    bytes.extend_from_slice(extra.as_bytes());
    axiograph_dsl::digest::fnv1a64_digest_bytes(&bytes)
}

fn cmd_stats(state: &ReplState) {
    let Some(db) = state.db.as_ref() else {
        println!("(no database loaded)");
        return;
    };
    println!(
        "entities={} relations={} equivalence_keys={}",
        db.entities.len(),
        db.relations.len(),
        db.equivalences.len()
    );
}

fn cmd_ctx(state: &mut ReplState, args: &[String]) -> Result<()> {
    // Context scoping is a REPL convenience: it sets a default `in <context>`
    // filter for subsequent AxQL/SQL-ish/Ask queries.
    if args.is_empty() || args[0].eq_ignore_ascii_case("show") {
        if state.contexts.is_empty() {
            println!("ctx: (none)");
            return Ok(());
        }
        let db = require_db(state)?;
        println!("ctx:");
        for c in &state.contexts {
            match c {
                crate::axql::AxqlContextSpec::EntityId(id) => {
                    println!("  - {}", describe_entity(db, *id));
                }
                crate::axql::AxqlContextSpec::Name(name) => match resolve_entity_ref(db, name) {
                    Ok(id) => println!("  - {} (name={})", describe_entity(db, id), name),
                    Err(_) => println!("  - (unresolved) name={}", name),
                },
            }
        }
        return Ok(());
    }

    match args[0].to_ascii_lowercase().as_str() {
        "clear" | "unset" => {
            state.contexts.clear();
            println!("ok: ctx cleared");
            Ok(())
        }
        "use" => {
            if args.len() != 2 {
                return Err(anyhow!("usage: ctx use <context_name|entity_id>"));
            }
            let token = args[1].as_str();
            // Validate upfront to avoid confusing “silent empty results”.
            let db = require_db(state)?;
            let _ = resolve_entity_ref(db, token)?;
            let spec = if let Ok(id) = token.parse::<u32>() {
                crate::axql::AxqlContextSpec::EntityId(id)
            } else {
                crate::axql::AxqlContextSpec::Name(token.to_string())
            };
            state.contexts.clear();
            state.contexts.push(spec);
            cmd_ctx(state, &["show".to_string()])
        }
        "add" => {
            if args.len() != 2 {
                return Err(anyhow!("usage: ctx add <context_name|entity_id>"));
            }
            let token = args[1].as_str();
            let db = require_db(state)?;
            let _ = resolve_entity_ref(db, token)?;
            let spec = if let Ok(id) = token.parse::<u32>() {
                crate::axql::AxqlContextSpec::EntityId(id)
            } else {
                crate::axql::AxqlContextSpec::Name(token.to_string())
            };
            if !state.contexts.contains(&spec) {
                state.contexts.push(spec);
            }
            cmd_ctx(state, &["show".to_string()])
        }
        "list" => {
            let db = require_db(state)?;
            let mut ids: Vec<u32> = Vec::new();

            if let Some(ctxs) = db.find_by_type("Context") {
                ids.extend(ctxs.iter());
            }

            if let Some(ctx_rel_id) = db
                .interner
                .id_of(axiograph_pathdb::axi_meta::REL_AXI_FACT_IN_CONTEXT)
            {
                for rel_id in 0..db.relations.len() as u32 {
                    let Some(rel) = db.relations.get_relation(rel_id) else {
                        continue;
                    };
                    if rel.rel_type == ctx_rel_id {
                        ids.push(rel.target);
                    }
                }
            }

            ids.sort_unstable();
            ids.dedup();

            if ids.is_empty() {
                println!("(no contexts found)");
                return Ok(());
            }

            println!("contexts:");
            for id in ids {
                println!("  - {}", describe_entity(db, id));
            }
            Ok(())
        }
        other => Err(anyhow!(
            "unknown ctx subcommand `{other}` (try: ctx show|list|use|add|clear)"
        )),
    }
}

fn cmd_load_axpd(state: &mut ReplState, path: &PathBuf) -> Result<()> {
    let bytes = fs::read(path)?;
    let snapshot_key = axiograph_dsl::digest::fnv1a64_digest_bytes(&bytes);
    let db = axiograph_pathdb::PathDB::from_bytes(&bytes)?;
    state.db = Some(db);
    set_snapshot_key(state, snapshot_key);
    refresh_meta_plane_index(state)?;
    println!("loaded {}", path.display());
    Ok(())
}

fn cmd_save_axpd(state: &mut ReplState, path: &PathBuf) -> Result<()> {
    let db = require_db(state)?;
    let bytes = db.to_bytes()?;
    fs::write(path, bytes)?;
    println!("wrote {}", path.display());
    Ok(())
}

fn cmd_import_axi(state: &mut ReplState, path: &PathBuf) -> Result<()> {
    let path = resolve_path_with_repo_fallback(path)?;
    let text = fs::read_to_string(&path)?;
    let module_digest = axiograph_dsl::digest::axi_digest_v1(&text);
    let m = axiograph_dsl::axi_v1::parse_axi_v1(&text)?;

    let is_snapshot = m
        .schemas
        .iter()
        .any(|s| s.name == axiograph_pathdb::axi_export::PATHDB_EXPORT_SCHEMA_NAME_V1)
        && m.instances.iter().any(|i| {
            i.schema == axiograph_pathdb::axi_export::PATHDB_EXPORT_SCHEMA_NAME_V1
                && i.name == axiograph_pathdb::axi_export::PATHDB_EXPORT_INSTANCE_NAME_V1
        });

    if is_snapshot {
        let db = axiograph_pathdb::axi_export::import_pathdb_from_axi_v1_module(&m)?;
        state.db = Some(db);
        set_snapshot_key(state, module_digest);
        refresh_meta_plane_index(state)?;
        println!("imported PathDB snapshot {}", path.display());
        return Ok(());
    }

    let summary = {
        let db = state.db.get_or_insert_with(axiograph_pathdb::PathDB::new);
        let summary =
            axiograph_pathdb::axi_module_import::import_axi_schema_v1_module_into_pathdb(db, &m)?;
        db.build_indexes();
        summary
    };
    let next_key = if state.snapshot_key.is_empty() {
        module_digest
    } else {
        chain_snapshot_key(&state.snapshot_key, "import_axi", &module_digest)
    };
    set_snapshot_key(state, next_key);
    refresh_meta_plane_index(state)?;
    println!(
        "imported axi_schema_v1 module {} (meta_entities={} meta_relations={} instances={} entities={} upgraded_types={} tuple_entities={} relations={} derived_edges={})",
        path.display(),
        summary.meta_entities_added,
        summary.meta_relations_added,
        summary.instances_imported,
        summary.entities_added,
        summary.entity_type_upgrades,
        summary.tuple_entities_added,
        summary.relations_added,
        summary.derived_edges_added
    );
    Ok(())
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn resolve_path_with_repo_fallback(input: &PathBuf) -> Result<PathBuf> {
    if input.exists() {
        return Ok(input.clone());
    }

    // Convenience for REPL scripts executed from sandboxed tmp dirs in tests:
    // treat repo-root-relative paths as valid when cwd-relative lookup fails.
    let candidate = repo_root().join(input);
    if candidate.exists() {
        return Ok(candidate);
    }

    Err(anyhow!("no such file: {}", input.display()))
}

fn cmd_import_proto(state: &mut ReplState, args: &[String]) -> Result<()> {
    if !(1..=2).contains(&args.len()) {
        return Err(anyhow!(
            "usage: import_proto <descriptor.json> [schema_hint]"
        ));
    }

    let path = resolve_path_with_repo_fallback(&PathBuf::from(&args[0]))?;
    let schema_hint = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "proto_api".to_string());

    let text = fs::read_to_string(&path)?;
    let ingest_digest = axiograph_dsl::digest::axi_digest_v1(&text);
    let ingest = axiograph_ingest_proto::ingest_descriptor_set_json(
        &text,
        Some(path.display().to_string()),
        Some(schema_hint.clone()),
    )?;

    let db = state.db.get_or_insert_with(axiograph_pathdb::PathDB::new);

    let start = Instant::now();
    let chunks_summary = crate::doc_chunks::import_chunks_into_pathdb(db, &ingest.chunks)?;

    let generated_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();
    let proposals_file = axiograph_ingest_docs::ProposalsFileV1 {
        version: axiograph_ingest_docs::PROPOSALS_VERSION_V1,
        generated_at,
        source: axiograph_ingest_docs::ProposalSourceV1 {
            source_type: "proto".to_string(),
            locator: path.display().to_string(),
        },
        schema_hint: Some(schema_hint),
        proposals: ingest.proposals,
    };

    let proposals_summary =
        crate::proposals_import::import_proposals_file_into_pathdb(db, &proposals_file, &ingest_digest)?;
    db.build_indexes();

    println!(
        "imported proto descriptor: chunks_added={} proposals_entities_added={} proposals_relation_facts_added={} derived_edges_added={} evidence_links_added={} (reindexed in {:?})",
        chunks_summary.chunks_added,
        proposals_summary.entities_added,
        proposals_summary.relation_facts_added,
        proposals_summary.derived_edges_added,
        proposals_summary.evidence_links_added,
        start.elapsed()
    );
    let next_key = if state.snapshot_key.is_empty() {
        ingest_digest
    } else {
        chain_snapshot_key(&state.snapshot_key, "import_proto", &ingest_digest)
    };
    set_snapshot_key(state, next_key);
    refresh_meta_plane_index(state)?;
    Ok(())
}

fn tokenize_alnum(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();

    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            cur.push(c.to_ascii_lowercase());
            continue;
        }
        if !cur.is_empty() {
            out.push(cur.clone());
            cur.clear();
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }

    out
}

fn enterprise_service_match_tokens(service_name: &str) -> Vec<String> {
    let lower = service_name.trim().to_ascii_lowercase();
    let mut tokens = tokenize_alnum(&lower);

    if let Some(rest) = lower.strip_prefix("svc_") {
        if !rest.is_empty() {
            tokens.push(rest.to_string());
            if rest.chars().all(|c| c.is_ascii_digit()) {
                tokens.push(format!("svc{rest}"));
                tokens.push(format!("svc_{rest}"));
            }
            if rest.ends_with('s') && rest.len() > 1 {
                tokens.push(rest[..rest.len() - 1].to_string());
            }
        }
    }

    tokens.sort();
    tokens.dedup();
    tokens
}

fn cmd_match_proto_enterprise(state: &mut ReplState, args: &[String]) -> Result<()> {
    if !args.is_empty() {
        return Err(anyhow!("usage: match_proto_enterprise"));
    }

    let db = require_db_mut(state)?;

    let Some(enterprise_services_bm) = db.find_by_type("Service") else {
        return Err(anyhow!(
            "no `Service` entities found (try: `gen enterprise ...`)"
        ));
    };
    let Some(proto_services_bm) = db.find_by_type("ProtoService") else {
        return Err(anyhow!(
            "no `ProtoService` entities found (try: `import_proto <descriptor.json>`)"
        ));
    };

    let enterprise_services: Vec<u32> = enterprise_services_bm.iter().collect();
    let proto_services: Vec<u32> = proto_services_bm.iter().collect();

    // Index proto services by tokens extracted from their FQN.
    let mut proto_by_token: std::collections::HashMap<String, Vec<u32>> =
        std::collections::HashMap::new();
    for pid in &proto_services {
        let Some(view) = db.get_entity(*pid) else {
            continue;
        };
        let fqn = view
            .attrs
            .get("fqn")
            .or_else(|| view.attrs.get("name"))
            .cloned()
            .unwrap_or_default();
        for tok in tokenize_alnum(&fqn) {
            proto_by_token.entry(tok).or_default().push(*pid);
        }
    }

    let rel_service_to_proto = "mapsToProtoService";
    let rel_proto_to_service = "mapsToService";
    let rel_service_to_proto_id = db.interner.intern(rel_service_to_proto);
    let rel_proto_to_service_id = db.interner.intern(rel_proto_to_service);

    let mut added = 0usize;
    let mut matched = 0usize;

    for sid in enterprise_services {
        let Some(service_name) = db
            .get_entity(sid)
            .and_then(|v| v.attrs.get("name").cloned())
        else {
            continue;
        };
        let tokens = enterprise_service_match_tokens(&service_name);

        let mut candidates: Vec<u32> = Vec::new();
        for tok in &tokens {
            if let Some(ids) = proto_by_token.get(tok) {
                candidates.extend(ids.iter().copied());
            }
        }
        candidates.sort();
        candidates.dedup();

        // Pick a deterministic best candidate: lexicographically smallest FQN.
        let mut best: Option<(String, u32)> = None;
        for pid in candidates {
            let Some(view) = db.get_entity(pid) else {
                continue;
            };
            let fqn = view
                .attrs
                .get("fqn")
                .or_else(|| view.attrs.get("name"))
                .cloned()
                .unwrap_or_default();
            if best.as_ref().map(|(cur, _)| fqn < *cur).unwrap_or(true) {
                best = Some((fqn, pid));
            }
        }

        let Some((proto_fqn, pid)) = best else {
            continue;
        };
        matched += 1;

        let attrs: Vec<(String, String)> = vec![
            ("match_kind".to_string(), "token_intersection".to_string()),
            ("service_name".to_string(), service_name.clone()),
            ("proto_service_fqn".to_string(), proto_fqn),
        ];
        let attrs_ref: Vec<(&str, &str)> = attrs
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        if !db.relations.has_edge(sid, rel_service_to_proto_id, pid) {
            db.add_relation(rel_service_to_proto, sid, pid, 0.75, attrs_ref.clone());
            added += 1;
        }
        if !db.relations.has_edge(pid, rel_proto_to_service_id, sid) {
            db.add_relation(rel_proto_to_service, pid, sid, 0.75, attrs_ref.clone());
            added += 1;
        }
    }

    // Keep the path index consistent for follow/RPQ/AxQL.
    db.build_indexes();

    println!(
        "matched {} Service nodes; added {} mapping edges (`{}` + `{}`)",
        matched, added, rel_service_to_proto, rel_proto_to_service
    );
    Ok(())
}

fn cmd_viz(state: &ReplState, args: &[String]) -> Result<()> {
    if args.is_empty() {
        return Err(anyhow!(
            "usage: viz <out_path> [options...]\n\
             options:\n\
               format dot|html|json\n\
               plane data|meta|both\n\
               focus <entity_id> | focus_name <name>\n\
               focus_type <TypeName>\n\
               hops <n> | max_nodes <n> | max_edges <n>\n\
               direction out|in|both\n\
               include_meta | typed_overlay | no_equivalences"
        ));
    }

    let out = PathBuf::from(&args[0]);

    let mut format = "dot".to_string();
    let mut plane = "data".to_string();
    let mut focus_ids: Vec<u32> = Vec::new();
    let mut focus_name: Option<String> = None;
    let mut focus_type: Option<String> = None;
    let mut hops: usize = 2;
    let mut max_nodes: usize = 250;
    let mut max_edges: usize = 4000;
    let mut direction = "both".to_string();
    let mut include_meta = false;
    let mut include_equivalences = true;
    let mut typed_overlay = false;

    let mut i = 1usize;
    while i < args.len() {
        match args[i].as_str() {
            "format" => {
                if i + 1 >= args.len() {
                    return Err(anyhow!("viz: missing value for `format`"));
                }
                format = args[i + 1].clone();
                i += 2;
            }
            "focus" => {
                if i + 1 >= args.len() {
                    return Err(anyhow!("viz: missing value for `focus`"));
                }
                focus_ids.push(args[i + 1].parse()?);
                i += 2;
            }
            "focus_name" => {
                if i + 1 >= args.len() {
                    return Err(anyhow!("viz: missing value for `focus_name`"));
                }
                focus_name = Some(args[i + 1].clone());
                i += 2;
            }
            "focus_type" => {
                if i + 1 >= args.len() {
                    return Err(anyhow!("viz: missing value for `focus_type`"));
                }
                focus_type = Some(args[i + 1].clone());
                i += 2;
            }
            "plane" => {
                if i + 1 >= args.len() {
                    return Err(anyhow!("viz: missing value for `plane`"));
                }
                plane = args[i + 1].clone();
                i += 2;
            }
            "hops" => {
                if i + 1 >= args.len() {
                    return Err(anyhow!("viz: missing value for `hops`"));
                }
                hops = args[i + 1].parse()?;
                i += 2;
            }
            "max_nodes" => {
                if i + 1 >= args.len() {
                    return Err(anyhow!("viz: missing value for `max_nodes`"));
                }
                max_nodes = args[i + 1].parse()?;
                i += 2;
            }
            "max_edges" => {
                if i + 1 >= args.len() {
                    return Err(anyhow!("viz: missing value for `max_edges`"));
                }
                max_edges = args[i + 1].parse()?;
                i += 2;
            }
            "direction" => {
                if i + 1 >= args.len() {
                    return Err(anyhow!("viz: missing value for `direction`"));
                }
                direction = args[i + 1].clone();
                i += 2;
            }
            "include_meta" => {
                include_meta = true;
                i += 1;
            }
            "typed_overlay" => {
                typed_overlay = true;
                i += 1;
            }
            "no_equivalences" => {
                include_equivalences = false;
                i += 1;
            }
            other => return Err(anyhow!("viz: unexpected token `{other}`")),
        }
    }

    let db = require_db(state)?;

    if focus_ids.is_empty() {
        if let Some(name) = focus_name.as_deref() {
            if let Some(id) =
                crate::viz::resolve_focus_by_name_and_type(db, name, focus_type.as_deref())?
            {
                focus_ids.push(id);
            } else {
                return Err(anyhow!("viz: no entity found with name `{name}`"));
            }
        }
    }

    let plane = plane.trim().to_ascii_lowercase();
    let (include_meta_plane, include_data_plane) = match plane.as_str() {
        "data" => (include_meta, true),
        "meta" => (true, false),
        "both" => (true, true),
        other => {
            return Err(anyhow!(
                "viz: unknown plane `{other}` (expected data|meta|both)"
            ))
        }
    };

    let options = crate::viz::VizOptions {
        focus_ids,
        hops,
        max_nodes,
        max_edges,
        direction: crate::viz::VizDirection::parse(&direction)?,
        include_meta_plane,
        include_data_plane,
        include_equivalences,
        typed_overlay,
    };

    let g = if typed_overlay && state.meta.is_none() {
        crate::viz::extract_viz_graph(db, &options)?
    } else {
        crate::viz::extract_viz_graph_with_meta(db, &options, state.meta.as_ref())?
    };
    let rendered = match crate::viz::VizFormat::parse(&format)? {
        crate::viz::VizFormat::Dot => crate::viz::render_dot(db, &g),
        crate::viz::VizFormat::Json => crate::viz::render_json(&g)?,
        crate::viz::VizFormat::Html => crate::viz::render_html(db, &g)?,
    };
    fs::write(&out, rendered)?;
    println!(
        "wrote {} (nodes={} edges={} truncated={})",
        out.display(),
        g.nodes.len(),
        g.edges.len(),
        g.truncated
    );
    Ok(())
}

fn cmd_analyze(state: &ReplState, args: &[String]) -> Result<()> {
    if args.is_empty() {
        return Err(anyhow!(
            "usage: analyze network [out_path] [options...]\n\
             options:\n\
               format text|json\n\
               plane data|meta|both\n\
               include_equivalences | skip_facts | communities\n\
               pagerank_iters <n> | pagerank_damping <d>\n\
               betweenness_sources <n> | seed <n>\n\
               max_heavy_nodes <n> | top <n>"
        ));
    }

    match args[0].as_str() {
        "network" => cmd_analyze_network(state, &args[1..]),
        other => Err(anyhow!(
            "unknown analyze subcommand `{other}` (try: analyze network)"
        )),
    }
}

fn cmd_analyze_network(state: &ReplState, args: &[String]) -> Result<()> {
    let mut out: Option<PathBuf> = None;
    let mut format = "text".to_string();
    let mut plane = "data".to_string();
    let mut include_equivalences = false;
    let mut skip_facts = false;
    let mut pagerank_iters: usize = 30;
    let mut pagerank_damping: f64 = 0.85;
    let mut betweenness_sources: usize = 64;
    let mut seed: u64 = 1;
    let mut communities = false;
    let mut max_heavy_nodes: usize = 200_000;
    let mut top: usize = 25;

    let mut i = 0usize;
    if i < args.len()
        && !matches!(
            args[i].as_str(),
            "format"
                | "plane"
                | "include_equivalences"
                | "skip_facts"
                | "communities"
                | "pagerank_iters"
                | "pagerank_damping"
                | "betweenness_sources"
                | "seed"
                | "max_heavy_nodes"
                | "top"
        )
    {
        out = Some(PathBuf::from(&args[i]));
        i += 1;
    }

    while i < args.len() {
        match args[i].as_str() {
            "format" => {
                if i + 1 >= args.len() {
                    return Err(anyhow!("analyze network: missing value for `format`"));
                }
                format = args[i + 1].clone();
                i += 2;
            }
            "plane" => {
                if i + 1 >= args.len() {
                    return Err(anyhow!("analyze network: missing value for `plane`"));
                }
                plane = args[i + 1].clone();
                i += 2;
            }
            "include_equivalences" => {
                include_equivalences = true;
                i += 1;
            }
            "skip_facts" => {
                skip_facts = true;
                i += 1;
            }
            "communities" => {
                communities = true;
                i += 1;
            }
            "pagerank_iters" => {
                if i + 1 >= args.len() {
                    return Err(anyhow!(
                        "analyze network: missing value for `pagerank_iters`"
                    ));
                }
                pagerank_iters = args[i + 1].parse()?;
                i += 2;
            }
            "pagerank_damping" => {
                if i + 1 >= args.len() {
                    return Err(anyhow!(
                        "analyze network: missing value for `pagerank_damping`"
                    ));
                }
                pagerank_damping = args[i + 1].parse()?;
                i += 2;
            }
            "betweenness_sources" => {
                if i + 1 >= args.len() {
                    return Err(anyhow!(
                        "analyze network: missing value for `betweenness_sources`"
                    ));
                }
                betweenness_sources = args[i + 1].parse()?;
                i += 2;
            }
            "seed" => {
                if i + 1 >= args.len() {
                    return Err(anyhow!("analyze network: missing value for `seed`"));
                }
                seed = args[i + 1].parse()?;
                i += 2;
            }
            "max_heavy_nodes" => {
                if i + 1 >= args.len() {
                    return Err(anyhow!(
                        "analyze network: missing value for `max_heavy_nodes`"
                    ));
                }
                max_heavy_nodes = args[i + 1].parse()?;
                i += 2;
            }
            "top" => {
                if i + 1 >= args.len() {
                    return Err(anyhow!("analyze network: missing value for `top`"));
                }
                top = args[i + 1].parse()?;
                i += 2;
            }
            other => return Err(anyhow!("analyze network: unexpected token `{other}`")),
        }
    }

    let db = require_db(state)?;
    let input_label = if state.snapshot_key.is_empty() {
        "repl".to_string()
    } else {
        format!("repl:{}", state.snapshot_key)
    };
    let report = crate::analyze::analyze_network_report(
        db,
        &input_label,
        &plane,
        include_equivalences,
        skip_facts,
        pagerank_iters,
        pagerank_damping,
        betweenness_sources,
        seed,
        communities,
        max_heavy_nodes,
        top,
    )?;

    let format = format.trim().to_ascii_lowercase();
    let rendered = match format.as_str() {
        "json" => serde_json::to_string_pretty(&report)?,
        "text" => crate::analyze::render_network_report_text(&report),
        other => {
            return Err(anyhow!(
                "analyze network: unknown format `{other}` (expected text|json)"
            ))
        }
    };

    match out {
        Some(path) => {
            fs::write(&path, rendered)?;
            println!("wrote {}", path.display());
        }
        None => println!("{rendered}"),
    }

    Ok(())
}

fn cmd_quality(state: &ReplState, args: &[String]) -> Result<()> {
    let mut out: Option<PathBuf> = None;
    let mut format = "text".to_string();
    let mut profile = "fast".to_string();
    let mut plane = "both".to_string();
    let mut no_fail = false;

    let mut i = 0usize;
    if i < args.len()
        && !matches!(
            args[i].as_str(),
            "format" | "profile" | "plane" | "no_fail" | "no-fail"
        )
    {
        out = Some(PathBuf::from(&args[i]));
        i += 1;
    }

    while i < args.len() {
        match args[i].as_str() {
            "format" => {
                if i + 1 >= args.len() {
                    return Err(anyhow!("quality: missing value for `format`"));
                }
                format = args[i + 1].clone();
                i += 2;
            }
            "profile" => {
                if i + 1 >= args.len() {
                    return Err(anyhow!("quality: missing value for `profile`"));
                }
                profile = args[i + 1].clone();
                i += 2;
            }
            "plane" => {
                if i + 1 >= args.len() {
                    return Err(anyhow!("quality: missing value for `plane`"));
                }
                plane = args[i + 1].clone();
                i += 2;
            }
            "no_fail" | "no-fail" => {
                no_fail = true;
                i += 1;
            }
            other => return Err(anyhow!("quality: unexpected token `{other}`")),
        }
    }

    let db = require_db(state)?;
    let input_label = if state.snapshot_key.is_empty() {
        "repl".to_string()
    } else {
        format!("repl:{}", state.snapshot_key)
    };
    let report = crate::quality::run_quality_checks(
        db,
        &PathBuf::from(input_label),
        &profile.trim().to_ascii_lowercase(),
        &plane.trim().to_ascii_lowercase(),
    )?;

    let format = format.trim().to_ascii_lowercase();
    let rendered = match format.as_str() {
        "json" => serde_json::to_string_pretty(&report)?,
        "text" => crate::quality::render_quality_report_text(&report),
        other => {
            return Err(anyhow!(
                "quality: unknown format `{other}` (expected text|json)"
            ))
        }
    };

    match out {
        Some(path) => {
            fs::write(&path, rendered)?;
            println!("wrote {}", path.display());
        }
        None => println!("{rendered}"),
    }

    if report.summary.error_count > 0 && !no_fail {
        return Err(anyhow!(
            "quality checks found {} error(s)",
            report.summary.error_count
        ));
    }

    Ok(())
}

fn cmd_export_axi(state: &ReplState, path: &PathBuf) -> Result<()> {
    let db = require_db(state)?;
    let axi = axiograph_pathdb::axi_export::export_pathdb_to_axi_v1(db)?;
    fs::write(path, axi)?;
    println!("wrote {}", path.display());
    Ok(())
}

fn cmd_export_axi_module(state: &ReplState, args: &[String]) -> Result<()> {
    if !(1..=2).contains(&args.len()) {
        return Err(anyhow!("usage: export_axi_module <file.axi> [module_name]"));
    }

    let out = PathBuf::from(&args[0]);
    let db = require_db(state)?;

    let module_name = if args.len() == 2 {
        args[1].clone()
    } else {
        infer_single_meta_module_name(db)?
    };

    let axi = axiograph_pathdb::axi_module_export::export_axi_schema_v1_module_from_pathdb(
        db,
        &module_name,
    )?;
    fs::write(&out, axi)?;
    println!("wrote {}", out.display());
    Ok(())
}

fn infer_single_meta_module_name(db: &axiograph_pathdb::PathDB) -> Result<String> {
    let Some(mods) = db.find_by_type(axiograph_pathdb::axi_meta::META_TYPE_MODULE) else {
        return Err(anyhow!(
            "no `.axi` meta-plane module found (import a canonical `.axi` module first, or pass an explicit module_name)"
        ));
    };

    let mut names: Vec<String> = Vec::new();
    for id in mods.iter() {
        let Some(view) = db.get_entity(id) else {
            continue;
        };
        if let Some(name) = view.attrs.get("name") {
            names.push(name.clone());
        }
    }
    names.sort();
    names.dedup();

    if names.is_empty() {
        return Err(anyhow!(
            "no `.axi` meta-plane modules have a `name` attribute"
        ));
    }
    if names.len() != 1 {
        return Err(anyhow!(
            "multiple `.axi` modules imported: {:?} (pass an explicit module_name)",
            names
        ));
    }
    Ok(names[0].clone())
}

fn cmd_build_indexes(state: &mut ReplState) -> Result<()> {
    let start = Instant::now();
    {
        let db = require_db_mut(state)?;
        db.build_indexes();
    }
    refresh_meta_plane_index(state)?;
    println!("built indexes in {:?}", start.elapsed());
    Ok(())
}

fn parse_kv_pairs(tokens: &[String]) -> Result<Vec<(String, String)>> {
    let mut out: Vec<(String, String)> = Vec::new();
    for t in tokens {
        let Some((k, v)) = t.split_once('=') else {
            return Err(anyhow!("expected `key=value`, got `{t}`"));
        };
        let k = k.trim();
        if k.is_empty() {
            return Err(anyhow!("expected `key=value`, got `{t}` (empty key)"));
        }
        out.push((k.to_string(), v.to_string()));
    }
    Ok(out)
}

fn resolve_entity_ref(db: &axiograph_pathdb::PathDB, token: &str) -> Result<u32> {
    if let Ok(id) = token.parse::<u32>() {
        if db.get_entity(id).is_some() {
            return Ok(id);
        }
        return Err(anyhow!("no entity with id {id}"));
    }

    let Some(key_id) = db.interner.id_of("name") else {
        return Err(anyhow!("database has no `name` attribute interned"));
    };
    let Some(value_id) = db.interner.id_of(token) else {
        return Err(anyhow!("no entity found with name `{token}`"));
    };

    let ids = db.entities.entities_with_attr_value(key_id, value_id);
    if ids.is_empty() {
        return Err(anyhow!("no entity found with name `{token}`"));
    }
    if ids.len() == 1 {
        return Ok(ids.iter().next().unwrap_or(0));
    }

    let mut examples: Vec<String> = Vec::new();
    for id in ids.iter().take(5) {
        examples.push(describe_entity(db, id));
    }
    Err(anyhow!(
        "ambiguous name `{token}` ({} matches). Pass a numeric id. Examples: {}",
        ids.len(),
        examples.join(", ")
    ))
}

fn cmd_add_entity(state: &mut ReplState, args: &[String]) -> Result<()> {
    if args.len() < 2 {
        return Err(anyhow!("usage: add_entity <Type> <name> [k=v...]"));
    }

    let raw_type_name = args[0].as_str();
    let name = args[1].as_str();
    let extra_attrs = parse_kv_pairs(&args[2..])?;

    let mut explicit_schema_attr: Option<String> = None;
    for (k, v) in &extra_attrs {
        if k == axiograph_pathdb::axi_meta::ATTR_AXI_SCHEMA {
            let v = v.trim();
            if !v.is_empty() {
                explicit_schema_attr = Some(v.to_string());
            }
        }
    }

    let (schema_prefix, type_name) = if let Some((schema, ty)) = raw_type_name.split_once('.') {
        let schema = schema.trim();
        let ty = ty.trim();
        if !schema.is_empty() && !ty.is_empty() {
            (Some(schema.to_string()), ty.to_string())
        } else {
            (None, raw_type_name.to_string())
        }
    } else {
        (None, raw_type_name.to_string())
    };

    // If this looks like an `.axi` schema-scoped object type, prefer the typed
    // builder so we always stamp `axi_schema` and reject unknown object types.
    let schema_for_typed_entity: Option<String> = if let Some(s) = schema_prefix {
        Some(s)
    } else if let Some(s) = explicit_schema_attr.clone() {
        Some(s)
    } else if let Some(meta) = state.meta.as_ref() {
        let mut schemas: Vec<String> = Vec::new();
        for (schema_name, schema) in &meta.schemas {
            if schema.object_types.contains(&type_name) {
                schemas.push(schema_name.clone());
            }
        }
        schemas.sort();
        schemas.dedup();
        match schemas.len() {
            0 => None,
            1 => Some(schemas[0].clone()),
            _ => {
                return Err(anyhow!(
                    "add_entity: object type `{}` exists in multiple schemas: {:?} (use `Schema.Type` or pass `axi_schema=...`)",
                    type_name,
                    schemas
                ));
            }
        }
    } else {
        None
    };

    let mut attrs: Vec<(String, String)> = Vec::with_capacity(1 + extra_attrs.len());
    attrs.push(("name".to_string(), name.to_string()));
    attrs.extend(extra_attrs);
    let attrs_ref: Vec<(&str, &str)> = attrs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let id = {
        let db = require_db_mut(state)?;
        if let Some(schema_name) = schema_for_typed_entity.as_ref() {
            let mut checked = axiograph_pathdb::CheckedDbMut::new(db)?;
            if let Some(explicit) = explicit_schema_attr.as_ref() {
                if explicit != schema_name {
                    return Err(anyhow!(
                        "add_entity: schema mismatch (type resolves to schema `{}`, but attr axi_schema=`{}` was provided)",
                        schema_name,
                        explicit
                    ));
                }
            }

            let mut builder = checked.entity_builder(schema_name, &type_name)?;
            for (k, v) in &attrs {
                builder = builder.with_attr(k, v);
            }
            builder.commit()?
        } else {
            db.add_entity(&type_name, attrs_ref)
        }
    };

    let op_spec = format!("type={type_name}|name={name}|id={id}");
    let next_key = if state.snapshot_key.is_empty() {
        axiograph_dsl::digest::axi_digest_v1(&op_spec)
    } else {
        chain_snapshot_key(&state.snapshot_key, "add_entity", &op_spec)
    };
    set_snapshot_key(state, next_key);
    refresh_meta_plane_index(state)?;

    println!("added entity {} ({type_name}, name={name})", id);
    Ok(())
}

fn cmd_add_fact(state: &mut ReplState, args: &[String]) -> Result<()> {
    if args.len() < 2 {
        return Err(anyhow!(
            "usage: add_fact <Schema.Relation|Schema Relation> <field=value...> [confidence <c>]"
        ));
    }

    let (schema_name, relation_name, field_tokens) = if let Some((schema, rel)) = args[0].split_once('.') {
        (schema.trim().to_string(), rel.trim().to_string(), &args[1..])
    } else {
        if args.len() < 3 {
            return Err(anyhow!(
                "usage: add_fact <Schema.Relation|Schema Relation> <field=value...> [confidence <c>]"
            ));
        }
        (args[0].trim().to_string(), args[1].trim().to_string(), &args[2..])
    };

    if schema_name.is_empty() || relation_name.is_empty() {
        return Err(anyhow!("add_fact: schema and relation must be non-empty"));
    }

    let mut confidence: f32 = 1.0;
    let mut kv_tokens: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < field_tokens.len() {
        match field_tokens[i].as_str() {
            "confidence" => {
                if i + 1 >= field_tokens.len() {
                    return Err(anyhow!("add_fact: missing value for `confidence`"));
                }
                confidence = field_tokens[i + 1].parse()?;
                i += 2;
            }
            other if other.starts_with("confidence=") => {
                let (_, v) = other.split_once('=').unwrap_or(("confidence", ""));
                confidence = v.parse()?;
                i += 1;
            }
            other => {
                kv_tokens.push(other.to_string());
                i += 1;
            }
        }
    }

    if !confidence.is_finite() || !(0.0..=1.0).contains(&confidence) {
        return Err(anyhow!(
            "add_fact: confidence must be a finite number in [0, 1] (got {confidence})"
        ));
    }

    let fields_kv = parse_kv_pairs(&kv_tokens)?;
    if fields_kv.is_empty() {
        return Err(anyhow!(
            "add_fact: expected at least one field assignment (like `child=Alice`)"
        ));
    }

    let fact_id = {
        let db = require_db_mut(state)?;

        // Resolve all entity references up front so the typed builder can borrow the DB.
        let mut resolved: Vec<(String, u32)> = Vec::with_capacity(fields_kv.len());
        for (field, token) in fields_kv {
            let id = resolve_entity_ref(db, &token)?;
            resolved.push((field, id));
        }

        let mut checked = axiograph_pathdb::CheckedDbMut::new(db)?;
        let mut builder = checked
            .fact_builder(&schema_name, &relation_name)?
            .with_edge_confidence(confidence);
        for (field, id) in resolved {
            builder.set_field(&field, id)?;
        }
        let fact_id = builder.commit()?;
        checked.db_mut().build_indexes();
        fact_id
    };

    let op_spec = format!(
        "schema={schema_name}|relation={relation_name}|fact_id={fact_id}|confidence={confidence:.3}"
    );
    let next_key = if state.snapshot_key.is_empty() {
        axiograph_dsl::digest::axi_digest_v1(&op_spec)
    } else {
        chain_snapshot_key(&state.snapshot_key, "add_fact", &op_spec)
    };
    set_snapshot_key(state, next_key);
    refresh_meta_plane_index(state)?;

    println!(
        "added fact node {fact_id} ({schema_name}.{relation_name}) (confidence={confidence:.3})"
    );
    Ok(())
}

fn cmd_add_edge(state: &mut ReplState, args: &[String]) -> Result<()> {
    if args.len() < 3 {
        return Err(anyhow!(
            "usage: add_edge <rel> <src> <dst> [confidence <c>] [k=v...]"
        ));
    }

    let rel_type = args[0].clone();
    let src_ref = args[1].as_str();
    let dst_ref = args[2].as_str();

    let mut confidence: f32 = 1.0;
    let mut kv_tokens: Vec<String> = Vec::new();

    let mut i = 3usize;
    while i < args.len() {
        match args[i].as_str() {
            "confidence" => {
                if i + 1 >= args.len() {
                    return Err(anyhow!("add_edge: missing value for `confidence`"));
                }
                confidence = args[i + 1].parse()?;
                i += 2;
            }
            other if other.starts_with("confidence=") => {
                let (_, v) = other.split_once('=').unwrap_or(("confidence", ""));
                confidence = v.parse()?;
                i += 1;
            }
            other => {
                kv_tokens.push(other.to_string());
                i += 1;
            }
        }
    }

    if !confidence.is_finite() || !(0.0..=1.0).contains(&confidence) {
        return Err(anyhow!(
            "add_edge: confidence must be a finite number in [0, 1] (got {confidence})"
        ));
    }

    let attrs = parse_kv_pairs(&kv_tokens)?;
    let attrs_ref: Vec<(&str, &str)> = attrs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let use_checked_edge_path = state
        .meta
        .as_ref()
        .is_some_and(|m| !m.schemas.is_empty());

    let (src, dst, already) = {
        let db = require_db_mut(state)?;
        let src = resolve_entity_ref(db, src_ref)?;
        let dst = resolve_entity_ref(db, dst_ref)?;
        let rel_id = db.interner.intern(&rel_type);
        let already = db.relations.has_edge(src, rel_id, dst);
        if !already {
            // Prefer the checked edge path when a meta-plane is available; fall back
            // to raw edges for untyped/evidence-only databases.
            if use_checked_edge_path {
                let mut checked = axiograph_pathdb::CheckedDbMut::new(db)?;
                checked.add_edge_checked(&rel_type, src, dst, confidence, attrs_ref)?;
            } else {
                // Minimal invariant: `axi_fact_in_context` must always target a Context.
                if rel_type == axiograph_pathdb::axi_meta::REL_AXI_FACT_IN_CONTEXT {
                    let Some(view) = db.get_entity(dst) else {
                        return Err(anyhow!("add_edge: missing target entity {dst}"));
                    };
                    if view.entity_type != "Context" && view.entity_type != "World" {
                        return Err(anyhow!(
                            "add_edge: `{}` target must be a Context/World (got `{}`)",
                            axiograph_pathdb::axi_meta::REL_AXI_FACT_IN_CONTEXT,
                            view.entity_type
                        ));
                    }
                }
                db.add_relation(&rel_type, src, dst, confidence, attrs_ref);
            }
            db.build_indexes();
        }
        (src, dst, already)
    };

    let op_spec = format!("rel={rel_type}|src={src}|dst={dst}|confidence={confidence:.3}");
    let next_key = if state.snapshot_key.is_empty() {
        axiograph_dsl::digest::axi_digest_v1(&op_spec)
    } else {
        chain_snapshot_key(&state.snapshot_key, "add_edge", &op_spec)
    };
    set_snapshot_key(state, next_key);

    if already {
        println!("edge already exists: {src} -{rel_type}-> {dst} (confidence={confidence:.3})");
    } else {
        println!("added edge: {src} -{rel_type}-> {dst} (confidence={confidence:.3})");
    }
    Ok(())
}

fn cmd_add_equiv(state: &mut ReplState, args: &[String]) -> Result<()> {
    if args.len() != 3 {
        return Err(anyhow!("usage: add_equiv <left> <right> <equiv_type>"));
    }

    let left_ref = args[0].as_str();
    let right_ref = args[1].as_str();
    let equiv_type = args[2].as_str();

    let (left, right) = {
        let db = require_db_mut(state)?;
        let left = resolve_entity_ref(db, left_ref)?;
        let right = resolve_entity_ref(db, right_ref)?;
        db.add_equivalence(left, right, equiv_type);
        (left, right)
    };

    let op_spec = format!("left={left}|right={right}|type={equiv_type}");
    let next_key = if state.snapshot_key.is_empty() {
        axiograph_dsl::digest::axi_digest_v1(&op_spec)
    } else {
        chain_snapshot_key(&state.snapshot_key, "add_equiv", &op_spec)
    };
    set_snapshot_key(state, next_key);

    println!("added equiv: {left} ≃ {right} ({equiv_type})");
    Ok(())
}

fn cmd_schema(state: &ReplState, args: &[String]) -> Result<()> {
    if args.len() > 1 {
        return Err(anyhow!("usage: schema [name]"));
    }

    let db = require_db(state)?;

    if args.is_empty() {
        // List all imported modules.
        let Some(mods) = db.find_by_type(axiograph_pathdb::axi_meta::META_TYPE_MODULE) else {
            println!("(no `.axi` meta-plane modules loaded)");
            return Ok(());
        };

        let mut module_ids: Vec<u32> = mods.iter().collect();
        module_ids.sort();

        for mid in module_ids {
            let Some(mv) = db.get_entity(mid) else {
                continue;
            };
            let Some(mname) = mv.attrs.get("name") else {
                continue;
            };

            let schemas = db.follow_one(mid, axiograph_pathdb::axi_meta::META_REL_HAS_SCHEMA);
            println!("module {mname} (schemas={})", schemas.len());
            for sid in schemas.iter() {
                let Some(sv) = db.get_entity(sid) else {
                    continue;
                };
                let sname = sv
                    .attrs
                    .get("name")
                    .cloned()
                    .unwrap_or_else(|| "(unnamed)".to_string());
                let objects =
                    db.follow_one(sid, axiograph_pathdb::axi_meta::META_REL_SCHEMA_HAS_OBJECT);
                let relations = db.follow_one(
                    sid,
                    axiograph_pathdb::axi_meta::META_REL_SCHEMA_HAS_RELATION,
                );
                let theories =
                    db.follow_one(sid, axiograph_pathdb::axi_meta::META_REL_SCHEMA_HAS_THEORY);
                println!(
                    "  schema {sname} (objects={} relations={} theories={})",
                    objects.len(),
                    relations.len(),
                    theories.len()
                );
            }
        }
        return Ok(());
    }

    let name = &args[0];

    // If it matches a module name, show module details.
    if let Some(mid) =
        find_entity_by_type_and_name(db, axiograph_pathdb::axi_meta::META_TYPE_MODULE, name)
    {
        let schemas = db.follow_one(mid, axiograph_pathdb::axi_meta::META_REL_HAS_SCHEMA);
        println!("module {name}");
        for sid in schemas.iter() {
            let sname = db
                .get_entity(sid)
                .and_then(|v| v.attrs.get("name").cloned())
                .unwrap_or_else(|| "(unnamed)".to_string());
            println!("  schema {sname}");
        }
        return Ok(());
    }

    // Otherwise treat it as a schema name.
    let schema_ids =
        find_entities_by_type_and_name(db, axiograph_pathdb::axi_meta::META_TYPE_SCHEMA, name);
    if schema_ids.is_empty() {
        println!("(no meta schema named `{name}` found)");
        return Ok(());
    }
    if schema_ids.len() > 1 {
        println!("multiple schemas named `{name}` found:");
        for sid in schema_ids {
            let mv = db
                .get_entity(sid)
                .and_then(|v| v.attrs.get("axi_module").cloned())
                .unwrap_or_else(|| "?".to_string());
            println!("  schema {name} (axi_module={mv})");
        }
        return Ok(());
    }

    let sid = schema_ids[0];
    println!("schema {name}");

    let subtype_ids = db.follow_one(sid, axiograph_pathdb::axi_meta::META_REL_SCHEMA_HAS_SUBTYPE);
    if !subtype_ids.is_empty() {
        println!("  subtypes:");
        for st in subtype_ids.iter() {
            let v = db.get_entity(st);
            let sub = v
                .as_ref()
                .and_then(|x| x.attrs.get("axi_sub").cloned())
                .unwrap_or_else(|| "?".to_string());
            let sup = v
                .as_ref()
                .and_then(|x| x.attrs.get("axi_sup").cloned())
                .unwrap_or_else(|| "?".to_string());
            println!("    {sub} < {sup}");
        }
    }

    let objects = db.follow_one(sid, axiograph_pathdb::axi_meta::META_REL_SCHEMA_HAS_OBJECT);
    if !objects.is_empty() {
        println!("  objects:");
        for oid in objects.iter() {
            let obj = db
                .get_entity(oid)
                .and_then(|v| v.attrs.get("name").cloned())
                .unwrap_or_else(|| "(unnamed)".to_string());
            println!("    {obj}");
        }
    }

    let relations = db.follow_one(
        sid,
        axiograph_pathdb::axi_meta::META_REL_SCHEMA_HAS_RELATION,
    );
    if !relations.is_empty() {
        println!("  relations:");
        for rid in relations.iter() {
            let rname = db
                .get_entity(rid)
                .and_then(|v| v.attrs.get("name").cloned())
                .unwrap_or_else(|| "(unnamed)".to_string());
            println!("    {rname}");

            let fields =
                db.follow_one(rid, axiograph_pathdb::axi_meta::META_REL_RELATION_HAS_FIELD);
            for fid in fields.iter() {
                let fv = db.get_entity(fid);
                let field = fv
                    .as_ref()
                    .and_then(|v| v.attrs.get("axi_field").cloned())
                    .unwrap_or_else(|| "?".to_string());
                let ty = fv
                    .as_ref()
                    .and_then(|v| v.attrs.get("axi_field_type").cloned())
                    .unwrap_or_else(|| "?".to_string());
                println!("      {field}: {ty}");
            }
        }
    }

    Ok(())
}

fn cmd_validate_axi(state: &ReplState, args: &[String]) -> Result<()> {
    if !args.is_empty() {
        return Err(anyhow!("usage: validate_axi"));
    }

    let db = require_db(state)?;

    let meta = match state.meta.as_ref() {
        Some(m) => m.clone(),
        None => axiograph_pathdb::axi_semantics::MetaPlaneIndex::from_db(db)?,
    };
    if meta.schemas.is_empty() {
        println!("(no `.axi` meta-plane schemas loaded)");
        return Ok(());
    }

    let report = meta.typecheck_axi_facts(db);
    if report.ok() {
        println!("axi typecheck ok (facts={} errors=0)", report.checked_facts);
        return Ok(());
    }

    println!(
        "axi typecheck FAILED (facts={} errors={})",
        report.checked_facts,
        report.errors.len()
    );
    for e in report.errors {
        println!("  - {e}");
    }
    Ok(())
}

fn cmd_learning_graph(state: &ReplState, args: &[String]) -> Result<()> {
    if args.len() != 1 {
        return Err(anyhow!("usage: learning_graph <schema>"));
    }

    let schema = args[0].as_str();
    let db = require_db(state)?;
    let g = axiograph_pathdb::learning::extract_learning_graph(db, schema)?;

    println!(
        "learning_graph schema={} concepts={} requires={} explains={} demonstrates={} concept_descriptions={}",
        g.schema,
        g.concepts.len(),
        g.requires.len(),
        g.explains.len(),
        g.demonstrates.len(),
        g.concept_descriptions.len()
    );

    fn name_of(db: &axiograph_pathdb::PathDB, entity: u32) -> String {
        db.get_entity(entity)
            .and_then(|v| v.attrs.get("name").cloned())
            .unwrap_or_else(|| entity.to_string())
    }

    let limit = 20usize;
    if !g.requires.is_empty() {
        println!("requires (first {}):", limit);
        for e in g.requires.iter().take(limit) {
            let from_id = e
                .from
                .entity_id(db)
                .unwrap_or_else(|_| e.from.raw_entity_id());
            let to_id = e.to.entity_id(db).unwrap_or_else(|_| e.to.raw_entity_id());
            println!("  - {} -> {}", name_of(db, from_id), name_of(db, to_id));
        }
    }

    Ok(())
}

fn find_entity_by_type_and_name(
    db: &axiograph_pathdb::PathDB,
    type_name: &str,
    name: &str,
) -> Option<u32> {
    find_entities_by_type_and_name(db, type_name, name)
        .into_iter()
        .next()
}

fn find_entities_by_type_and_name(
    db: &axiograph_pathdb::PathDB,
    type_name: &str,
    name: &str,
) -> Vec<u32> {
    let Some(type_bitmap) = db.find_by_type(type_name) else {
        return Vec::new();
    };
    let Some(key_id) = db.interner.id_of("name") else {
        return Vec::new();
    };
    let Some(value_id) = db.interner.id_of(name) else {
        return Vec::new();
    };

    let mut candidates = type_bitmap.clone();
    candidates &= db.entities.entities_with_attr_value(key_id, value_id);
    candidates.iter().collect()
}

fn cmd_show(state: &ReplState, args: &[String]) -> Result<()> {
    if args.len() != 1 {
        return Err(anyhow!("usage: show <entity_id|name>"));
    }
    let db = require_db(state)?;
    let entity_id = resolve_entity_ref(db, &args[0])?;
    let Some(view) = db.get_entity(entity_id) else {
        println!("(missing entity {entity_id})");
        return Ok(());
    };
    println!("Entity {entity_id} : {}", view.entity_type);
    for (k, v) in view.attrs {
        println!("  {k} = {v}");
    }
    Ok(())
}

fn entity_plane(view: &axiograph_pathdb::EntityView) -> &'static str {
    if view.entity_type.starts_with("AxiMeta") {
        return "meta";
    }
    if view.attrs.contains_key("axi_fact_id")
        || view.attrs.contains_key("axi_module")
        || view.attrs.contains_key("axi_schema")
        || view.attrs.contains_key("axi_dialect")
    {
        return "accepted";
    }
    if view.entity_type == "DocChunk"
        || view.entity_type == "Document"
        || view.entity_type == "ProposalRun"
        || view.attrs.contains_key("proposal_id")
        || view.attrs.contains_key("proposals_digest")
    {
        return "evidence";
    }
    "data"
}

fn is_fact_node(view: &axiograph_pathdb::EntityView) -> bool {
    view.attrs.contains_key(axiograph_pathdb::axi_meta::ATTR_AXI_RELATION)
}

fn truncate_value(s: &str, max_chars: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out = String::new();
    out.extend(s.chars().take(max_chars));
    out.push('…');
    out
}

fn cmd_describe(state: &ReplState, args: &[String]) -> Result<()> {
    if args.is_empty() {
        return Err(anyhow!(
            "usage: describe <entity_id|name> [--in N] [--out N] [--attrs N] [--max_rels N]"
        ));
    }

    let mut in_limit: usize = 6;
    let mut out_limit: usize = 6;
    let mut max_attrs: usize = 64;
    let mut max_rels: usize = 18;
    let mut max_value_chars: usize = 400;

    let mut idx = 0usize;
    let mut entity_token: Option<String> = None;
    while idx < args.len() {
        match args[idx].as_str() {
            "--in" => {
                if idx + 1 >= args.len() {
                    return Err(anyhow!("describe: missing value for --in"));
                }
                in_limit = args[idx + 1].parse()?;
                idx += 2;
            }
            "--out" => {
                if idx + 1 >= args.len() {
                    return Err(anyhow!("describe: missing value for --out"));
                }
                out_limit = args[idx + 1].parse()?;
                idx += 2;
            }
            "--attrs" => {
                if idx + 1 >= args.len() {
                    return Err(anyhow!("describe: missing value for --attrs"));
                }
                max_attrs = args[idx + 1].parse()?;
                idx += 2;
            }
            "--max_rels" | "--rels" => {
                if idx + 1 >= args.len() {
                    return Err(anyhow!("describe: missing value for --max_rels"));
                }
                max_rels = args[idx + 1].parse()?;
                idx += 2;
            }
            "--max_value_chars" => {
                if idx + 1 >= args.len() {
                    return Err(anyhow!("describe: missing value for --max_value_chars"));
                }
                max_value_chars = args[idx + 1].parse()?;
                idx += 2;
            }
            tok if entity_token.is_none() => {
                entity_token = Some(tok.to_string());
                idx += 1;
            }
            other => return Err(anyhow!("describe: unexpected token `{other}`")),
        }
    }

    let token = entity_token.ok_or_else(|| anyhow!("describe: missing entity argument"))?;
    let db = require_db(state)?;
    let entity_id = resolve_entity_ref(db, &token)?;
    let Some(view) = db.get_entity(entity_id) else {
        println!("(missing entity {entity_id})");
        return Ok(());
    };

    println!(
        "{}",
        format!(
            "{}  plane={}{}",
            describe_entity(db, entity_id),
            entity_plane(&view),
            if is_fact_node(&view) { " fact" } else { "" }
        )
        .bold()
    );

    // Attributes
    if !view.attrs.is_empty() {
        println!("attrs ({}):", view.attrs.len());
        let mut keys: Vec<String> = view.attrs.keys().cloned().collect();
        keys.sort();
        for (i, k) in keys.iter().enumerate() {
            if i >= max_attrs {
                println!("  …");
                break;
            }
            let v = view.attrs.get(k).expect("key present");
            println!("  {k} = {}", truncate_value(v, max_value_chars));
        }
    }

    // Context scoping
    let ctxs = db.follow_one(entity_id, axiograph_pathdb::axi_meta::REL_AXI_FACT_IN_CONTEXT);
    if !ctxs.is_empty() {
        println!("contexts ({}):", ctxs.len());
        for id in ctxs.iter().take(12) {
            println!("  - {}", describe_entity(db, id));
        }
        if ctxs.len() > 12 {
            println!("  …");
        }
    }

    // Equivalences
    if let Some(eqs) = db.equivalences.get(&entity_id) {
        if !eqs.is_empty() {
            println!("equivalences ({}):", eqs.len());
            for (other, ty_id) in eqs.iter().take(12) {
                let ty = db.interner.lookup(*ty_id).unwrap_or_else(|| "?".to_string());
                println!("  - {}  ({ty})", describe_entity(db, *other));
            }
            if eqs.len() > 12 {
                println!("  …");
            }
        }
    }

    // Evidence links (if present)
    for rel in ["has_evidence_chunk", "doc_chunk_about", "has_doc_chunk"] {
        let ids = db.follow_one(entity_id, rel);
        if ids.is_empty() {
            continue;
        }
        println!("{rel} ({}):", ids.len());
        for id in ids.iter().take(12) {
            println!("  - {}", describe_entity(db, id));
        }
        if ids.len() > 12 {
            println!("  …");
        }
    }

    // Outgoing/incoming relations grouped by label.
    fn group_edges(
        db: &axiograph_pathdb::PathDB,
        rels: Vec<&axiograph_pathdb::Relation>,
        dir: &'static str,
        max_rels: usize,
        per_rel: usize,
        entity_id: u32,
    ) {
        use std::collections::HashMap;

        let mut groups: HashMap<String, Vec<(u32, f32)>> = HashMap::new();
        for r in rels {
            let label = db.interner.lookup(r.rel_type).unwrap_or_else(|| "?".to_string());
            let endpoint = if dir == "out" { r.target } else { r.source };
            groups.entry(label).or_default().push((endpoint, r.confidence));
        }

        let mut keys: Vec<String> = groups.keys().cloned().collect();
        keys.sort_by_key(|k| std::cmp::Reverse(groups.get(k).map(|v| v.len()).unwrap_or(0)));

        println!("{dir} edges ({} rel types):", keys.len());
        for (i, k) in keys.iter().enumerate() {
            if i >= max_rels {
                println!("  …");
                break;
            }
            let mut edges = groups.remove(k).unwrap_or_default();
            edges.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            print!("  - {k} ({}):", edges.len());
            if edges.is_empty() {
                println!();
                continue;
            }
            println!();
            for (j, (id, conf)) in edges.iter().take(per_rel).enumerate() {
                let prefix = if j == 0 { "    " } else { "    " };
                if *id == entity_id {
                    println!("{prefix}{} (confidence={conf:.3})", describe_entity(db, *id));
                } else {
                    println!("{prefix}{} (confidence={conf:.3})", describe_entity(db, *id));
                }
            }
            if edges.len() > per_rel {
                println!("    …");
            }
        }
    }

    let outgoing = db.relations.outgoing_any(entity_id);
    if !outgoing.is_empty() {
        group_edges(db, outgoing, "out", max_rels, out_limit, entity_id);
    }
    let incoming = db.relations.incoming_any(entity_id);
    if !incoming.is_empty() {
        group_edges(db, incoming, "in", max_rels, in_limit, entity_id);
    }

    Ok(())
}

fn cmd_neigh(state: &ReplState, args: &[String]) -> Result<()> {
    if args.is_empty() {
        return Err(anyhow!(
            "usage: neigh <entity_id|name> [--hops N] [--plane data|meta|both] [--out <path>] [--format dot|html|json] [--typed_overlay] [--max_nodes N] [--max_edges N] [--direction out|in|both]"
        ));
    }

    let mut entity_token: Option<String> = None;
    let mut hops: usize = 2;
    let mut plane: String = "data".to_string();
    let mut out: Option<PathBuf> = None;
    let mut format: String = "html".to_string();
    let mut typed_overlay = true;
    let mut max_nodes: usize = 250;
    let mut max_edges: usize = 4_000;
    let mut direction: String = "both".to_string();
    let mut include_equivalences = true;

    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--hops" | "hops" => {
                if idx + 1 >= args.len() {
                    return Err(anyhow!("neigh: missing value for hops"));
                }
                hops = args[idx + 1].parse()?;
                idx += 2;
            }
            "--plane" | "plane" => {
                if idx + 1 >= args.len() {
                    return Err(anyhow!("neigh: missing value for plane"));
                }
                plane = args[idx + 1].clone();
                idx += 2;
            }
            "--out" | "out" => {
                if idx + 1 >= args.len() {
                    return Err(anyhow!("neigh: missing value for out"));
                }
                out = Some(PathBuf::from(&args[idx + 1]));
                idx += 2;
            }
            "--format" | "format" => {
                if idx + 1 >= args.len() {
                    return Err(anyhow!("neigh: missing value for format"));
                }
                format = args[idx + 1].clone();
                idx += 2;
            }
            "--typed_overlay" | "typed_overlay" => {
                typed_overlay = true;
                idx += 1;
            }
            "--no_typed_overlay" => {
                typed_overlay = false;
                idx += 1;
            }
            "--max_nodes" | "max_nodes" => {
                if idx + 1 >= args.len() {
                    return Err(anyhow!("neigh: missing value for max_nodes"));
                }
                max_nodes = args[idx + 1].parse()?;
                idx += 2;
            }
            "--max_edges" | "max_edges" => {
                if idx + 1 >= args.len() {
                    return Err(anyhow!("neigh: missing value for max_edges"));
                }
                max_edges = args[idx + 1].parse()?;
                idx += 2;
            }
            "--direction" | "direction" => {
                if idx + 1 >= args.len() {
                    return Err(anyhow!("neigh: missing value for direction"));
                }
                direction = args[idx + 1].clone();
                idx += 2;
            }
            "--no_equivalences" => {
                include_equivalences = false;
                idx += 1;
            }
            tok if entity_token.is_none() => {
                entity_token = Some(tok.to_string());
                idx += 1;
            }
            other => return Err(anyhow!("neigh: unexpected token `{other}`")),
        }
    }

    let token = entity_token.ok_or_else(|| anyhow!("neigh: missing entity argument"))?;
    let db = require_db(state)?;
    let focus_id = resolve_entity_ref(db, &token)?;

    let plane = plane.trim().to_ascii_lowercase();
    let (include_meta_plane, include_data_plane) = match plane.as_str() {
        "data" => (false, true),
        "meta" => (true, false),
        "both" => (true, true),
        other => {
            return Err(anyhow!(
                "neigh: unknown plane `{other}` (expected data|meta|both)"
            ))
        }
    };

    let options = crate::viz::VizOptions {
        focus_ids: vec![focus_id],
        hops,
        max_nodes,
        max_edges,
        direction: crate::viz::VizDirection::parse(&direction)?,
        include_meta_plane,
        include_data_plane,
        include_equivalences,
        typed_overlay,
    };

    let g = if typed_overlay && state.meta.is_none() {
        crate::viz::extract_viz_graph(db, &options)?
    } else {
        crate::viz::extract_viz_graph_with_meta(db, &options, state.meta.as_ref())?
    };

    let mut kinds: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for n in &g.nodes {
        *kinds.entry(n.kind.clone()).or_default() += 1;
    }

    println!(
        "neigh {}: nodes={} edges={} truncated={} kinds={:?}",
        describe_entity(db, focus_id),
        g.nodes.len(),
        g.edges.len(),
        g.truncated,
        kinds
    );

    if let Some(out) = out.as_ref() {
        let rendered = match crate::viz::VizFormat::parse(&format)? {
            crate::viz::VizFormat::Dot => crate::viz::render_dot(db, &g),
            crate::viz::VizFormat::Json => crate::viz::render_json(&g)?,
            crate::viz::VizFormat::Html => crate::viz::render_html(db, &g)?,
        };
        fs::write(out, rendered)?;
        println!("wrote {}", out.display());
    }

    Ok(())
}

fn cmd_open(state: &mut ReplState, args: &[String]) -> Result<()> {
    if args.len() < 2 {
        return Err(anyhow!(
            "usage: open chunk <DocChunk> [--max_chars N]\n\
             usage: open doc <Document> [--max_chunks N]\n\
             usage: open evidence <entity>\n\
             usage: open entity <entity>"
        ));
    }

    let mut max_chars: usize = 3_000;
    let mut max_chunks: usize = 8;

    let sub = args[0].as_str();
    let target = args[1].as_str();

    let mut i = 2usize;
    while i < args.len() {
        match args[i].as_str() {
            "--max_chars" => {
                if i + 1 >= args.len() {
                    return Err(anyhow!("open: missing value for --max_chars"));
                }
                max_chars = args[i + 1].parse()?;
                i += 2;
            }
            "--max_chunks" => {
                if i + 1 >= args.len() {
                    return Err(anyhow!("open: missing value for --max_chunks"));
                }
                max_chunks = args[i + 1].parse()?;
                i += 2;
            }
            other => return Err(anyhow!("open: unexpected token `{other}`")),
        }
    }

    let db = require_db(state)?;
    let id = resolve_entity_ref(db, target)?;
    let Some(view) = db.get_entity(id) else {
        return Err(anyhow!("missing entity {id}"));
    };

    match sub {
        "chunk" => {
            if view.entity_type != "DocChunk" {
                println!(
                    "warning: {} is {}, not DocChunk",
                    describe_entity(db, id),
                    view.entity_type
                );
            }
            println!("{}", format!("DocChunk {}", describe_entity(db, id)).bold());
            if let Some(doc) = view.attrs.get("document_id") {
                println!("document_id: {doc}");
            }
            if let Some(span) = view.attrs.get("span_id") {
                println!("span_id: {span}");
            }
            if let Some(page) = view.attrs.get("page") {
                println!("page: {page}");
            }
            if let Some(text) = view.attrs.get("text") {
                println!("\n{}", truncate_value(text, max_chars));
            }
            Ok(())
        }
        "doc" | "document" => {
            if view.entity_type != "Document" {
                println!(
                    "warning: {} is {}, not Document",
                    describe_entity(db, id),
                    view.entity_type
                );
            }
            println!("{}", format!("Document {}", describe_entity(db, id)).bold());
            let chunks = db.follow_one(id, "document_has_chunk");
            println!("chunks: {}", chunks.len());
            for cid in chunks.iter().take(max_chunks) {
                println!("  - {}", describe_entity(db, cid));
            }
            if chunks.len() > (max_chunks as u64) {
                println!("  …");
            }
            Ok(())
        }
        "evidence" => {
            println!("{}", format!("Evidence {}", describe_entity(db, id)).bold());
            // Evidence links
            let chunks = db.follow_one(id, "has_evidence_chunk");
            if !chunks.is_empty() {
                println!("evidence chunks ({}):", chunks.len());
                for cid in chunks.iter().take(max_chunks) {
                    println!("  - {}", describe_entity(db, cid));
                }
                if chunks.len() > (max_chunks as u64) {
                    println!("  …");
                }
            }
            cmd_describe(state, &[target.to_string()])?;
            Ok(())
        }
        "entity" => cmd_describe(state, &[target.to_string()]),
        other => Err(anyhow!(
            "unknown open subcommand `{other}` (expected chunk|doc|evidence|entity)"
        )),
    }
}

fn cmd_diff(state: &ReplState, args: &[String]) -> Result<()> {
    if args.is_empty() {
        return Err(anyhow!("usage: diff ctx <c1> <c2> [rel <RelName>] [limit N]"));
    }
    match args[0].as_str() {
        "ctx" => cmd_diff_ctx(state, &args[1..]),
        other => Err(anyhow!("unknown diff subcommand `{other}` (try: diff ctx ...)")),
    }
}

fn cmd_diff_ctx(state: &ReplState, args: &[String]) -> Result<()> {
    if args.len() < 2 {
        return Err(anyhow!("usage: diff ctx <c1> <c2> [rel <RelName>] [limit N]"));
    }
    let db = require_db(state)?;

    let c1 = resolve_entity_ref(db, &args[0])?;
    let c2 = resolve_entity_ref(db, &args[1])?;

    let mut rel_filter: Option<String> = None;
    let mut limit: usize = 25;

    let mut i = 2usize;
    while i < args.len() {
        match args[i].as_str() {
            "rel" | "--rel" => {
                if i + 1 >= args.len() {
                    return Err(anyhow!("diff ctx: missing value for rel"));
                }
                rel_filter = Some(args[i + 1].clone());
                i += 2;
            }
            "limit" | "--limit" => {
                if i + 1 >= args.len() {
                    return Err(anyhow!("diff ctx: missing value for limit"));
                }
                limit = args[i + 1].parse()?;
                i += 2;
            }
            other => return Err(anyhow!("diff ctx: unexpected token `{other}`")),
        }
    }

    let mut a = db.fact_nodes_by_context(c1);
    let mut b = db.fact_nodes_by_context(c2);

    if let Some(rel) = rel_filter.as_deref() {
        let rel_facts = db.fact_nodes_by_axi_relation(rel);
        a &= rel_facts.clone();
        b &= rel_facts;
    }

    let only_a = &a - &b;
    let only_b = &b - &a;
    let both = &a & &b;

    println!(
        "diff ctx {} vs {}{}:",
        describe_entity(db, c1),
        describe_entity(db, c2),
        rel_filter
            .as_ref()
            .map(|r| format!(" (rel={r})"))
            .unwrap_or_default()
    );
    println!(
        "  facts: a={} b={} both={} only_a={} only_b={}",
        a.len(),
        b.len(),
        both.len(),
        only_a.len(),
        only_b.len()
    );

    fn print_facts(db: &axiograph_pathdb::PathDB, label: &str, facts: &RoaringBitmap, limit: usize) {
        if facts.is_empty() {
            return;
        }
        println!("{label} ({}):", facts.len());
        for id in facts.iter().take(limit) {
            let desc = describe_entity(db, id);
            let rel = db
                .get_entity(id)
                .and_then(|v| v.attrs.get(axiograph_pathdb::axi_meta::ATTR_AXI_RELATION).cloned())
                .unwrap_or_else(|| "?".to_string());
            println!("  - {desc}  (axi_relation={rel})");
        }
        if facts.len() as usize > limit {
            println!("  …");
        }
    }

    print_facts(db, "only in a", &only_a, limit);
    print_facts(db, "only in b", &only_b, limit);

    Ok(())
}

fn cmd_find_by_type(state: &ReplState, args: &[String]) -> Result<()> {
    if args.len() != 1 {
        return Err(anyhow!("usage: find_by_type <type_name>"));
    }
    let type_name = &args[0];
    let db = require_db(state)?;
    let Some(ids) = db.find_by_type(type_name) else {
        println!("(no entities of type {type_name})");
        return Ok(());
    };
    println!("count={}", ids.len());
    let limit = 20usize;
    let mut shown = 0usize;
    for id in ids.iter().take(limit) {
        println!("  {id}");
        shown += 1;
    }
    if (ids.len() as usize) > shown {
        println!("  ...");
    }
    Ok(())
}

fn cmd_follow(state: &ReplState, args: &[String]) -> Result<()> {
    if args.len() < 2 {
        return Err(anyhow!(
            "usage: follow <start_id> <rel...> | follow <start_id> <path_expr> [max_hops N]"
        ));
    }
    let start_id: u32 = args[0].parse()?;
    let mut tokens: Vec<String> = args[1..].to_vec();

    // Optional: `max_hops N` (applies to RPQ-style path expressions).
    let mut max_hops: Option<u32> = None;
    if let Some(i) = tokens
        .iter()
        .position(|t| t.eq_ignore_ascii_case("max_hops"))
    {
        if i + 1 >= tokens.len() {
            return Err(anyhow!("usage: follow <start_id> <path_expr> max_hops <N>"));
        }
        max_hops = Some(tokens[i + 1].parse()?);
        tokens.drain(i..=i + 1);
    } else if let Some(i) = tokens.iter().position(|t| t.eq_ignore_ascii_case("max")) {
        if i + 2 >= tokens.len() || !tokens[i + 1].eq_ignore_ascii_case("hops") {
            return Err(anyhow!("usage: follow <start_id> <path_expr> max hops <N>"));
        }
        max_hops = Some(tokens[i + 2].parse()?);
        tokens.drain(i..=i + 2);
    } else if let Some(i) = tokens.iter().position(|t| t.eq_ignore_ascii_case("within")) {
        if i + 2 >= tokens.len() || !tokens[i + 2].eq_ignore_ascii_case("hops") {
            return Err(anyhow!(
                "usage: follow <start_id> <path_expr> within <N> hops"
            ));
        }
        max_hops = Some(tokens[i + 1].parse()?);
        tokens.drain(i..=i + 2);
    }

    let db = require_db(state)?;
    let start = Instant::now();
    let rpq_like: Vec<String> = tokens
        .iter()
        .filter_map(|t| {
            let lower = t.to_ascii_lowercase();
            match lower.as_str() {
                "," => None,
                "then" | "next" | "to" => Some("/".to_string()),
                "or" => Some("|".to_string()),
                "->" | "→" => Some("/".to_string()),
                _ => Some(t.to_string()),
            }
        })
        .collect();

    let mut chain_tokens: Vec<String> = Vec::new();
    for t in &tokens {
        let lower = t.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "," | "then"
                | "next"
                | "to"
                | "or"
                | "->"
                | "→"
                | "/"
                | "|"
                | "("
                | ")"
                | "*"
                | "+"
                | "?"
        ) {
            continue;
        }
        chain_tokens.push(t.to_string());
    }

    let targets = match crate::axql::parse_axql_path_expr(&rpq_like.join(" ")) {
        Ok(expr) => crate::axql::follow_path_expr(db, start_id, &expr, max_hops)?,
        Err(_) => match crate::axql::parse_axql_path_expr(&chain_tokens.join("/")) {
            Ok(expr) => crate::axql::follow_path_expr(db, start_id, &expr, max_hops)?,
            Err(_) => {
                // Back-compat: treat args as a raw list of relation names.
                let path: Vec<&str> = tokens.iter().map(|s| s.as_str()).collect();
                db.follow_path(start_id, &path)
            }
        },
    };
    let dt = start.elapsed();

    println!("count={} ({:?})", targets.len(), dt);
    let limit = 20usize;
    let mut shown = 0usize;
    for id in targets.iter().take(limit) {
        println!("  {id}");
        shown += 1;
    }
    if (targets.len() as usize) > shown {
        println!("  ...");
    }
    Ok(())
}

fn cmd_find_paths(state: &ReplState, args: &[String]) -> Result<()> {
    if args.len() != 3 {
        return Err(anyhow!("usage: find_paths <from_id> <to_id> <max_depth>"));
    }
    let from_id: u32 = args[0].parse()?;
    let to_id: u32 = args[1].parse()?;
    let max_depth: usize = args[2].parse()?;

    let db = require_db(state)?;
    let start = Instant::now();
    let paths = db.find_paths(from_id, to_id, max_depth);
    let dt = start.elapsed();

    println!("paths={} ({:?})", paths.len(), dt);
    let limit = 10usize;
    for (i, path) in paths.iter().take(limit).enumerate() {
        let names: Vec<String> = path
            .iter()
            .filter_map(|sid| db.interner.lookup(*sid))
            .collect();
        println!("  {i}: {}", names.join(" -> "));
    }
    if paths.len() > limit {
        println!("  ...");
    }
    Ok(())
}

fn cmd_gen(state: &mut ReplState, args: &[String]) -> Result<()> {
    if args.is_empty() {
        return Err(anyhow!(
            "usage: gen <entities> <edges> <types> [index_depth] [seed]\n       gen scenario <name> [scale] [index_depth] [seed]"
        ));
    }

    // Back-compat: the old numeric generator.
    if let Ok(entities) = args[0].parse::<usize>() {
        if args.len() < 3 || args.len() > 5 {
            return Err(anyhow!(
                "usage: gen <entities> <edges> <types> [index_depth] [seed]"
            ));
        }
        let edges_per_entity: usize = args[1].parse()?;
        let rel_types: usize = args[2].parse()?;
        let index_depth: usize = args.get(3).map(|s| s.parse()).transpose()?.unwrap_or(3);
        let seed: u64 = args.get(4).map(|s| s.parse()).transpose()?.unwrap_or(1);

        let ingest = crate::synthetic_pathdb::build_synthetic_pathdb_ingest(
            entities,
            edges_per_entity,
            rel_types,
            index_depth,
            seed,
        )?;
        let mut db = ingest.db;

        let start = Instant::now();
        db.build_indexes();
        let index_time = start.elapsed();

        println!(
            "generated: entities={} edges={} rel_types={} (entity={:?} relations={:?} index={:?})",
            entities,
            ingest.edge_count,
            rel_types,
            ingest.entity_time,
            ingest.relation_time,
            index_time
        );

        let gen_spec = format!(
            "synthetic:entities={entities}|edges_per_entity={edges_per_entity}|rel_types={rel_types}|index_depth={index_depth}|seed={seed}"
        );
        let gen_key = axiograph_dsl::digest::axi_digest_v1(&gen_spec);
        state.db = Some(db);
        set_snapshot_key(state, gen_key);
        refresh_meta_plane_index(state)?;
        return Ok(());
    }

    // Scenario generator (richer types/relations/homotopies).
    let (scenario, pos) = if args[0].eq_ignore_ascii_case("scenario") {
        if args.len() < 2 {
            return Err(anyhow!(
                "usage: gen scenario <name> [scale] [index_depth] [seed]"
            ));
        }
        (args[1].as_str(), 2usize)
    } else {
        (args[0].as_str(), 1usize)
    };

    let scale: usize = args.get(pos).map(|s| s.parse()).transpose()?.unwrap_or(3);
    let index_depth: usize = args
        .get(pos + 1)
        .map(|s| s.parse())
        .transpose()?
        .unwrap_or(3);
    let seed: u64 = args
        .get(pos + 2)
        .map(|s| s.parse())
        .transpose()?
        .unwrap_or(1);

    let ingest =
        crate::synthetic_pathdb::build_scenario_pathdb_ingest(scenario, scale, index_depth, seed)?;
    let mut db = ingest.db;

    let start = Instant::now();
    db.build_indexes();
    let index_time = start.elapsed();

    println!(
        "generated scenario: {} (scale={} index_depth={} seed={})",
        ingest.scenario_name, scale, index_depth, seed
    );
    println!("  {}", ingest.description);
    println!(
        "  entities={} relations={} equivalence_keys={} (entity={:?} relations={:?} index={:?})",
        db.entities.len(),
        db.relations.len(),
        db.equivalences.len(),
        ingest.entity_time,
        ingest.relation_time,
        index_time
    );

    if !ingest.example_commands.is_empty() {
        println!("try:");
        for cmd in ingest.example_commands.iter().take(12) {
            println!("  {cmd}");
        }
        if ingest.example_commands.len() > 12 {
            println!("  ...");
        }
    }

    let gen_spec = format!(
        "scenario:{}:scale={scale}|index_depth={index_depth}|seed={seed}",
        ingest.scenario_name
    );
    let gen_key = axiograph_dsl::digest::axi_digest_v1(&gen_spec);
    state.db = Some(db);
    set_snapshot_key(state, gen_key);
    refresh_meta_plane_index(state)?;
    Ok(())
}

fn cmd_axql(state: &mut ReplState, args: &[String]) -> Result<()> {
    if args.is_empty() {
        return Err(anyhow!("usage: q [--elaborate|--typecheck] <AxQL query>"));
    }
    let mut show_elaboration = false;
    let mut typecheck_only = false;

    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--elaborate" | "--elab" | "--explain" => {
                show_elaboration = true;
                idx += 1;
            }
            "--typecheck" => {
                show_elaboration = true;
                typecheck_only = true;
                idx += 1;
            }
            _ => break,
        }
    }

    if idx >= args.len() {
        return Err(anyhow!("usage: q [--elaborate|--typecheck] <AxQL query>"));
    }

    let query_text = args[idx..].join(" ");
    let db = state
        .db
        .as_ref()
        .ok_or_else(|| anyhow!("no database loaded (use `load`, `import_axi`, or `gen`)"))?;
    let meta = state.meta.as_ref();

    let mut query = crate::axql::parse_axql_query(&query_text)?;
    if query.contexts.is_empty() && !state.contexts.is_empty() {
        query.contexts = state.contexts.clone();
    }
    let key = crate::axql::axql_query_cache_key(&state.snapshot_key, &query);

    let start = Instant::now();
    let mut cache_hit = false;
    let result = if let Some(prepared) = state.query_cache.get_mut(&key) {
        cache_hit = true;
        if show_elaboration {
            let report = prepared.elaboration_report();
            println!("elaborated: {}", prepared.elaborated_query_text());
            if !report.inferred_types.is_empty() {
                println!("inferred types:");
                for (var, tys) in &report.inferred_types {
                    println!("  {var}: {}", tys.join(", "));
                }
            }
            if !report.notes.is_empty() {
                println!("notes:");
                for note in &report.notes {
                    println!("  - {note}");
                }
            }
            let plan_lines = prepared.explain_plan_lines();
            if !plan_lines.is_empty() {
                println!("plan:");
                for l in plan_lines {
                    println!("  {l}");
                }
            }
            if typecheck_only {
                return Ok(());
            }
        }
        prepared.execute(db, meta)?
    } else {
        let prepared = crate::axql::prepare_axql_query_with_meta(db, &query, meta)?;
        state.query_cache.insert(key.clone(), prepared);
        let prepared = state.query_cache.get_mut(&key).expect("query cache insert");
        if show_elaboration {
            let report = prepared.elaboration_report();
            println!("elaborated: {}", prepared.elaborated_query_text());
            if !report.inferred_types.is_empty() {
                println!("inferred types:");
                for (var, tys) in &report.inferred_types {
                    println!("  {var}: {}", tys.join(", "));
                }
            }
            if !report.notes.is_empty() {
                println!("notes:");
                for note in &report.notes {
                    println!("  - {note}");
                }
            }
            let plan_lines = prepared.explain_plan_lines();
            if !plan_lines.is_empty() {
                println!("plan:");
                for l in plan_lines {
                    println!("  {l}");
                }
            }
            if typecheck_only {
                return Ok(());
            }
        }
        prepared.execute(db, meta)?
    };
    let dt = start.elapsed();
    println!(
        "cache={} ({:?})",
        if cache_hit { "hit" } else { "miss" },
        dt
    );

    let vars = if result.selected_vars.is_empty() {
        "(no selected vars)".to_string()
    } else {
        result.selected_vars.join(" ")
    };
    println!("vars: {vars}");
    println!("rows: {}", result.rows.len());

    for (i, row) in result.rows.iter().enumerate() {
        println!("row {i}:");
        for v in &result.selected_vars {
            let Some(id) = row.get(v).copied() else {
                continue;
            };
            println!("  {} = {}", v, describe_entity(db, id));
        }
        if result.selected_vars.is_empty() {
            for (k, id) in row {
                println!("  {} = {}", k, describe_entity(db, *id));
            }
        }
    }

    if result.truncated {
        println!("(truncated by limit)");
    }

    Ok(())
}

fn cmd_schema_constraints(state: &ReplState, args: &[String]) -> Result<()> {
    let Some(meta) = state.meta.as_ref() else {
        return Err(anyhow!(
            "no `.axi` meta-plane loaded (import a canonical `.axi` module first)"
        ));
    };
    if args.is_empty() || args.len() > 2 {
        return Err(anyhow!("usage: constraints <schema> [relation]"));
    }

    let schema_name = args[0].as_str();
    let relation_filter = args.get(1).map(|s| s.as_str());

    let Some(schema) = meta.schemas.get(schema_name) else {
        return Err(anyhow!("unknown schema `{schema_name}`"));
    };

    let mut rels: Vec<&str> = schema
        .constraints_by_relation
        .keys()
        .map(|s| s.as_str())
        .collect();
    rels.sort();

    println!("constraints {schema_name}");
    let mut printed_any = false;
    for rel in rels {
        if let Some(filter) = relation_filter {
            if rel != filter {
                continue;
            }
        }
        let Some(constraints) = schema.constraints_by_relation.get(rel) else {
            continue;
        };
        if constraints.is_empty() {
            continue;
        }
        printed_any = true;
        println!("  {rel}");
        for c in constraints {
            use axiograph_pathdb::axi_semantics::ConstraintDecl;
            match c {
                ConstraintDecl::Key { fields, .. } => {
                    println!("    key({})", fields.join(", "));
                }
                ConstraintDecl::Functional {
                    src_field,
                    dst_field,
                    ..
                } => {
                    println!("    functional({src_field} -> {dst_field})");
                }
                ConstraintDecl::Typing { rule, .. } => {
                    println!("    typing({rule})");
                }
                ConstraintDecl::SymmetricWhereIn { field, values, .. } => {
                    println!("    symmetric_where_in({field} in {{{}}})", values.join(", "));
                }
                ConstraintDecl::Symmetric { .. } => {
                    println!("    symmetric");
                }
                ConstraintDecl::Transitive { .. } => {
                    println!("    transitive");
                }
                ConstraintDecl::NamedBlock { name, .. } => {
                    println!("    named_block({name})");
                }
                ConstraintDecl::Unknown { text, .. } => {
                    println!("    unknown({text})");
                }
            }
        }
    }
    if relation_filter.is_some() && !printed_any {
        println!("  (none)");
    }

    // Named-block constraints are not relation-scoped, so show them separately.
    if relation_filter.is_none() && !schema.named_block_constraints_by_theory.is_empty() {
        println!();
        println!("named blocks {schema_name}");
        let mut theories: Vec<&str> = schema
            .named_block_constraints_by_theory
            .keys()
            .map(|s| s.as_str())
            .collect();
        theories.sort();
        for th in theories {
            let Some(blocks) = schema.named_block_constraints_by_theory.get(th) else {
                continue;
            };
            let mut blocks = blocks.clone();
            blocks.sort_by_key(|b| b.index);
            println!("  {th}");
            for b in blocks {
                println!("    {}", b.name);
                if !b.body.trim().is_empty() {
                    let lines: Vec<&str> = b.body.lines().collect();
                    for line in lines.iter().take(12) {
                        println!("      {line}");
                    }
                    if lines.len() > 12 {
                        println!("      ... ({} more line(s))", lines.len() - 12);
                    }
                }
            }
        }
    }
    Ok(())
}

fn cmd_rules(state: &ReplState, args: &[String]) -> Result<()> {
    if args.len() > 2 {
        return Err(anyhow!("usage: rules [theory] [rule]"));
    }

    let db = require_db(state)?;
    let theory_filter = args.first().map(|s| s.as_str());
    let rule_filter = args.get(1).map(|s| s.as_str());

    let mut theory_ids: Vec<u32> = Vec::new();
    if let Some(tname) = theory_filter {
        theory_ids =
            find_entities_by_type_and_name(db, axiograph_pathdb::axi_meta::META_TYPE_THEORY, tname);
        if theory_ids.is_empty() {
            println!("(no theory named `{tname}` found)");
            return Ok(());
        }
    } else {
        if let Some(ids) = db.find_by_type(axiograph_pathdb::axi_meta::META_TYPE_THEORY) {
            theory_ids = ids.iter().collect();
        }
        if theory_ids.is_empty() {
            println!("(no `.axi` theories loaded)");
            return Ok(());
        }
        theory_ids.sort();
    }

    println!("rewrite_rules");
    for tid in theory_ids {
        let theory_name = db
            .get_entity(tid)
            .and_then(|e| {
                e.attrs
                    .get(axiograph_pathdb::axi_meta::META_ATTR_NAME)
                    .cloned()
            })
            .unwrap_or_else(|| format!("theory_{tid}"));

        let rule_ids = db.follow_one(
            tid,
            axiograph_pathdb::axi_meta::META_REL_THEORY_HAS_REWRITE_RULE,
        );
        if rule_ids.is_empty() {
            continue;
        }

        let mut rules: Vec<(usize, u32)> = Vec::new();
        for rid in rule_ids.iter() {
            let idx = db
                .get_entity(rid)
                .and_then(|e| {
                    e.attrs
                        .get(axiograph_pathdb::axi_meta::ATTR_REWRITE_RULE_INDEX)
                        .and_then(|s| s.parse::<usize>().ok())
                })
                .unwrap_or(usize::MAX);
            rules.push((idx, rid));
        }
        rules.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

        println!("  {theory_name}");

        if let Some(rname) = rule_filter {
            let Some((_idx, rid)) = rules.into_iter().find(|(_idx, rid)| {
                db.get_entity(*rid)
                    .and_then(|e| {
                        e.attrs
                            .get(axiograph_pathdb::axi_meta::META_ATTR_NAME)
                            .cloned()
                    })
                    .map(|n| n == rname)
                    .unwrap_or(false)
            }) else {
                println!("    (no rewrite rule named `{rname}` found)");
                continue;
            };

            let Some(view) = db.get_entity(rid) else {
                println!("    (missing rule entity {rid})");
                continue;
            };
            let orientation = view
                .attrs
                .get(axiograph_pathdb::axi_meta::ATTR_REWRITE_RULE_ORIENTATION)
                .cloned()
                .unwrap_or_else(|| "forward".to_string());
            let vars = view
                .attrs
                .get(axiograph_pathdb::axi_meta::ATTR_REWRITE_RULE_VARS)
                .cloned()
                .unwrap_or_default();
            let lhs = view
                .attrs
                .get(axiograph_pathdb::axi_meta::ATTR_REWRITE_RULE_LHS)
                .cloned()
                .unwrap_or_default();
            let rhs = view
                .attrs
                .get(axiograph_pathdb::axi_meta::ATTR_REWRITE_RULE_RHS)
                .cloned()
                .unwrap_or_default();

            println!("    rule {rname}");
            println!("      orientation: {orientation}");
            if !vars.trim().is_empty() {
                println!("      vars: {vars}");
            }
            println!("      lhs: {lhs}");
            println!("      rhs: {rhs}");
        } else {
            for (_idx, rid) in rules {
                let rule_name = db
                    .get_entity(rid)
                    .and_then(|e| {
                        e.attrs
                            .get(axiograph_pathdb::axi_meta::META_ATTR_NAME)
                            .cloned()
                    })
                    .unwrap_or_else(|| format!("rule_{rid}"));
                println!("    {rule_name}");
            }
        }
    }

    Ok(())
}

fn cmd_sqlish(state: &mut ReplState, args: &[String]) -> Result<()> {
    if args.is_empty() {
        return Err(anyhow!("usage: sql <SQL query>"));
    }
    let query_text = args.join(" ");
    let db = state
        .db
        .as_ref()
        .ok_or_else(|| anyhow!("no database loaded (use `load`, `import_axi`, or `gen`)"))?;
    let meta = state.meta.as_ref();

    let mut query = crate::sqlish::parse_sqlish_query(&query_text)?;
    if query.contexts.is_empty() && !state.contexts.is_empty() {
        query.contexts = state.contexts.clone();
    }
    let key = crate::axql::axql_query_cache_key(&state.snapshot_key, &query);

    let result = if let Some(prepared) = state.query_cache.get_mut(&key) {
        prepared.execute(db, meta)?
    } else {
        let prepared = crate::axql::prepare_axql_query_with_meta(db, &query, meta)?;
        state.query_cache.insert(key.clone(), prepared);
        state
            .query_cache
            .get_mut(&key)
            .expect("query cache insert")
            .execute(db, meta)?
    };

    let vars = if result.selected_vars.is_empty() {
        "(no selected vars)".to_string()
    } else {
        result.selected_vars.join(" ")
    };
    println!("vars: {vars}");
    println!("rows: {}", result.rows.len());

    for (i, row) in result.rows.iter().enumerate() {
        println!("row {i}:");
        for v in &result.selected_vars {
            let Some(id) = row.get(v).copied() else {
                continue;
            };
            println!("  {} = {}", v, describe_entity(db, id));
        }
        if result.selected_vars.is_empty() {
            for (k, id) in row {
                println!("  {} = {}", k, describe_entity(db, *id));
            }
        }
    }

    if result.truncated {
        println!("(truncated by limit)");
    }

    Ok(())
}

fn cmd_ask(state: &mut ReplState, args: &[String]) -> Result<()> {
    if args.is_empty() {
        return Err(anyhow!("usage: ask <query>"));
    }
    let db = state
        .db
        .as_ref()
        .ok_or_else(|| anyhow!("no database loaded (use `load`, `import_axi`, or `gen`)"))?;
    let meta = state.meta.as_ref();

    let mut query = crate::nlq::parse_ask_query(args)?;
    if query.contexts.is_empty() && !state.contexts.is_empty() {
        query.contexts = state.contexts.clone();
    }
    println!("axql: {}", crate::nlq::render_axql_query(&query));

    let key = crate::axql::axql_query_cache_key(&state.snapshot_key, &query);

    let result = if let Some(prepared) = state.query_cache.get_mut(&key) {
        prepared.execute(db, meta)?
    } else {
        let prepared = crate::axql::prepare_axql_query_with_meta(db, &query, meta)?;
        state.query_cache.insert(key.clone(), prepared);
        state
            .query_cache
            .get_mut(&key)
            .expect("query cache insert")
            .execute(db, meta)?
    };

    let vars = if result.selected_vars.is_empty() {
        "(no selected vars)".to_string()
    } else {
        result.selected_vars.join(" ")
    };
    println!("vars: {vars}");
    println!("rows: {}", result.rows.len());

    for (i, row) in result.rows.iter().enumerate() {
        println!("row {i}:");
        for v in &result.selected_vars {
            let Some(id) = row.get(v).copied() else {
                continue;
            };
            println!("  {} = {}", v, describe_entity(db, id));
        }
        if result.selected_vars.is_empty() {
            for (k, id) in row {
                println!("  {} = {}", k, describe_entity(db, *id));
            }
        }
    }

    if result.truncated {
        println!("(truncated by limit)");
    }

    Ok(())
}

fn cmd_llm(state: &mut ReplState, args: &[String]) -> Result<()> {
    if args.is_empty() || args[0].eq_ignore_ascii_case("status") {
        println!("{}", state.llm.status_line());
        return Ok(());
    }

    match args[0].to_ascii_lowercase().as_str() {
        "disable" => {
            state.llm.backend = crate::llm::LlmBackend::Disabled;
            println!("ok: {}", state.llm.status_line());
            Ok(())
        }
        "model" => {
            if args.len() == 1 {
                println!("{}", state.llm.status_line());
                return Ok(());
            }
            state.llm.model = Some(args[1..].join(" "));
            println!("ok: {}", state.llm.status_line());
            Ok(())
        }
        "use" => {
            if args.len() < 2 {
                return Err(anyhow!(
                    "usage: llm use mock | llm use command <exe> [args...] | llm use ollama [model] | llm use openai [model] | llm use anthropic [model]"
                ));
            }
            match args[1].to_ascii_lowercase().as_str() {
                "mock" => {
                    state.llm.backend = crate::llm::LlmBackend::Mock;
                    println!("ok: {}", state.llm.status_line());
                    Ok(())
                }
                "ollama" => {
                    #[cfg(feature = "llm-ollama")]
                    {
                        state.llm.backend = crate::llm::LlmBackend::Ollama {
                            host: crate::llm::default_ollama_host(),
                        };
                        if args.len() > 2 {
                            state.llm.model = Some(args[2..].join(" "));
                        }
                        println!("ok: {}", state.llm.status_line());
                        Ok(())
                    }
                    #[cfg(not(feature = "llm-ollama"))]
                    {
                        Err(anyhow!(
                            "ollama support not compiled (enable `axiograph-cli` feature `llm-ollama`)"
                        ))
                    }
                }
                "openai" => {
                    #[cfg(feature = "llm-openai")]
                    {
                        state.llm.backend = crate::llm::LlmBackend::OpenAI {
                            base_url: crate::llm::default_openai_base_url(),
                        };
                        if args.len() > 2 {
                            state.llm.model = Some(args[2..].join(" "));
                        } else {
                            let env = std::env::var(crate::llm::OPENAI_MODEL_ENV).unwrap_or_default();
                            let env = env.trim().to_string();
                            state.llm.model = if env.is_empty() { None } else { Some(env) };
                        }
                        println!("ok: {}", state.llm.status_line());
                        Ok(())
                    }
                    #[cfg(not(feature = "llm-openai"))]
                    {
                        Err(anyhow!(
                            "openai support not compiled (enable `axiograph-cli` feature `llm-openai`)"
                        ))
                    }
                }
                "anthropic" => {
                    #[cfg(feature = "llm-anthropic")]
                    {
                        state.llm.backend = crate::llm::LlmBackend::Anthropic {
                            base_url: crate::llm::default_anthropic_base_url(),
                        };
                        if args.len() > 2 {
                            state.llm.model = Some(args[2..].join(" "));
                        } else {
                            let env =
                                std::env::var(crate::llm::ANTHROPIC_MODEL_ENV).unwrap_or_default();
                            let env = env.trim().to_string();
                            state.llm.model = if env.is_empty() { None } else { Some(env) };
                        }
                        println!("ok: {}", state.llm.status_line());
                        Ok(())
                    }
                    #[cfg(not(feature = "llm-anthropic"))]
                    {
                        Err(anyhow!(
                            "anthropic support not compiled (enable `axiograph-cli` feature `llm-anthropic`)"
                        ))
                    }
                }
                "command" => {
                    if args.len() < 3 {
                        return Err(anyhow!("usage: llm use command <exe> [args...]"));
                    }
                    state.llm.backend = crate::llm::LlmBackend::Command {
                        program: PathBuf::from(&args[2]),
                        args: args[3..].to_vec(),
                    };
                    println!("ok: {}", state.llm.status_line());
                    Ok(())
                }
                other => Err(anyhow!(
                    "unknown llm backend `{other}` (try `mock`, `ollama`, `openai`, `anthropic`, or `command`)"
                )),
            }
        }
        "query" => {
            if args.len() < 2 {
                return Err(anyhow!("usage: llm query <question...>"));
            }
            let question = args[1..].join(" ");
            let db = require_db(state)?;

            let generated = state.llm.generate_query(db, &question)?;
            match &generated {
                crate::llm::GeneratedQuery::Axql(q) => println!("axql: {q}"),
                crate::llm::GeneratedQuery::QueryIrV1(ir) => {
                    println!(
                        "query_ir_v1:\n{}",
                        serde_json::to_string_pretty(ir)
                            .unwrap_or_else(|_| "<unprintable>".to_string())
                    );
                    println!("axql: {}", ir.to_axql_text()?);
                }
            }

            Ok(())
        }
        "ask" | "answer" => {
            if args.len() < 2 {
                return Err(anyhow!(
                    "usage: llm ask [--steps N] [--rows N] <question...> | llm answer [--steps N] [--rows N] <question...>"
                ));
            }

            let mut steps: usize = crate::llm::llm_default_max_steps()?;
            let mut rows: usize = 25;

            let mut idx = 1usize;
            while idx < args.len() {
                match args[idx].as_str() {
                    "--steps" => {
                        if idx + 1 >= args.len() {
                            return Err(anyhow!("llm {}: missing value for --steps", args[0]));
                        }
                        steps = args[idx + 1].parse()?;
                        idx += 2;
                    }
                    "--rows" => {
                        if idx + 1 >= args.len() {
                            return Err(anyhow!("llm {}: missing value for --rows", args[0]));
                        }
                        rows = args[idx + 1].parse()?;
                        idx += 2;
                    }
                    _ => break,
                }
            }

            if idx >= args.len() {
                return Err(anyhow!(
                    "usage: llm ask [--steps N] [--rows N] <question...> | llm answer [--steps N] [--rows N] <question...>"
                ));
            }

            let question = args[idx..].join(" ");
            let opts = crate::llm::ToolLoopOptions {
                max_steps: steps,
                max_rows: rows,
                ..Default::default()
            };
            let contexts = state.contexts.clone();
            let snapshot_key = state.snapshot_key.clone();

            let db = state
                .db
                .take()
                .ok_or_else(|| anyhow!("no database loaded (use `load`, `import_axi`, or `gen`)"))?;

            let outcome = crate::llm::run_tool_loop_with_meta(
                &state.llm,
                &db,
                state.meta.as_ref(),
                &contexts,
                &snapshot_key,
                None,
                None,
                None,
                &mut state.query_cache,
                &question,
                opts,
            )?;

            state.db = Some(db);

            println!("\nanswer:\n{}", outcome.final_answer.answer);
            if let Some(rationale) = outcome.final_answer.public_rationale.as_deref() {
                if !rationale.trim().is_empty() {
                    println!("\nrationale:\n{rationale}");
                }
            }
            if !outcome.final_answer.citations.is_empty() {
                println!("\ncitations:");
                for c in &outcome.final_answer.citations {
                    println!("  - {c}");
                }
            }
            if !outcome.final_answer.queries.is_empty() {
                println!("\nqueries:");
                for q in &outcome.final_answer.queries {
                    println!("  - {q}");
                }
            }
            if !outcome.final_answer.notes.is_empty() {
                println!("\nnotes:");
                for n in &outcome.final_answer.notes {
                    println!("  - {n}");
                }
            }

            Ok(())
        }
        "agent" => {
            if args.len() < 2 {
                return Err(anyhow!(
                    "usage: llm agent [--steps N] [--rows N] <question...>"
                ));
            }

            let mut steps: usize = crate::llm::llm_default_max_steps()?;
            let mut rows: usize = 25;

            let mut idx = 1usize;
            while idx < args.len() {
                match args[idx].as_str() {
                    "--steps" => {
                        if idx + 1 >= args.len() {
                            return Err(anyhow!("llm agent: missing value for --steps"));
                        }
                        steps = args[idx + 1].parse()?;
                        idx += 2;
                    }
                    "--rows" => {
                        if idx + 1 >= args.len() {
                            return Err(anyhow!("llm agent: missing value for --rows"));
                        }
                        rows = args[idx + 1].parse()?;
                        idx += 2;
                    }
                    _ => break,
                }
            }

            if idx >= args.len() {
                return Err(anyhow!(
                    "usage: llm agent [--steps N] [--rows N] <question...>"
                ));
            }

            let question = args[idx..].join(" ");
            let opts = crate::llm::ToolLoopOptions {
                max_steps: steps,
                max_rows: rows,
                ..Default::default()
            };
            let contexts = state.contexts.clone();
            let snapshot_key = state.snapshot_key.clone();

            let db = state
                .db
                .take()
                .ok_or_else(|| anyhow!("no database loaded (use `load`, `import_axi`, or `gen`)"))?;

            let outcome = crate::llm::run_tool_loop_with_meta(
                &state.llm,
                &db,
                state.meta.as_ref(),
                &contexts,
                &snapshot_key,
                None,
                None,
                None,
                &mut state.query_cache,
                &question,
                opts,
            )?;

            state.db = Some(db);

            fn truncate_json(v: &serde_json::Value, max_chars: usize) -> String {
                let s = serde_json::to_string_pretty(v)
                    .unwrap_or_else(|_| "<unprintable>".to_string());
                if s.chars().count() <= max_chars {
                    return s;
                }
                let mut out = String::new();
                out.extend(s.chars().take(max_chars));
                out.push('…');
                out
            }

            println!("tool loop steps: {}", outcome.steps.len());
            for (i, step) in outcome.steps.iter().enumerate() {
                println!("step {i}: {}", step.tool);
                println!("args:\n{}", truncate_json(&step.args, 1_200));
                println!("result:\n{}", truncate_json(&step.result, 2_000));
            }

            println!("\nanswer:\n{}", outcome.final_answer.answer);
            if let Some(rationale) = outcome.final_answer.public_rationale.as_deref() {
                if !rationale.trim().is_empty() {
                    println!("\nrationale:\n{rationale}");
                }
            }
            if !outcome.final_answer.citations.is_empty() {
                println!("\ncitations:");
                for c in &outcome.final_answer.citations {
                    println!("  - {c}");
                }
            }
            if !outcome.final_answer.queries.is_empty() {
                println!("\nqueries:");
                for q in &outcome.final_answer.queries {
                    println!("  - {q}");
                }
            }
            if !outcome.final_answer.notes.is_empty() {
                println!("\nnotes:");
                for n in &outcome.final_answer.notes {
                    println!("  - {n}");
                }
            }

            Ok(())
        }
        other => Err(anyhow!(
            "unknown llm subcommand `{other}` (try: status, use, model, ask, answer, query, agent)"
        )),
    }
}

fn describe_entity(db: &axiograph_pathdb::PathDB, entity_id: u32) -> String {
    let Some(view) = db.get_entity(entity_id) else {
        return format!("{entity_id} (missing)");
    };

    if let Some(name) = view.attrs.get("name") {
        return format!("{entity_id} ({}, name={})", view.entity_type, name);
    }

    format!("{entity_id} ({})", view.entity_type)
}

fn split_command_line(line: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '"' => in_quotes = !in_quotes,
            '\\' => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            c if c.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(c),
        }
    }

    if !current.is_empty() {
        out.push(current);
    }

    out
}

fn tokenize_repl_line(line: &str) -> Vec<String> {
    let line = line.trim();
    if line.is_empty() {
        return Vec::new();
    }

    // Special-case AxQL: preserve quotes inside the query string.
    //
    // The REPL tokenization layer exists mainly to support convenient quoting for
    // file paths and natural language prompts. For AxQL, however, stripping
    // quotes changes the meaning (and can make URLs/IRIs unparsable).
    //
    // So we parse:
    //   q [--elaborate|--typecheck] <raw query...>
    // as:
    //   ["q", "--elaborate", "<raw query...>"]
    //
    // preserving the query text verbatim after the option prefix.
    {
        let mut cmd_end = None;
        for (i, c) in line.char_indices() {
            if c.is_whitespace() {
                cmd_end = Some(i);
                break;
            }
        }

        let (cmd, rest) = match cmd_end {
            Some(i) => (&line[..i], line[i..].trim_start()),
            None => (line, ""),
        };

        if cmd == "q" || cmd == "axql" {
            let mut out: Vec<String> = vec![cmd.to_string()];
            if rest.is_empty() {
                return out;
            }

            // Identify the option prefix using the standard tokenizer (options
            // themselves are not quote-sensitive), but slice the raw query from
            // the original text.
            let rest_tokens = split_command_line(rest);
            let mut opt_count = 0usize;
            for t in &rest_tokens {
                if t.starts_with('-') {
                    opt_count += 1;
                } else {
                    break;
                }
            }
            out.extend(rest_tokens.iter().take(opt_count).cloned());

            // Skip `opt_count` leading tokens in the raw `rest` string.
            let mut i = 0usize;
            let bytes = rest.as_bytes();
            let mut skipped = 0usize;
            while skipped < opt_count {
                while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                skipped += 1;
            }

            let raw_query = rest[i..].trim_start();
            if !raw_query.is_empty() {
                out.push(raw_query.to_string());
            }
            return out;
        }
    }

    split_command_line(line)
}

fn cmd_world_model(state: &mut ReplState, args: &[String]) -> Result<()> {
    if args.is_empty() || args[0].eq_ignore_ascii_case("status") {
        println!("{}", state.world_model.status_line());
        return Ok(());
    }

    match args[0].to_ascii_lowercase().as_str() {
        "disable" => {
            state.world_model.backend = crate::world_model::WorldModelBackend::Disabled;
            println!("ok: {}", state.world_model.status_line());
            Ok(())
        }
        "model" => {
            if args.len() == 1 {
                println!("{}", state.world_model.status_line());
                return Ok(());
            }
            state.world_model.model = Some(args[1..].join(" "));
            println!("ok: {}", state.world_model.status_line());
            Ok(())
        }
        "use" => {
            if args.len() < 2 {
                return Err(anyhow!(
                    "usage: wm use stub | wm use command <exe> [args...]"
                ));
            }
            match args[1].to_ascii_lowercase().as_str() {
                "stub" => {
                    state.world_model.backend = crate::world_model::WorldModelBackend::Stub;
                    println!("ok: {}", state.world_model.status_line());
                    Ok(())
                }
                "command" => {
                    if args.len() < 3 {
                        return Err(anyhow!("usage: wm use command <exe> [args...]"));
                    }
                    state.world_model.backend = crate::world_model::WorldModelBackend::Command {
                        program: PathBuf::from(&args[2]),
                        args: args[3..].to_vec(),
                    };
                    println!("ok: {}", state.world_model.status_line());
                    Ok(())
                }
                other => Err(anyhow!("unknown wm backend `{other}`")),
            }
        }
        "propose" => cmd_world_model_propose_repl(state, &args[1..]),
        "plan" => cmd_world_model_plan_repl(state, &args[1..]),
        _ => Err(anyhow!(
            "unknown wm subcommand (try: wm status | wm use ... | wm propose ... | wm plan ...)"
        )),
    }
}

fn cmd_world_model_propose_repl(state: &mut ReplState, args: &[String]) -> Result<()> {
    let Some(db) = state.db.as_ref() else {
        return Err(anyhow!("no db loaded (use `load` or `import_axi`)"));
    };
    if args.is_empty() {
        return Err(anyhow!(
            "usage: wm propose <out.json> [--goal <text>] [--max N] [--guardrail off|fast|strict] [--plane meta|data|both] [--export <file>] [--axi <file>] [--commit-dir <dir>] [--accepted-snapshot <id>] [--message <msg>] [--no-validate]"
        ));
    }

    let mut out: Option<PathBuf> = None;
    let mut goals: Vec<String> = Vec::new();
    let mut max_new: usize = 0;
    let mut guardrail_profile = "fast".to_string();
    let mut guardrail_plane = "both".to_string();
    let mut export_path: Option<PathBuf> = None;
    let mut axi_path: Option<PathBuf> = None;
    let mut commit_dir: Option<PathBuf> = None;
    let mut accepted_snapshot = "head".to_string();
    let mut commit_message: Option<String> = None;
    let mut validate = true;
    let mut seed: Option<u64> = None;
    let mut guardrail_weight_pairs: Vec<String> = Vec::new();
    let mut task_cost_pairs: Vec<String> = Vec::new();
    let mut horizon_steps: Option<usize> = None;

    let mut i = 0usize;
    while i < args.len() {
        let tok = args[i].as_str();
        match tok {
            "--goal" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err(anyhow!("--goal requires a value"));
                };
                goals.push(v.to_string());
            }
            "--max" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err(anyhow!("--max requires a value"));
                };
                max_new = v.parse::<usize>().map_err(|_| {
                    anyhow!("invalid --max value `{}` (expected integer)", v)
                })?;
            }
            "--guardrail" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err(anyhow!("--guardrail requires a value"));
                };
                guardrail_profile = v.to_string();
            }
            "--plane" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err(anyhow!("--plane requires a value"));
                };
                guardrail_plane = v.to_string();
            }
            "--export" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err(anyhow!("--export requires a path"));
                };
                export_path = Some(PathBuf::from(v));
            }
            "--axi" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err(anyhow!("--axi requires a path"));
                };
                axi_path = Some(PathBuf::from(v));
            }
            "--commit-dir" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err(anyhow!("--commit-dir requires a path"));
                };
                commit_dir = Some(PathBuf::from(v));
            }
            "--accepted-snapshot" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err(anyhow!("--accepted-snapshot requires a value"));
                };
                accepted_snapshot = v.to_string();
            }
            "--message" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err(anyhow!("--message requires a value"));
                };
                commit_message = Some(v.to_string());
            }
            "--no-validate" => {
                validate = false;
            }
            "--seed" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err(anyhow!("--seed requires a value"));
                };
                seed = Some(v.parse::<u64>().map_err(|_| {
                    anyhow!("invalid --seed value `{}` (expected integer)", v)
                })?);
            }
            "--guardrail-weight" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err(anyhow!("--guardrail-weight requires key=value"));
                };
                guardrail_weight_pairs.push(v.to_string());
            }
            "--task-cost" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err(anyhow!("--task-cost requires name=value[:weight[:unit]]"));
                };
                task_cost_pairs.push(v.to_string());
            }
            "--horizon-steps" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err(anyhow!("--horizon-steps requires a value"));
                };
                horizon_steps = Some(v.parse::<usize>().map_err(|_| {
                    anyhow!("invalid --horizon-steps value `{}`", v)
                })?);
            }
            _ => {
                if out.is_none() {
                    out = Some(PathBuf::from(tok));
                } else {
                    return Err(anyhow!("unknown argument `{}`", tok));
                }
            }
        }
        i += 1;
    }

    let out = out.ok_or_else(|| anyhow!("wm propose: missing output path"))?;

    let guardrail_profile = guardrail_profile.trim().to_ascii_lowercase();
    let guardrail_plane = guardrail_plane.trim().to_ascii_lowercase();
    let guardrail_weights = if guardrail_weight_pairs.is_empty() {
        crate::world_model::GuardrailCostWeightsV1::defaults()
    } else {
        crate::world_model::parse_guardrail_weights(&guardrail_weight_pairs)?
    };
    let task_costs = crate::world_model::parse_task_costs(&task_cost_pairs)?;

    let guardrail = if guardrail_profile != "off" {
        Some(crate::world_model::compute_guardrail_costs(
            db,
            "repl",
            &guardrail_profile,
            &guardrail_plane,
            &guardrail_weights,
        )?)
    } else {
        None
    };

    let mut export_inline: Option<crate::world_model::JepaExportFileV1> = None;
    let mut export_path_str: Option<String> = None;
    if let Some(path) = export_path.as_ref() {
        export_path_str = Some(path.display().to_string());
    } else if let Some(axi) = axi_path.as_ref() {
        let text = fs::read_to_string(axi)?;
        let opts = crate::world_model::JepaExportOptions {
            instance_filter: None,
            max_items: 0,
            mask_fields: 1,
            seed: 1,
        };
        export_inline = Some(crate::world_model::build_jepa_export_from_axi_text(&text, &opts)?);
    }

    let mut input = crate::world_model::WorldModelInputV1::default();
    input.export = export_inline;
    input.export_path = export_path_str;
    if guardrail.is_some() {
        input.guardrail = guardrail.clone();
    }

    let mut options = crate::world_model::WorldModelOptionsV1::default();
    options.max_new_proposals = max_new;
    options.seed = seed;
    options.goals = goals;
    options.task_costs = task_costs.clone();
    options.horizon_steps = horizon_steps;

    let req = crate::world_model::make_world_model_request(input, options);
    let mut response = state.world_model.propose(&req)?;
    if let Some(err) = response.error.take() {
        return Err(anyhow!("world model error: {err}"));
    }

    let guardrail_profile_label = if guardrail_profile == "off" {
        None
    } else {
        Some(guardrail_profile.clone())
    };
    let guardrail_plane_label = if guardrail_profile == "off" {
        None
    } else {
        Some(guardrail_plane.clone())
    };

    let provenance = crate::world_model::WorldModelProvenance {
        trace_id: response.trace_id.clone(),
        backend: state.world_model.backend_label(),
        model: state.world_model.model.clone(),
        axi_digest_v1: None,
        guardrail_total_cost: guardrail
            .as_ref()
            .map(|g| g.summary.total_cost),
        guardrail_profile: guardrail_profile_label,
        guardrail_plane: guardrail_plane_label,
    };

    let mut proposals =
        crate::world_model::apply_world_model_provenance(response.proposals, &provenance);

    if max_new > 0 && proposals.proposals.len() > max_new {
        proposals.proposals.truncate(max_new);
    }

    let json = serde_json::to_string_pretty(&proposals)?;
    fs::write(&out, json)?;
    println!("wrote {}", out.display());

    if let Some(dir) = commit_dir.as_ref() {
        if validate {
            let profile = if guardrail_profile == "off" {
                "fast"
            } else {
                guardrail_profile.as_str()
            };
            let plane = if guardrail_profile == "off" {
                "both"
            } else {
                guardrail_plane.as_str()
            };
            let validation = crate::proposals_validate::validate_proposals_v1(
                db,
                &proposals,
                profile,
                plane,
            )?;
            if !validation.ok {
                return Err(anyhow!(
                    "refusing to commit: proposals validation failed (errors={}, warnings={})",
                    validation.quality_delta.summary.error_count,
                    validation.quality_delta.summary.warning_count
                ));
            }
        }

        let res = crate::pathdb_wal::commit_pathdb_snapshot_with_overlays(
            dir,
            &accepted_snapshot,
            &[],
            &[out.clone()],
            commit_message.as_deref(),
        )?;
        println!(
            "ok committed {} WAL op(s) on accepted snapshot {} → pathdb snapshot {}",
            res.ops_added, res.accepted_snapshot_id, res.snapshot_id
        );
    }

    Ok(())
}

fn cmd_world_model_plan_repl(state: &mut ReplState, args: &[String]) -> Result<()> {
    let Some(db) = state.db.as_ref() else {
        return Err(anyhow!("no db loaded (use `load` or `import_axi`)"));
    };
    if args.is_empty() {
        return Err(anyhow!(
            "usage: wm plan <out.json> [--steps N] [--rollouts N] [--goal <text>] [--max N] [--guardrail off|fast|strict] [--plane meta|data|both] [--export <file>] [--axi <file>] [--cq <name=query>] [--cq-file <file>] [--commit-dir <dir>] [--accepted-snapshot <id>] [--message <msg>] [--no-validate]"
        ));
    }

    let mut out: Option<PathBuf> = None;
    let mut goals: Vec<String> = Vec::new();
    let mut max_new: usize = 0;
    let mut guardrail_profile = "fast".to_string();
    let mut guardrail_plane = "both".to_string();
    let mut export_path: Option<PathBuf> = None;
    let mut axi_path: Option<PathBuf> = None;
    let mut commit_dir: Option<PathBuf> = None;
    let mut accepted_snapshot = "head".to_string();
    let mut commit_message: Option<String> = None;
    let mut validate = true;
    let mut seed: Option<u64> = None;
    let mut guardrail_weight_pairs: Vec<String> = Vec::new();
    let mut task_cost_pairs: Vec<String> = Vec::new();
    let mut cq_pairs: Vec<String> = Vec::new();
    let mut cq_files: Vec<PathBuf> = Vec::new();
    let mut steps: usize = 3;
    let mut rollouts: usize = 2;
    let mut quality = "fast".to_string();
    let mut quality_plane = "both".to_string();

    let mut i = 0usize;
    while i < args.len() {
        let tok = args[i].as_str();
        match tok {
            "--goal" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err(anyhow!("--goal requires a value"));
                };
                goals.push(v.to_string());
            }
            "--max" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err(anyhow!("--max requires a value"));
                };
                max_new = v.parse::<usize>().map_err(|_| {
                    anyhow!("invalid --max value `{}` (expected integer)", v)
                })?;
            }
            "--guardrail" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err(anyhow!("--guardrail requires a value"));
                };
                guardrail_profile = v.to_string();
            }
            "--plane" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err(anyhow!("--plane requires a value"));
                };
                guardrail_plane = v.to_string();
            }
            "--export" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err(anyhow!("--export requires a path"));
                };
                export_path = Some(PathBuf::from(v));
            }
            "--axi" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err(anyhow!("--axi requires a path"));
                };
                axi_path = Some(PathBuf::from(v));
            }
            "--commit-dir" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err(anyhow!("--commit-dir requires a path"));
                };
                commit_dir = Some(PathBuf::from(v));
            }
            "--accepted-snapshot" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err(anyhow!("--accepted-snapshot requires a value"));
                };
                accepted_snapshot = v.to_string();
            }
            "--message" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err(anyhow!("--message requires a value"));
                };
                commit_message = Some(v.to_string());
            }
            "--no-validate" => {
                validate = false;
            }
            "--seed" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err(anyhow!("--seed requires a value"));
                };
                seed = Some(v.parse::<u64>().map_err(|_| {
                    anyhow!("invalid --seed value `{}` (expected integer)", v)
                })?);
            }
            "--guardrail-weight" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err(anyhow!("--guardrail-weight requires key=value"));
                };
                guardrail_weight_pairs.push(v.to_string());
            }
            "--task-cost" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err(anyhow!("--task-cost requires name=value[:weight[:unit]]"));
                };
                task_cost_pairs.push(v.to_string());
            }
            "--steps" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err(anyhow!("--steps requires a value"));
                };
                steps = v.parse::<usize>().map_err(|_| {
                    anyhow!("invalid --steps value `{}`", v)
                })?;
            }
            "--rollouts" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err(anyhow!("--rollouts requires a value"));
                };
                rollouts = v.parse::<usize>().map_err(|_| {
                    anyhow!("invalid --rollouts value `{}`", v)
                })?;
            }
            "--quality" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err(anyhow!("--quality requires a value"));
                };
                quality = v.to_string();
            }
            "--quality-plane" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err(anyhow!("--quality-plane requires a value"));
                };
                quality_plane = v.to_string();
            }
            "--cq" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err(anyhow!("--cq requires name=query"));
                };
                cq_pairs.push(v.to_string());
            }
            "--cq-file" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err(anyhow!("--cq-file requires a path"));
                };
                cq_files.push(PathBuf::from(v));
            }
            _ => {
                if out.is_none() {
                    out = Some(PathBuf::from(tok));
                } else {
                    return Err(anyhow!("unknown argument `{}`", tok));
                }
            }
        }
        i += 1;
    }

    let out = out.ok_or_else(|| anyhow!("wm plan: missing output path"))?;

    let guardrail_profile = guardrail_profile.trim().to_ascii_lowercase();
    let guardrail_plane = guardrail_plane.trim().to_ascii_lowercase();
    let guardrail_weights = if guardrail_weight_pairs.is_empty() {
        crate::world_model::GuardrailCostWeightsV1::defaults()
    } else {
        crate::world_model::parse_guardrail_weights(&guardrail_weight_pairs)?
    };
    let task_costs = crate::world_model::parse_task_costs(&task_cost_pairs)?;
    let mut competency_questions =
        crate::world_model::parse_competency_questions(&cq_pairs)?;
    for path in &cq_files {
        let mut loaded = crate::world_model::load_competency_questions(path)?;
        competency_questions.append(&mut loaded);
    }

    let mut export_inline: Option<crate::world_model::JepaExportFileV1> = None;
    let mut export_path_str: Option<String> = None;
    if let Some(path) = export_path.as_ref() {
        export_path_str = Some(path.display().to_string());
    } else if let Some(axi) = axi_path.as_ref() {
        let text = fs::read_to_string(axi)?;
        let opts = crate::world_model::JepaExportOptions {
            instance_filter: None,
            max_items: 0,
            mask_fields: 1,
            seed: 1,
        };
        export_inline = Some(crate::world_model::build_jepa_export_from_axi_text(&text, &opts)?);
    }

    let mut base_input = crate::world_model::WorldModelInputV1::default();
    base_input.export = export_inline;
    base_input.export_path = export_path_str;

    let plan_opts = crate::world_model::WorldModelPlanOptionsV1 {
        horizon_steps: steps,
        rollouts,
        max_new_proposals: max_new,
        seed,
        goals,
        task_costs,
        competency_questions,
        guardrail_profile: guardrail_profile.clone(),
        guardrail_plane: guardrail_plane.clone(),
        guardrail_weights,
        include_guardrail: guardrail_profile != "off",
        validation_profile: quality.clone(),
        validation_plane: quality_plane.clone(),
    };

    let report = crate::world_model::run_world_model_plan(
        db,
        &state.world_model,
        &base_input,
        &plan_opts,
    )?;

    let json = serde_json::to_string_pretty(&report)?;
    fs::write(&out, json)?;
    println!("wrote {}", out.display());

    if let Some(dir) = commit_dir.as_ref() {
        let generated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .to_string();
        let mut merged = axiograph_ingest_docs::ProposalsFileV1 {
            version: axiograph_ingest_docs::proposals::PROPOSALS_VERSION_V1,
            generated_at,
            source: axiograph_ingest_docs::ProposalSourceV1 {
                source_type: "world_model_plan".to_string(),
                locator: report.trace_id.clone(),
            },
            schema_hint: None,
            proposals: Vec::new(),
        };
        for step in &report.steps {
            merged.proposals.extend(step.proposals.proposals.clone());
        }

        if validate {
            if quality != "off" {
                let validation = crate::proposals_validate::validate_proposals_v1(
                    db,
                    &merged,
                    &quality,
                    &quality_plane,
                )?;
                if !validation.ok {
                    return Err(anyhow!(
                        "refusing to commit: proposals validation failed (errors={}, warnings={})",
                        validation.quality_delta.summary.error_count,
                        validation.quality_delta.summary.warning_count
                    ));
                }
            }
        }

        let tmp_path = std::env::temp_dir().join(format!(
            "axiograph_wm_plan_{}.json",
            report.trace_id.replace(':', "_")
        ));
        let json = serde_json::to_string_pretty(&merged)?;
        fs::write(&tmp_path, json)?;

        let res = crate::pathdb_wal::commit_pathdb_snapshot_with_overlays(
            dir,
            &accepted_snapshot,
            &[],
            &[tmp_path.clone()],
            commit_message.as_deref(),
        )?;
        let _ = std::fs::remove_file(&tmp_path);
        println!(
            "ok committed {} WAL op(s) on accepted snapshot {} -> pathdb snapshot {}",
            res.ops_added, res.accepted_snapshot_id, res.snapshot_id
        );
    }

    Ok(())
}

#[cfg(test)]
mod repl_tokenize_tests {
    use super::*;

    #[test]
    fn tokenize_repl_line_preserves_axql_quotes_and_urls() {
        let tokens = tokenize_repl_line(
            r#"q select ?x where attr(?x, "iri", "http://example.org/a") limit 3"#,
        );
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0], "q");
        assert_eq!(
            tokens[1],
            r#"select ?x where attr(?x, "iri", "http://example.org/a") limit 3"#
        );
    }

    #[test]
    fn tokenize_repl_line_preserves_axql_with_options() {
        let tokens = tokenize_repl_line(
            r#"q --elaborate select ?x where name("Alice") -Parent-> ?x limit 3"#,
        );
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0], "q");
        assert_eq!(tokens[1], "--elaborate");
        assert_eq!(
            tokens[2],
            r#"select ?x where name("Alice") -Parent-> ?x limit 3"#
        );
    }
}
