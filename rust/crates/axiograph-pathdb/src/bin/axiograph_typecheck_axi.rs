use std::env;

use axiograph_dsl::{axi_v1::parse_axi_v1, schema_v1::SchemaV1Module};
use axiograph_pathdb::validate_axi_v1_module;
use serde_json::{json, Value};

const ENVELOPE_SCHEMA: &str = "axiograph.axi_v1_differential_envelope";

fn emit_envelope(
    observed_stage: &str,
    decision: &str,
    normalized_ast: Option<&SchemaV1Module>,
    summary: Value,
    diagnostic: Option<String>,
) -> Result<(), serde_json::Error> {
    let envelope = json!({
        "schema": ENVELOPE_SCHEMA,
        "version": 1,
        "implementation": "rust",
        "requested_stage": "typecheck",
        "observed_stage": observed_stage,
        "decision": decision,
        "normalized_ast": normalized_ast,
        "summary": summary,
        "rejection_class": Value::Null,
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

fn main() {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let (envelope, path) = match args.as_slice() {
        [path] => (false, path.as_str()),
        [flag, path] if flag == "--contract-envelope-v1" => (true, path.as_str()),
        _ => {
            eprintln!("usage: axiograph_typecheck_axi [--contract-envelope-v1] <file.axi>");
            std::process::exit(2);
        }
    };
    let text = match axiograph_security::read_utf8_file_bounded(
        std::path::Path::new(&path),
        4 * 1024 * 1024,
        "canonical .axi typecheck input",
    ) {
        Ok(text) => text,
        Err(error) if envelope => {
            emit_or_exit(emit_envelope(
                "boundary",
                "rejected",
                None,
                Value::Null,
                Some(format!("failed to read `{path}`: {error}")),
            ));
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("failed to read `{path}`: {error}");
            std::process::exit(2);
        }
    };
    let module = match parse_axi_v1(&text) {
        Ok(module) => module,
        Err(error) if envelope => {
            emit_or_exit(emit_envelope(
                "parse",
                "rejected",
                None,
                Value::Null,
                Some(format!("parse error: {error}")),
            ));
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("parse error: {error}");
            std::process::exit(1);
        }
    };
    let normalized_ast = if envelope { Some(module.clone()) } else { None };
    match validate_axi_v1_module(module) {
        Ok(module) if envelope => {
            let proof = module.proof();
            emit_or_exit(emit_envelope(
                "typecheck",
                "accepted",
                normalized_ast.as_ref(),
                json!({
                    "module": proof.module_name,
                    "schemas": proof.schema_count,
                    "theories": proof.theory_count,
                    "instances": proof.instance_count,
                    "assignments": proof.assignment_count,
                    "tuples": proof.tuple_count,
                }),
                None,
            ));
        }
        Ok(module) => println!(
            "ok(typecheck): module={} schemas={} theories={} instances={} assignments={} tuples={}",
            module.proof().module_name,
            module.proof().schema_count,
            module.proof().theory_count,
            module.proof().instance_count,
            module.proof().assignment_count,
            module.proof().tuple_count,
        ),
        Err(error) if envelope => {
            emit_or_exit(emit_envelope(
                "typecheck",
                "rejected",
                normalized_ast.as_ref(),
                Value::Null,
                Some(format!("type error: {error}")),
            ));
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("type error: {error}");
            std::process::exit(1);
        }
    }
}
