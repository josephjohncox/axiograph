use anyhow::{anyhow, Context, Result};
use axiograph_kernel::RevisionDigestV2;
use std::env;

fn main() -> Result<()> {
    let mut args = env::args_os();
    let _program = args.next();
    let path = args
        .next()
        .ok_or_else(|| anyhow!("usage: axiograph_revision_digest <module.axi>"))?;
    if args.next().is_some() {
        return Err(anyhow!("usage: axiograph_revision_digest <module.axi>"));
    }
    let bytes = axiograph_security::read_file_bounded(
        std::path::Path::new(&path),
        4 * 1024 * 1024,
        "canonical .axi digest input",
    )
    .with_context(|| format!("read `{}`", path.to_string_lossy()))?;
    let digest = RevisionDigestV2::from_accepted_bytes(&bytes)?;
    println!("{digest}");
    Ok(())
}
