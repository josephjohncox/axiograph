use std::path::Path;

use anyhow::Result;
use axiograph_pathdb::axi_semantics::MetaPlaneIndex;
use axiograph_pathdb::{AcceptedAxiAnchor, PathDB};
use serde::Deserialize;
use serde_json::{json, Value};

pub(crate) const INDUSTRIAL_HARNESS_INSPECT_TOOL_NAME: &str = "industrial_harness_inspect";
pub(crate) const INDUSTRIAL_HARNESS_RUN_REGULATED_SEED_TOOL_NAME: &str =
    "industrial_harness_run_regulated_seed";

#[derive(Debug, Clone)]
pub(crate) struct IndustrialHarnessToolSpecV1 {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
    pub read_only_hint: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct IndustrialHarnessToolContext<'a> {
    pub db: Option<&'a PathDB>,
    pub meta: Option<&'a MetaPlaneIndex>,
    pub accepted_axi_anchor: Option<&'a AcceptedAxiAnchor>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct IndustrialHarnessInspectArgs {
    pub cache_root: String,
    pub campaign_id: String,
    pub run_id: String,
}

pub(crate) fn industrial_harness_tool_specs() -> Vec<IndustrialHarnessToolSpecV1> {
    vec![IndustrialHarnessToolSpecV1 {
        name: INDUSTRIAL_HARNESS_INSPECT_TOOL_NAME,
        description: "Read and verify one persisted industrial shadow-harness run from `_cache/industrial_harness/...` without mutating accepted state.",
        input_schema: json!({
            "type": "object",
            "required": ["cache_root", "campaign_id", "run_id"],
            "properties": {
                "cache_root": { "type": "string" },
                "campaign_id": { "type": "string" },
                "run_id": { "type": "string" }
            }
        }),
        read_only_hint: true,
    }, IndustrialHarnessToolSpecV1 {
        name: INDUSTRIAL_HARNESS_RUN_REGULATED_SEED_TOOL_NAME,
        description: "Materialize a regulated production line shadow-harness run into `_cache/industrial_harness/...` using the currently loaded accepted runtime, without mutating accepted ontology state.",
        input_schema: json!({
            "type": "object",
            "required": ["cache_root", "run_id", "created_at_unix_secs"],
            "properties": {
                "cache_root": { "type": "string" },
                "run_id": { "type": "string" },
                "created_at_unix_secs": { "type": "integer", "minimum": 0 }
            }
        }),
        read_only_hint: false,
    }]
}

pub(crate) fn is_industrial_harness_tool(name: &str) -> bool {
    matches!(
        name,
        INDUSTRIAL_HARNESS_INSPECT_TOOL_NAME | INDUSTRIAL_HARNESS_RUN_REGULATED_SEED_TOOL_NAME
    )
}

pub(crate) fn invoke_industrial_harness_tool(
    name: &str,
    context: IndustrialHarnessToolContext<'_>,
    arguments: Value,
) -> Result<Value> {
    match name {
        INDUSTRIAL_HARNESS_INSPECT_TOOL_NAME => {
            serde_json::to_value(call_industrial_harness_inspect(context, arguments)?)
                .map_err(Into::into)
        }
        INDUSTRIAL_HARNESS_RUN_REGULATED_SEED_TOOL_NAME => serde_json::to_value(
            call_industrial_harness_run_regulated_seed(context, arguments)?,
        )
        .map_err(Into::into),
        other => Err(anyhow::anyhow!("unknown industrial harness tool `{other}`")),
    }
}

pub(crate) fn call_industrial_harness_inspect(
    context: IndustrialHarnessToolContext<'_>,
    arguments: Value,
) -> Result<crate::industrial_harness::IndustrialHarnessInspectionResultV1> {
    let args: IndustrialHarnessInspectArgs = serde_json::from_value(arguments)
        .map_err(|err| anyhow::anyhow!("industrial_harness_inspect: invalid args: {err}"))?;
    crate::industrial_harness::inspect_industrial_harness_run(
        Path::new(&args.cache_root),
        &args.campaign_id,
        &args.run_id,
        context.accepted_axi_anchor,
    )
}

pub(crate) fn call_industrial_harness_run_regulated_seed(
    context: IndustrialHarnessToolContext<'_>,
    arguments: Value,
) -> Result<crate::industrial_harness::IndustrialHarnessRunResponseV1> {
    let args: crate::industrial_harness::IndustrialHarnessRunRequestV1 =
        serde_json::from_value(arguments).map_err(|err| {
            anyhow::anyhow!("industrial_harness_run_regulated_seed: invalid args: {err}")
        })?;
    let db = context.db.ok_or_else(|| {
        anyhow::anyhow!("industrial_harness_run_regulated_seed requires a loaded runtime db")
    })?;
    let meta = context.meta.ok_or_else(|| {
        anyhow::anyhow!("industrial_harness_run_regulated_seed requires meta-plane data")
    })?;
    let anchor = context.accepted_axi_anchor.cloned().ok_or_else(|| {
        anyhow::anyhow!(
            "industrial_harness_run_regulated_seed requires a single accepted-module anchor"
        )
    })?;
    crate::industrial_harness::run_regulated_production_line_seed_harness_from_runtime_parts(
        Path::new(&args.cache_root),
        db,
        meta,
        anchor,
        &args.run_id,
        args.created_at_unix_secs,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use axiograph_pathdb::{AcceptedSnapshotId, AxiDigest};

    fn sample_anchor() -> AcceptedAxiAnchor {
        AcceptedAxiAnchor::new(
            AcceptedSnapshotId::new("accepted:regulated-line"),
            AxiDigest::new("fnv1a64:regulated-line"),
        )
    }

    fn sample_trust() -> crate::trust_contract::TrustContractV1 {
        crate::trust_contract::TrustContractV1 {
            trust_class: "runtime_guarded".to_string(),
            soundness: "seed_cache_artifacts_are_anchor_scoped".to_string(),
            coverage: "regulated_line_seed_shadow_harness".to_string(),
            scope: crate::trust_contract::TrustScopeV1 {
                anchor: "accepted:regulated-line@fnv1a64:regulated-line".to_string(),
                context: "regulated_production_line_seed".to_string(),
            },
            reasons: Vec::new(),
            certifiable_disjuncts: None,
            execution_only_disjuncts: None,
            semantic_coverage: None,
            semantic_claims: Vec::new(),
            gaps: Vec::new(),
        }
    }

    fn temp_test_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "axiograph-industrial-harness-tool-{name}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("temp test dir");
        path
    }

    #[test]
    fn industrial_harness_tool_specs_expose_inspect_tool() {
        let names = industrial_harness_tool_specs()
            .into_iter()
            .map(|tool| tool.name)
            .collect::<Vec<_>>();
        assert!(names
            .iter()
            .any(|name| *name == INDUSTRIAL_HARNESS_INSPECT_TOOL_NAME));
        assert!(names
            .iter()
            .any(|name| *name == INDUSTRIAL_HARNESS_RUN_REGULATED_SEED_TOOL_NAME));
    }

    #[test]
    fn industrial_harness_inspect_dispatch_reads_persisted_run() -> Result<()> {
        let cache_root = temp_test_dir("inspect-tool");
        crate::industrial_harness::materialize_regulated_production_line_seed_harness(
            &cache_root,
            "inspect-tool-run",
            4242,
            sample_anchor(),
            sample_trust(),
        )?;

        let out = call_industrial_harness_inspect(
            IndustrialHarnessToolContext {
                db: None,
                meta: None,
                accepted_axi_anchor: Some(&sample_anchor()),
            },
            json!({
                "cache_root": cache_root.to_string_lossy().to_string(),
                "campaign_id": crate::industrial_harness::REGULATED_PRODUCTION_LINE_CAMPAIGN_ID,
                "run_id": "inspect-tool-run"
            }),
        )?;

        assert_eq!(out.bundle.run.run_id, "inspect-tool-run");
        assert_eq!(out.verification.failed, 0);
        std::fs::remove_dir_all(&cache_root).expect("cleanup temp dir");
        Ok(())
    }

    #[test]
    fn industrial_harness_run_dispatch_materializes_seed_artifacts() -> Result<()> {
        let cache_root = temp_test_dir("run-tool");
        let axi = std::fs::read_to_string(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../..")
                .join(crate::industrial_harness::REGULATED_PRODUCTION_LINE_MODULE_PATH),
        )?;
        let mut db = PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, &axi)?;
        db.build_indexes();
        let meta = MetaPlaneIndex::from_db(&db)?;
        let anchor = AcceptedAxiAnchor::new(
            axiograph_pathdb::AcceptedSnapshotId::new("accepted:regulated-line"),
            axiograph_pathdb::AxiDigest::from_axi_text(&axi),
        );

        let out = call_industrial_harness_run_regulated_seed(
            IndustrialHarnessToolContext {
                db: Some(&db),
                meta: Some(&meta),
                accepted_axi_anchor: Some(&anchor),
            },
            json!({
                "cache_root": cache_root.to_string_lossy().to_string(),
                "run_id": "run-tool-seed",
                "created_at_unix_secs": 4242
            }),
        )?;

        assert_eq!(out.run_id, "run-tool-seed");
        assert!(Path::new(&cache_root).join(out.artifacts.run).exists());
        std::fs::remove_dir_all(&cache_root).expect("cleanup temp dir");
        Ok(())
    }
}
