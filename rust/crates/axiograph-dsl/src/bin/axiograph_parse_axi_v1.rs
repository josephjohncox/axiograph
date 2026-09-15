use std::env;

use axiograph_dsl::{axi_v1::parse_axi_v1, schema_v1::SchemaV1Module};
use serde_json::{json, Value};

const ENVELOPE_SCHEMA: &str = "axiograph.axi_v1_differential_envelope";

fn emit_envelope(
    observed_stage: &str,
    decision: &str,
    normalized_ast: Option<&SchemaV1Module>,
    diagnostic: Option<String>,
) -> Result<(), serde_json::Error> {
    let envelope = json!({
        "schema": ENVELOPE_SCHEMA,
        "version": 1,
        "implementation": "rust",
        "requested_stage": "parse",
        "observed_stage": observed_stage,
        "decision": decision,
        "normalized_ast": normalized_ast,
        "summary": Value::Null,
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
    let (mode, path) = match args.as_slice() {
        [path] => ("summary", path.as_str()),
        [flag, path] if flag == "--contract-ast-v1" => ("ast", path.as_str()),
        [flag, path] if flag == "--contract-envelope-v1" => ("envelope", path.as_str()),
        _ => {
            eprintln!(
                "usage: axiograph_parse_axi_v1 [--contract-ast-v1|--contract-envelope-v1] <file.axi>"
            );
            std::process::exit(2);
        }
    };

    const MAX_AXI_BYTES: usize = 4 * 1024 * 1024;
    let text = match axiograph_security::read_utf8_file_bounded(
        std::path::Path::new(&path),
        MAX_AXI_BYTES,
        "canonical .axi input",
    ) {
        Ok(text) => text,
        Err(error) if mode == "envelope" => {
            emit_or_exit(emit_envelope(
                "boundary",
                "rejected",
                None,
                Some(format!("failed to read `{path}`: {error}")),
            ));
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("failed to read `{path}`: {error}");
            std::process::exit(2);
        }
    };

    match parse_axi_v1(&text) {
        Ok(module_ast) => match mode {
            "envelope" => {
                emit_or_exit(emit_envelope("parse", "accepted", Some(&module_ast), None));
            }
            "ast" => {
                let value = serde_json::to_string(&module_ast).unwrap_or_else(|error| {
                    eprintln!("failed to render contract AST: {error}");
                    std::process::exit(2);
                });
                println!("{value}");
            }
            _ => println!(
                "ok(axi_v1): module={} schemas={} theories={} instances={}",
                module_ast.module_name,
                module_ast.schemas.len(),
                module_ast.theories.len(),
                module_ast.instances.len()
            ),
        },
        Err(error) if mode == "envelope" => {
            emit_or_exit(emit_envelope(
                "parse",
                "rejected",
                None,
                Some(error.to_string()),
            ));
            std::process::exit(1);
        }
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    }
}
