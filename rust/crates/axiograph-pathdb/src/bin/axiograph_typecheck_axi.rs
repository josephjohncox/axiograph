use std::env;

use axiograph_dsl::axi_v1::parse_axi_v1;
use axiograph_pathdb::validate_axi_v1_module;

fn main() {
    let mut args = env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: axiograph_typecheck_axi <file.axi>");
        std::process::exit(2);
    };
    if args.next().is_some() {
        eprintln!("usage: axiograph_typecheck_axi <file.axi>");
        std::process::exit(2);
    }
    let text = match axiograph_security::read_utf8_file_bounded(
        std::path::Path::new(&path),
        4 * 1024 * 1024,
        "canonical .axi typecheck input",
    ) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("failed to read `{path}`: {error}");
            std::process::exit(2);
        }
    };
    let module = match parse_axi_v1(&text) {
        Ok(module) => module,
        Err(error) => {
            eprintln!("parse error: {error}");
            std::process::exit(1);
        }
    };
    match validate_axi_v1_module(module) {
        Ok(module) => println!(
            "ok(typecheck): module={} schemas={} theories={} instances={} assignments={} tuples={}",
            module.proof().module_name,
            module.proof().schema_count,
            module.proof().theory_count,
            module.proof().instance_count,
            module.proof().assignment_count,
            module.proof().tuple_count,
        ),
        Err(error) => {
            eprintln!("type error: {error}");
            std::process::exit(1);
        }
    }
}
