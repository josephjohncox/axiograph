use anyhow::{anyhow, Result};
use axiograph_pathdb::{AxiDigest, PathDB};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(crate) type PathCertVerifier = dyn Fn(&str, &str) -> Result<(bool, String)>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct PathCertRequestV1 {
    pub start: u32,
    #[serde(default)]
    pub relation_ids: Vec<u32>,
    #[serde(default)]
    pub verify: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PathCertReportV1 {
    pub anchor_digest: AxiDigest,
    pub trust: crate::trust_contract::TrustContractV1,
    pub certificate: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certificate_verified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certificate_verify_output: Option<String>,
}

pub(crate) fn certify_path_from_request_json(
    db: &PathDB,
    request_json: &str,
    verifier: Option<&PathCertVerifier>,
) -> Result<PathCertReportV1> {
    let request: PathCertRequestV1 = serde_json::from_str(request_json)
        .map_err(|err| anyhow!("path cert request JSON: invalid request: {err}"))?;
    certify_path(db, &request, verifier)
}

pub(crate) fn certify_path_from_value(
    db: &PathDB,
    arguments: Value,
    verifier: Option<&PathCertVerifier>,
) -> Result<PathCertReportV1> {
    let request: PathCertRequestV1 = serde_json::from_value(arguments)
        .map_err(|err| anyhow!("path_certify: invalid args: {err}"))?;
    certify_path(db, &request, verifier)
}

pub(crate) fn certify_path(
    db: &PathDB,
    request: &PathCertRequestV1,
    verifier: Option<&PathCertVerifier>,
) -> Result<PathCertReportV1> {
    let (anchor_digest, axi_text) = export_canonical_module_axi(db)?;
    let proof = axiograph_pathdb::witness::reachability_proof_v3_from_relation_ids(
        db,
        request.start,
        &request.relation_ids,
    )?
    .into_inner_in_db(db)
    .map_err(|err| anyhow!(err))?;

    let cert = axiograph_pathdb::certificate::CertificateV2::reachability_v3(proof).with_anchor(
        axiograph_pathdb::certificate::AxiAnchorV1::new(anchor_digest.clone()),
    );

    let certificate = serde_json::to_value(&cert)?;
    let (certificate_verified, certificate_verify_output) = if request.verify {
        let verifier = verifier.ok_or_else(|| {
            anyhow!(
                "path certification verification requested but no Lean verifier is configured for this surface"
            )
        })?;
        let cert_text = serde_json::to_string_pretty(&cert)?;
        let (ok, out) = verifier(&axi_text, &cert_text)?;
        (Some(ok), Some(out))
    } else {
        (None, None)
    };

    Ok(PathCertReportV1 {
        anchor_digest,
        trust: crate::trust_contract::certificate_trust_contract(certificate_verified),
        certificate,
        certificate_verified,
        certificate_verify_output,
    })
}

fn export_canonical_module_axi(db: &PathDB) -> Result<(AxiDigest, String)> {
    let exported = crate::world_model_input::export_pathdb_world_model_axi(
        db,
        &crate::world_model_input::WorldModelAxiInputOptionsV1::default(),
    )?;
    Ok((exported.axi_digest_v1, exported.axi_text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_db() -> Result<(PathDB, u32, u32, u32)> {
        let axi = r#"
module Demo

schema S:
  object Node
  relation road(from: Node, to: Node)

instance I of S:
  Node = {A, B}
  road = {(from=A, to=B)}
"#;
        let mut db = PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)?;
        db.build_indexes();

        let a = find_named_entity(&db, "Node", "A").expect("find A");
        let b = find_named_entity(&db, "Node", "B").expect("find B");
        let ab = find_relation(&db, "road", a, b).expect("find road relation");
        Ok((db, a, b, ab))
    }

    fn find_named_entity(db: &PathDB, type_name: &str, name: &str) -> Option<u32> {
        let ids = db.find_by_type(type_name)?;
        let key = db.interner.id_of("name")?;
        ids.iter().find(|id| {
            db.entities
                .get_attr(*id, key)
                .and_then(|value| db.interner.lookup(value))
                .as_deref()
                == Some(name)
        })
    }

    fn find_relation(db: &PathDB, rel_type: &str, source: u32, target: u32) -> Option<u32> {
        (0..db.relations.len() as u32).find(|rel_id| {
            let Some(rel) = db.relations.get_relation(*rel_id) else {
                return false;
            };
            rel.source == source
                && rel.target == target
                && db.interner.lookup(rel.rel_type).as_deref() == Some(rel_type)
        })
    }

    #[test]
    fn certify_path_returns_reachability_v3_report() -> Result<()> {
        let (db, a, _b, ab) = sample_db()?;
        let report = certify_path(
            &db,
            &PathCertRequestV1 {
                start: a,
                relation_ids: vec![ab],
                verify: false,
            },
            None,
        )?;

        assert_eq!(report.certificate["kind"].as_str(), Some("reachability_v3"));
        assert_eq!(report.certificate["proof"]["type"].as_str(), Some("step"));
        assert!(report.anchor_digest.as_str().starts_with("fnv1a64:"));
        assert!(report.certificate_verified.is_none());
        assert!(report.certificate_verify_output.is_none());
        Ok(())
    }

    #[test]
    fn certify_path_supports_reflexive_empty_relation_ids() -> Result<()> {
        let (db, a, _b, _ab) = sample_db()?;
        let report = certify_path(
            &db,
            &PathCertRequestV1 {
                start: a,
                relation_ids: Vec::new(),
                verify: false,
            },
            None,
        )?;

        assert_eq!(report.certificate["kind"].as_str(), Some("reachability_v3"));
        assert_eq!(
            report.certificate["proof"]["type"].as_str(),
            Some("reflexive")
        );
        Ok(())
    }

    #[test]
    fn certify_path_can_run_verifier_callback() -> Result<()> {
        let (db, a, _b, ab) = sample_db()?;
        let verifier = |_axi: &str, cert_json: &str| -> Result<(bool, String)> {
            assert!(cert_json.contains("reachability_v3"));
            Ok((true, "verified".to_string()))
        };

        let report = certify_path(
            &db,
            &PathCertRequestV1 {
                start: a,
                relation_ids: vec![ab],
                verify: true,
            },
            Some(&verifier),
        )?;

        assert_eq!(report.certificate_verified, Some(true));
        assert_eq!(
            report.certificate_verify_output.as_deref(),
            Some("verified")
        );
        assert_eq!(report.trust.soundness, "lean_verified_certificate");
        Ok(())
    }

    #[test]
    fn certify_path_errors_when_verify_is_requested_without_verifier() -> Result<()> {
        let (db, a, _b, ab) = sample_db()?;
        let err = certify_path(
            &db,
            &PathCertRequestV1 {
                start: a,
                relation_ids: vec![ab],
                verify: true,
            },
            None,
        )
        .expect_err("expected missing verifier error");

        assert!(err
            .to_string()
            .contains("no Lean verifier is configured for this surface"));
        Ok(())
    }
}
