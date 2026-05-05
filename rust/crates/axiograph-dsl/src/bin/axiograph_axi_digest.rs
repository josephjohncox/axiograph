use std::{env, fs};

fn main() {
    let mut args = env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: axiograph_axi_digest <file.axi>");
        std::process::exit(2);
    };
    if args.next().is_some() {
        eprintln!("usage: axiograph_axi_digest <file.axi>");
        std::process::exit(2);
    }

    let text = match fs::read_to_string(&path) {
        Ok(v) => v,
        Err(err) => {
            eprintln!("failed to read `{path}`: {err}");
            std::process::exit(2);
        }
    };

    println!("{}", axiograph_dsl::digest::axi_digest_v1(&text));
}
