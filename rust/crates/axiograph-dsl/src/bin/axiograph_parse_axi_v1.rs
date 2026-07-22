use std::env;

use axiograph_dsl::axi_v1::parse_axi_v1;

fn main() {
    let mut args = env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: axiograph_parse_axi_v1 <file.axi>");
        std::process::exit(2);
    };
    if args.next().is_some() {
        eprintln!("usage: axiograph_parse_axi_v1 <file.axi>");
        std::process::exit(2);
    }

    const MAX_AXI_BYTES: usize = 4 * 1024 * 1024;
    let text = axiograph_security::read_utf8_file_bounded(
        std::path::Path::new(&path),
        MAX_AXI_BYTES,
        "canonical .axi input",
    )
    .unwrap_or_else(|error| {
        eprintln!("failed to read `{path}`: {error}");
        std::process::exit(2);
    });

    match parse_axi_v1(&text) {
        Ok(module_ast) => {
            println!(
                "ok(axi_v1): module={} schemas={} theories={} instances={}",
                module_ast.module_name,
                module_ast.schemas.len(),
                module_ast.theories.len(),
                module_ast.instances.len()
            );
        }
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    }
}
