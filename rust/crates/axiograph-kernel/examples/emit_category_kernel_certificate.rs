use anyhow::{anyhow, Context, Result};
use axiograph_kernel::{
    CanonicalCompiler, CanonicalModuleSource, KernelCompilationRequest, RepositoryIdV2,
    SnapshotIdV2,
};

#[derive(Debug, Clone, Copy)]
enum Tamper {
    Saturation,
    Presentation,
    Congruence,
    Groupoid,
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or_else(|| {
        anyhow!(
            "usage: emit_category_kernel_certificate <module.axi> <schema> \
             [--tamper-saturation|--tamper-presentation|--tamper-congruence|--tamper-groupoid]"
        )
    })?;
    let schema = args.next().ok_or_else(|| anyhow!("missing schema name"))?;
    let tamper = match args.next().as_deref() {
        None => None,
        Some("--tamper-saturation") => Some(Tamper::Saturation),
        Some("--tamper-presentation") => Some(Tamper::Presentation),
        Some("--tamper-congruence") => Some(Tamper::Congruence),
        Some("--tamper-groupoid") => Some(Tamper::Groupoid),
        Some(flag) => return Err(anyhow!("unknown argument `{flag}`")),
    };
    if args.next().is_some() {
        return Err(anyhow!("too many arguments"));
    }

    let exact = axiograph_security::read_file_bounded(
        std::path::Path::new(&path),
        4 * 1024 * 1024,
        "category-kernel canonical .axi input",
    )
    .with_context(|| format!("read `{path}`"))?;
    let source = CanonicalModuleSource::parse(exact.clone())?;
    let root_module = source.parsed().module_name.clone();
    let compiled = CanonicalCompiler::compile(KernelCompilationRequest {
        repository_id: RepositoryIdV2::from_descriptor_bytes(
            b"category-kernel-certificate-example",
        ),
        accepted_snapshot_id: SnapshotIdV2::from_canonical_fields(&[&exact]),
        root_module,
        modules: vec![source],
    })?;
    let mut envelope = compiled.category_kernel_certificate_json(&schema)?;
    match tamper {
        None => {}
        Some(Tamper::Saturation) => {
            let entries = envelope["proof"]["certificate"]["entries"]
                .as_array_mut()
                .ok_or_else(|| anyhow!("generated certificate has no saturation entries"))?;
            let first = entries
                .first_mut()
                .ok_or_else(|| anyhow!("generated saturation certificate is empty"))?;
            first["target"] = serde_json::json!(1);
        }
        Some(Tamper::Presentation) => {
            let arrows = envelope["proof"]["presentation"]["arrows"]
                .as_array_mut()
                .ok_or_else(|| anyhow!("generated certificate has no presentation arrows"))?;
            let first = arrows
                .first_mut()
                .ok_or_else(|| anyhow!("generated category presentation is empty"))?;
            let target = first["target"]
                .as_u64()
                .ok_or_else(|| anyhow!("presentation arrow target is not numeric"))?;
            first["target"] = serde_json::json!(target.saturating_add(1));
        }
        Some(Tamper::Congruence) => {
            let certificates = envelope["proof"]["congruence_certificates"]
                .as_array_mut()
                .ok_or_else(|| anyhow!("generated certificate has no congruence array"))?;
            let first = certificates
                .first_mut()
                .ok_or_else(|| anyhow!("generated category presentation has no equation"))?;
            let steps = first["steps"]
                .as_array_mut()
                .ok_or_else(|| anyhow!("generated congruence certificate has no steps"))?;
            let step = steps
                .first_mut()
                .ok_or_else(|| anyhow!("generated congruence certificate is empty"))?;
            let offset = step["offset"]
                .as_u64()
                .ok_or_else(|| anyhow!("congruence offset is not numeric"))?;
            step["offset"] = serde_json::json!(offset.saturating_add(1));
        }
        Some(Tamper::Groupoid) => {
            let certificates = envelope["proof"]["groupoid_normalizations"]
                .as_array_mut()
                .ok_or_else(|| anyhow!("generated certificate has no groupoid array"))?;
            let first = certificates
                .first_mut()
                .ok_or_else(|| anyhow!("generated category presentation has no arrows"))?;
            let trace = first["rewrite_trace"]
                .as_array_mut()
                .ok_or_else(|| anyhow!("generated groupoid certificate has no rewrite trace"))?;
            let step = trace
                .first_mut()
                .ok_or_else(|| anyhow!("generated groupoid rewrite trace is empty"))?;
            step["offset"] = serde_json::json!(1);
        }
    }
    println!("{}", serde_json::to_string_pretty(&envelope)?);
    Ok(())
}
