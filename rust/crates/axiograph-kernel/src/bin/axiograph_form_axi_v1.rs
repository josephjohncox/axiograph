use std::{env, path::Path};

use axiograph_kernel::{
    CanonicalCompiler, CanonicalModuleSource, KernelCompilationRequest, KernelCompileError,
    RepositoryIdV2, SnapshotIdV2,
};
use serde_json::{json, Value};

const ENVELOPE_SCHEMA: &str = "axiograph.axi_v1_differential_envelope";
const MAX_AXI_BYTES: usize = 4 * 1024 * 1024;

fn rejection_class(error: &KernelCompileError) -> &'static str {
    match error {
        KernelCompileError::UnknownImport { .. }
        | KernelCompileError::ImportResolution { .. }
        | KernelCompileError::ImportCycle(_)
        | KernelCompileError::UnreachableModules(_) => "formation.import_closure",
        KernelCompileError::InvalidRefinement { detail, .. }
            if detail.contains("no implemented finite witness") =>
        {
            "formation.unsupported_refinement_witness"
        }
        KernelCompileError::InvalidSchemaEquation { .. }
        | KernelCompileError::InvalidRewriteRule { .. } => {
            "formation.unsupported_equation_fragment"
        }
        _ => "formation.finite_model",
    }
}

fn emit(
    observed_stage: &str,
    decision: &str,
    summary: Value,
    class: Option<&str>,
    diagnostic: Option<String>,
) -> Result<(), serde_json::Error> {
    let envelope = json!({
        "schema": ENVELOPE_SCHEMA,
        "version": 1,
        "implementation": "rust",
        "requested_stage": "formation",
        "observed_stage": observed_stage,
        "decision": decision,
        "normalized_ast": Value::Null,
        "summary": summary,
        "rejection_class": class,
        "diagnostic": diagnostic,
    });
    println!("{}", serde_json::to_string(&envelope)?);
    Ok(())
}

fn emit_or_exit(result: Result<(), serde_json::Error>) {
    if let Err(error) = result {
        eprintln!("failed to render differential envelope: {error}");
        std::process::exit(2);
    }
}

fn reject(stage: &str, class: &str, diagnostic: String) -> ! {
    emit_or_exit(emit(
        stage,
        "rejected",
        Value::Null,
        Some(class),
        Some(diagnostic),
    ));
    std::process::exit(1);
}

fn main() {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let path = match args.as_slice() {
        [flag, path] if flag == "--contract-envelope-v1" => path.as_str(),
        _ => {
            eprintln!("usage: axiograph_form_axi_v1 --contract-envelope-v1 <single-module.axi>");
            std::process::exit(2);
        }
    };
    let bytes = match axiograph_security::read_file_bounded(
        Path::new(path),
        MAX_AXI_BYTES,
        "canonical .axi formation input",
    ) {
        Ok(bytes) => bytes,
        Err(error) => reject(
            "boundary",
            "boundary.invalid_utf8",
            format!("failed to read `{path}`: {error}"),
        ),
    };
    let source = match CanonicalModuleSource::parse(bytes) {
        Ok(source) => source,
        Err(error @ KernelCompileError::InvalidUtf8) => {
            reject("boundary", "boundary.invalid_utf8", error.to_string())
        }
        Err(error @ KernelCompileError::Parse { .. }) => {
            reject("parse", "parse.section_syntax", error.to_string())
        }
        Err(error) => reject("formation", rejection_class(&error), error.to_string()),
    };
    let module_name = source.parsed().module_name.clone();
    let snapshot_id = SnapshotIdV2::from_canonical_fields(&[source.exact_text().as_bytes()]);
    let request = KernelCompilationRequest {
        repository_id: RepositoryIdV2::from_descriptor_bytes(b"axi-v1-differential-runner"),
        accepted_snapshot_id: snapshot_id,
        root_module: module_name.clone(),
        modules: vec![source],
    };
    match CanonicalCompiler::compile(request) {
        Ok(snapshot) => emit_or_exit(emit(
            "formation",
            "accepted",
            json!({
                "module": module_name,
                "import_closure": snapshot.ir().ordered_module_closure().len(),
                "schemas": snapshot.ir().schemas().len(),
                "theories": snapshot.ir().theories().len(),
                "instances": snapshot.ir().instances().len(),
            }),
            None,
            None,
        )),
        Err(error) => reject("formation", rejection_class(&error), error.to_string()),
    }
}
