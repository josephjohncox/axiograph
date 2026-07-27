//! Fail-closed, typed bridge to the approved Lean certificate checker.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use axiograph_pathdb::{AnswerIdV2, CertificateIdV2, CertificateV3, QueryIdV2, RevisionDigestV2};
use serde::{de::Error as _, Deserialize, Deserializer, Serialize};

pub(crate) const VERIFIER_PROTOCOL_V2: &str = "axiograph-verifier-stdio-v2";
const MAX_VERIFIER_INPUT_BYTES: usize = 16 * 1024 * 1024;
const MAX_VERIFIER_OUTPUT_BYTES: usize = 1024 * 1024;
const MAX_VERIFIER_EXECUTABLE_BYTES: usize = 256 * 1024 * 1024;

#[derive(Debug, Clone)]
pub(crate) struct CertVerifyConfig {
    pub(crate) verifier_bin: Option<PathBuf>,
    pub(crate) timeout: Option<Duration>,
    /// SHA-256 of the only checker executable approved for this server.
    pub(crate) approved_checker_sha256: Option<String>,
    /// Build identifier expected in the checker's structured receipt.
    pub(crate) approved_checker_build_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct VerifierRequestV2<'a> {
    version: &'static str,
    nonce: &'a str,
    checker_sha256: &'a str,
    module_axi: &'a str,
    certificate_json: &'a str,
    expected_prepared_query_digest: &'a QueryIdV2,
    expected_answer_digest: &'a AnswerIdV2,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct VerifierNonceV2(String);

impl VerifierNonceV2 {
    fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for VerifierNonceV2 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        uuid::Uuid::parse_str(&value).map_err(D::Error::custom)?;
        Ok(Self(value))
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum VerifierDecisionV2 {
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerifierReceiptV2 {
    version: String,
    nonce: VerifierNonceV2,
    checker_sha256: String,
    checker_build_id: String,
    revision_digest_v2: RevisionDigestV2,
    certificate_digest_v2: CertificateIdV2,
    prepared_query_digest_v1: QueryIdV2,
    answer_digest_v1: AnswerIdV2,
    certificate_kind: String,
    claim_kind: String,
    decision: VerifierDecisionV2,
    message: String,
}

impl VerifierReceiptV2 {
    pub(crate) fn accepted(&self) -> bool {
        self.decision == VerifierDecisionV2::Accepted
    }

    #[cfg(test)]
    pub(crate) fn message(&self) -> &str {
        &self.message
    }

    pub(crate) fn revision_digest_v2(&self) -> &RevisionDigestV2 {
        &self.revision_digest_v2
    }

    pub(crate) fn certificate_digest_v2(&self) -> &CertificateIdV2 {
        &self.certificate_digest_v2
    }

    pub(crate) fn prepared_query_digest_v1(&self) -> &QueryIdV2 {
        &self.prepared_query_digest_v1
    }

    pub(crate) fn answer_digest_v1(&self) -> &AnswerIdV2 {
        &self.answer_digest_v1
    }
}

pub(crate) fn resolve_verifier_bin(config: &CertVerifyConfig) -> Option<PathBuf> {
    if let Some(path) = config.verifier_bin.as_ref() {
        return Some(path.clone());
    }
    if let Ok(path) = std::env::var("AXIOGRAPH_VERIFY_BIN") {
        let path = path.trim();
        if !path.is_empty() {
            return Some(PathBuf::from(path));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("axiograph_verify");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    let dev = PathBuf::from("lean/.lake/build/bin/axiograph_verify");
    dev.exists().then_some(dev)
}

#[cfg(test)]
pub(crate) fn sha256_file(path: &Path) -> Result<String> {
    axiograph_security::sha256_file_bounded(
        path,
        MAX_VERIFIER_EXECUTABLE_BYTES,
        "verifier executable",
    )
}

/// Copy the bytes that were actually hashed into a private execution directory.
/// The configured pathname is never reopened by `Command`, closing the
/// hash-then-exec replacement window.
fn stage_approved_verifier(
    source: &Path,
    approved_sha: &str,
) -> Result<axiograph_security::StagedApprovedExecutable> {
    let executable_name = if cfg!(windows) {
        "axiograph_verify.exe"
    } else {
        "axiograph_verify"
    };
    axiograph_security::stage_approved_executable(
        source,
        approved_sha,
        MAX_VERIFIER_EXECUTABLE_BYTES,
        executable_name,
        "verifier executable",
    )
}

fn run_stdio_with_timeout(
    verifier: &PathBuf,
    request: &[u8],
    timeout: Duration,
) -> Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>)> {
    let limits = crate::security::ProcessLimits::new(
        timeout,
        MAX_VERIFIER_INPUT_BYTES,
        MAX_VERIFIER_OUTPUT_BYTES,
        MAX_VERIFIER_OUTPUT_BYTES,
    )?;
    let context = format!("verifier `{}`", verifier.display());
    let mut command = Command::new(verifier);
    command.arg("--stdio-v2");
    let output = crate::security::run_command_bounded(command, request, limits, &context)?;
    Ok((output.status, output.stdout, output.stderr))
}

pub(crate) fn verify_certificate_with_lean(
    config: &CertVerifyConfig,
    module_axi: &str,
    certificate_json: &str,
    expected_prepared_query_digest: &QueryIdV2,
    expected_answer_digest: &AnswerIdV2,
) -> Result<VerifierReceiptV2> {
    let verifier = resolve_verifier_bin(config).ok_or_else(|| {
        anyhow!(
            "Lean verifier not configured (set --verify-bin or AXIOGRAPH_VERIFY_BIN, or build with `make lean-exe`)"
        )
    })?;
    let approved_sha = config
        .approved_checker_sha256
        .as_deref()
        .ok_or_else(|| anyhow!("certificate verification requires --verify-sha256"))?
        .trim()
        .to_ascii_lowercase();
    if approved_sha.len() != 64
        || !approved_sha
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(anyhow!(
            "approved verifier SHA-256 must be 64 lowercase hexadecimal characters"
        ));
    }
    let staged_verifier = stage_approved_verifier(&verifier, &approved_sha)?;
    let approved_build_id = config
        .approved_checker_build_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("certificate verification requires --verify-build-id"))?;
    let timeout = config
        .timeout
        .filter(|limit| !limit.is_zero())
        .ok_or_else(|| anyhow!("certificate verification requires a positive timeout"))?;

    if module_axi.len() > crate::security::MAX_AXI_MODULE_BYTES {
        return Err(anyhow!("verifier module exceeds canonical .axi byte limit"));
    }
    let certificate: CertificateV3 = crate::security::parse_json_bounded(
        certificate_json.as_bytes(),
        crate::security::MAX_JSON_INPUT_BYTES,
        "query_result_v4 certificate",
    )?;
    let expected_anchor = RevisionDigestV2::from_accepted_text(module_axi);
    let expected_certificate =
        CertificateIdV2::from_canonical_fields(&[certificate_json.as_bytes()]);
    if certificate.anchor.revision_digest_v2 != expected_anchor
        || certificate.proof.prepared_query_digest_v1 != *expected_prepared_query_digest
        || certificate.proof.answer_digest_v1 != *expected_answer_digest
    {
        return Err(anyhow!(
            "certificate anchor, prepared-query digest, or answer digest differs from caller expectation"
        ));
    }

    let nonce = VerifierNonceV2::new();
    let request = serde_json::to_vec(&VerifierRequestV2 {
        version: VERIFIER_PROTOCOL_V2,
        nonce: nonce.as_str(),
        checker_sha256: &approved_sha,
        module_axi,
        certificate_json,
        expected_prepared_query_digest,
        expected_answer_digest,
    })?;
    let (status, stdout, stderr) = run_stdio_with_timeout(
        &staged_verifier.executable().to_path_buf(),
        &request,
        timeout,
    )?;
    if stdout.len() > MAX_VERIFIER_OUTPUT_BYTES || stderr.len() > MAX_VERIFIER_OUTPUT_BYTES {
        return Err(anyhow!("verifier output exceeded configured cap"));
    }
    let receipt: VerifierReceiptV2 = crate::security::parse_json_bounded(
        &stdout,
        MAX_VERIFIER_OUTPUT_BYTES,
        "verifier V2 receipt",
    )
    .with_context(|| {
        format!(
            "verifier did not return a valid V2 receipt (stderr: {})",
            String::from_utf8_lossy(&stderr).trim()
        )
    })?;

    if receipt.version != VERIFIER_PROTOCOL_V2
        || receipt.nonce != nonce
        || receipt.checker_sha256 != approved_sha
        || receipt.checker_build_id != approved_build_id
        || receipt.revision_digest_v2 != expected_anchor
        || receipt.certificate_digest_v2 != expected_certificate
        || receipt.prepared_query_digest_v1 != *expected_prepared_query_digest
        || receipt.answer_digest_v1 != *expected_answer_digest
        || receipt.certificate_kind != "query_result_v4"
        || receipt.claim_kind != "finite_exact_complete"
    {
        return Err(anyhow!("verifier receipt identity or digest mismatch"));
    }

    match receipt.decision {
        VerifierDecisionV2::Accepted if status.success() => Ok(receipt),
        VerifierDecisionV2::Rejected if !status.success() => Ok(receipt),
        _ => Err(anyhow!(
            "verifier exit status and structured decision disagree"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axiograph_pathdb::certificate::{
        answer_digest_v1, CertificateAnchorV2, FiniteQueryRowV4, FiniteQueryV4,
        PreparedQueryBindingV1, QueryResultProofV4,
    };

    fn anchored_boolean_query_certificate(
        module_axi: &str,
    ) -> Result<(String, QueryIdV2, AnswerIdV2)> {
        let binding = PreparedQueryBindingV1::new(
            FiniteQueryV4 {
                select_vars: Vec::new(),
                disjuncts: vec![Vec::new()],
                max_hops: None,
                min_confidence_fp: None,
            },
            1,
        );
        let prepared = binding.digest_v1().map_err(anyhow::Error::msg)?;
        let rows = vec![FiniteQueryRowV4 {
            disjunct: 0,
            bindings: Vec::new(),
            witnesses: Vec::new(),
        }];
        let answer =
            answer_digest_v1(&binding, &prepared, &rows, false).map_err(anyhow::Error::msg)?;
        let proof = QueryResultProofV4 {
            binding,
            prepared_query_digest_v1: prepared.clone(),
            rows,
            runtime_truncated: false,
            answer_digest_v1: answer.clone(),
        };
        let certificate = CertificateV3::query_result_v4(
            CertificateAnchorV2::new(RevisionDigestV2::from_accepted_text(module_axi)),
            proof,
        )
        .map_err(anyhow::Error::msg)?;
        Ok((
            serde_json::to_string_pretty(&certificate)?,
            prepared,
            answer,
        ))
    }

    fn lean_checker_fixture() -> Result<Option<(CertVerifyConfig, String)>> {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let verifier = repo.join("lean/.lake/build/bin/axiograph_verify");
        if !verifier.exists() {
            return Ok(None);
        }
        let module_axi = crate::security::read_utf8_file_bounded(
            &repo.join("fixtures/verification/rewrite_rules_anchor_v1.axi"),
            crate::security::MAX_TEXT_INPUT_BYTES,
            "CLI input",
        )?;
        let config = CertVerifyConfig {
            verifier_bin: Some(verifier.clone()),
            timeout: Some(Duration::from_secs(10)),
            approved_checker_sha256: Some(sha256_file(&verifier)?),
            approved_checker_build_id: Some("axiograph-verify-main-v3".to_string()),
        };
        Ok(Some((config, module_axi)))
    }

    #[cfg(unix)]
    fn executable_script(contents: &str) -> Result<(tempfile::TempDir, PathBuf)> {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir()?;
        let path = dir.path().join("fake-checker");
        crate::security::write_output_bounded(&path, contents, "CLI output")?;
        let mut permissions = std::fs::metadata(&path)?.permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions)?;
        Ok((dir, path))
    }

    #[test]
    fn staged_verifier_retains_the_bytes_that_were_hashed() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let source = directory.path().join("checker");
        let approved_bytes = b"approved checker bytes";
        crate::security::write_output_bounded(&source, approved_bytes, "CLI output")?;
        let approved_sha = sha256_file(&source)?;
        let staged = stage_approved_verifier(&source, &approved_sha)?;
        crate::security::write_output_bounded(&source, b"replacement checker bytes", "CLI output")?;
        assert_eq!(std::fs::read(staged.executable())?, approved_bytes);
        Ok(())
    }

    #[test]
    fn oversized_verifier_request_rejects_before_spawn() {
        let error = run_stdio_with_timeout(
            &PathBuf::from("definitely-not-a-verifier"),
            &vec![b'x'; MAX_VERIFIER_INPUT_BYTES + 1],
            Duration::from_millis(50),
        )
        .expect_err("oversized verifier request must reject");
        assert!(error.to_string().contains("stdin exceeds"));
    }

    #[cfg(unix)]
    #[test]
    fn verifier_output_flood_and_timeout_are_bounded() -> Result<()> {
        let (_flood_dir, flood) = executable_script("#!/bin/sh\nyes x\n")?;
        let flood_error = run_stdio_with_timeout(&flood, b"{}", Duration::from_secs(2))
            .expect_err("verifier output flood must reject");
        assert!(flood_error.to_string().contains("stdout exceeded"));

        let (_sleep_dir, sleep) = executable_script("#!/bin/sh\nsleep 2\n")?;
        let timeout_error = run_stdio_with_timeout(&sleep, b"{}", Duration::from_millis(50))
            .expect_err("hung verifier must time out");
        assert!(timeout_error.to_string().contains("timed out"));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn true_binary_is_rejected_even_when_its_hash_is_approved() -> Result<()> {
        let path = ["/bin/true", "/usr/bin/true"]
            .into_iter()
            .map(PathBuf::from)
            .find(|candidate| candidate.exists())
            .ok_or_else(|| anyhow!("system true binary not found"))?;
        let module_axi = "module X\n";
        let (certificate, prepared, answer) = anchored_boolean_query_certificate(module_axi)?;
        let config = CertVerifyConfig {
            verifier_bin: Some(path.clone()),
            timeout: Some(Duration::from_secs(1)),
            approved_checker_sha256: Some(sha256_file(&path)?),
            approved_checker_build_id: Some("axiograph-verify-main-v3".to_string()),
        };
        verify_certificate_with_lean(&config, module_axi, &certificate, &prepared, &answer)
            .err()
            .ok_or_else(|| anyhow!("system true binary unexpectedly produced a valid receipt"))?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn malformed_receipt_is_rejected() -> Result<()> {
        let (_dir, fake) = executable_script(
            r#"#!/bin/sh
cat >/dev/null
printf '%s\n' '{"version":"axiograph-verifier-stdio-v2","nonce":"not-a-uuid","checker_sha256":"0000000000000000000000000000000000000000000000000000000000000000","checker_build_id":"axiograph-verify-main-v3","anchor_digest_v2":"axi:module:v2:sha256:0000000000000000000000000000000000000000000000000000000000000000","certificate_digest_v2":"axi:certificate:v2:sha256:0000000000000000000000000000000000000000000000000000000000000000","prepared_query_digest_v1":"axi:query:v2:sha256:0000000000000000000000000000000000000000000000000000000000000000","answer_digest_v1":"axi:answer:v2:sha256:0000000000000000000000000000000000000000000000000000000000000000","certificate_kind":"query_result_v4","claim_kind":"finite_exact_complete","decision":"accepted","message":"fake"}'
"#,
        )?;
        let module_axi = "module X\n";
        let (certificate, prepared, answer) = anchored_boolean_query_certificate(module_axi)?;
        let config = CertVerifyConfig {
            verifier_bin: Some(fake.clone()),
            timeout: Some(Duration::from_secs(1)),
            approved_checker_sha256: Some(sha256_file(&fake)?),
            approved_checker_build_id: Some("axiograph-verify-main-v3".to_string()),
        };
        let err =
            verify_certificate_with_lean(&config, module_axi, &certificate, &prepared, &answer)
                .err()
                .ok_or_else(|| anyhow!("malformed verifier receipt was accepted"))?;
        assert!(err.to_string().contains("valid V2 receipt"));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn approved_lean_checker_returns_bound_receipt() -> Result<()> {
        let Some((config, module_axi)) = lean_checker_fixture()? else {
            return Ok(());
        };
        let (certificate, prepared, answer) = anchored_boolean_query_certificate(&module_axi)?;
        let receipt =
            verify_certificate_with_lean(&config, &module_axi, &certificate, &prepared, &answer)?;
        assert!(receipt.accepted());
        assert_eq!(receipt.message(), "exact finite query answer verified");
        assert_eq!(receipt.prepared_query_digest_v1(), &prepared);
        assert_eq!(receipt.answer_digest_v1(), &answer);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn query_substitution_and_old_build_id_reject() -> Result<()> {
        let Some((mut config, module_axi)) = lean_checker_fixture()? else {
            return Ok(());
        };
        let (certificate, prepared, answer) = anchored_boolean_query_certificate(&module_axi)?;
        let substituted = QueryIdV2::from_canonical_fields(&[b"different prepared query"]);
        let err =
            verify_certificate_with_lean(&config, &module_axi, &certificate, &substituted, &answer)
                .expect_err("substituted expected prepared digest must reject");
        assert!(err.to_string().contains("differs from caller expectation"));

        config.approved_checker_build_id = Some("obsolete-checker-build".to_string());
        let err =
            verify_certificate_with_lean(&config, &module_axi, &certificate, &prepared, &answer)
                .expect_err("old build id must reject V2 receipt");
        assert!(err.to_string().contains("identity or digest mismatch"));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn unknown_field_upgrade_rejects() -> Result<()> {
        let Some((config, module_axi)) = lean_checker_fixture()? else {
            return Ok(());
        };
        let (certificate, prepared, answer) = anchored_boolean_query_certificate(&module_axi)?;
        let mut value: serde_json::Value = serde_json::from_str(&certificate)?;
        value["upgrade_only"] = serde_json::json!(true);
        let err = verify_certificate_with_lean(
            &config,
            &module_axi,
            &serde_json::to_string(&value)?,
            &prepared,
            &answer,
        )
        .expect_err("unknown-field upgrade must reject");
        assert!(
            err.to_string()
                .contains("invalid query_result_v4 certificate JSON"),
            "unexpected verifier error: {err:#}"
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn stdio_v2_rejects_missing_null_nonstring_and_malformed_expectations() -> Result<()> {
        let Some((config, module_axi)) = lean_checker_fixture()? else {
            return Ok(());
        };
        let verifier = config.verifier_bin.as_ref().expect("fixture verifier");
        let checker_sha = config
            .approved_checker_sha256
            .as_deref()
            .expect("fixture checker SHA");
        let (certificate, prepared, answer) = anchored_boolean_query_certificate(&module_axi)?;
        let base = serde_json::json!({
            "version": VERIFIER_PROTOCOL_V2,
            "nonce": uuid::Uuid::new_v4().to_string(),
            "checker_sha256": checker_sha,
            "module_axi": module_axi,
            "certificate_json": certificate,
            "expected_prepared_query_digest": prepared,
            "expected_answer_digest": answer,
        });
        let mut cases = Vec::new();
        let mut missing = base.clone();
        missing
            .as_object_mut()
            .unwrap()
            .remove("expected_prepared_query_digest");
        cases.push(missing);
        let mut null = base.clone();
        null["expected_prepared_query_digest"] = serde_json::Value::Null;
        cases.push(null);
        let mut nonstring = base.clone();
        nonstring["expected_prepared_query_digest"] = serde_json::json!(17);
        cases.push(nonstring);
        let mut malformed = base.clone();
        malformed["expected_prepared_query_digest"] = serde_json::json!("axi:query:v2:sha256:abc");
        cases.push(malformed);
        let mut old_protocol = base.clone();
        old_protocol["version"] = serde_json::json!("obsolete-verifier-protocol");
        cases.push(old_protocol);
        let mut unknown = base;
        unknown["upgrade_only"] = serde_json::json!(true);
        cases.push(unknown);

        for request in cases {
            let bytes = serde_json::to_vec(&request)?;
            let (status, stdout, _) =
                run_stdio_with_timeout(verifier, &bytes, Duration::from_secs(10))?;
            assert!(!status.success());
            let receipt: serde_json::Value = serde_json::from_slice(&stdout)?;
            assert_eq!(receipt["decision"], "rejected");
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn stdio_v2_rejects_binding_answer_and_nested_field_mutations() -> Result<()> {
        let Some((config, module_axi)) = lean_checker_fixture()? else {
            return Ok(());
        };
        let verifier = config.verifier_bin.as_ref().expect("fixture verifier");
        let checker_sha = config
            .approved_checker_sha256
            .as_deref()
            .expect("fixture checker SHA");
        let (certificate, prepared, answer) = anchored_boolean_query_certificate(&module_axi)?;
        let certificate: serde_json::Value = serde_json::from_str(&certificate)?;
        let mut mutations = Vec::new();
        let mut limit = certificate.clone();
        limit["proof"]["binding"]["row_limit"] = serde_json::json!(0);
        mutations.push(limit);
        let mut truncation = certificate.clone();
        truncation["proof"]["runtime_truncated"] = serde_json::json!(true);
        mutations.push(truncation);
        let mut duplicate = certificate.clone();
        let row = duplicate["proof"]["rows"][0].clone();
        duplicate["proof"]["rows"].as_array_mut().unwrap().push(row);
        mutations.push(duplicate);
        let mut top_unknown = certificate.clone();
        top_unknown["upgrade_only"] = serde_json::json!(true);
        mutations.push(top_unknown);
        let mut proof_unknown = certificate.clone();
        proof_unknown["proof"]["unexpected_proof_field"] = serde_json::json!(true);
        mutations.push(proof_unknown);
        let mut binding_unknown = certificate.clone();
        binding_unknown["proof"]["binding"]["unexpected_binding_field"] = serde_json::json!(true);
        mutations.push(binding_unknown);
        let mut row_unknown = certificate.clone();
        row_unknown["proof"]["rows"][0]["unexpected_row_field"] = serde_json::json!(true);
        mutations.push(row_unknown);
        let mut missing_binding_field = certificate.clone();
        missing_binding_field["proof"]["binding"]
            .as_object_mut()
            .expect("binding object")
            .remove("row_limit");
        mutations.push(missing_binding_field);
        let mut missing_proof_field = certificate.clone();
        missing_proof_field["proof"]
            .as_object_mut()
            .expect("proof object")
            .remove("runtime_truncated");
        mutations.push(missing_proof_field);
        let mut nested_unknown = certificate;
        nested_unknown["proof"]["binding"]["query"]["upgrade_only"] = serde_json::json!(true);
        mutations.push(nested_unknown);

        for certificate in mutations {
            let request = serde_json::json!({
                "version": VERIFIER_PROTOCOL_V2,
                "nonce": uuid::Uuid::new_v4().to_string(),
                "checker_sha256": checker_sha,
                "module_axi": module_axi,
                "certificate_json": serde_json::to_string(&certificate)?,
                "expected_prepared_query_digest": prepared,
                "expected_answer_digest": answer,
            });
            let (status, stdout, _) = run_stdio_with_timeout(
                verifier,
                &serde_json::to_vec(&request)?,
                Duration::from_secs(10),
            )?;
            assert!(!status.success());
            let receipt: serde_json::Value = serde_json::from_slice(&stdout)?;
            assert_eq!(receipt["decision"], "rejected");
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn durable_query_result_v4_exact_and_adversarial_fixtures() -> Result<()> {
        let Some((config, _)) = lean_checker_fixture()? else {
            return Ok(());
        };
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let module_axi = crate::security::read_utf8_file_bounded(
            &repo.join("fixtures/verification/query_result_v4_exact.axi"),
            crate::security::MAX_TEXT_INPUT_BYTES,
            "CLI input",
        )?;
        let verifier = config
            .verifier_bin
            .as_ref()
            .ok_or_else(|| anyhow!("fixture verifier path missing"))?;
        let checker_sha = config
            .approved_checker_sha256
            .as_deref()
            .ok_or_else(|| anyhow!("fixture verifier SHA missing"))?;
        let cases = [
            ("query_result_v4_exact.json", true),
            ("reject/query_result_v4_altered_prepared_digest.json", false),
            ("reject/query_result_v4_altered_answer_digest.json", false),
            ("reject/query_result_v4_forged_witness.json", false),
            ("reject/query_result_v4_extra_row.json", false),
            ("reject/query_result_v4_missing_row.json", false),
            ("reject/query_result_v4_duplicate_row.json", false),
            ("reject/query_result_v4_truncated.json", false),
        ];

        for (relative, should_accept) in cases {
            let certificate_json = crate::security::read_utf8_file_bounded(
                &repo.join("fixtures/verification").join(relative),
                crate::security::MAX_TEXT_INPUT_BYTES,
                "CLI input",
            )?;
            let certificate: serde_json::Value = serde_json::from_str(&certificate_json)?;
            let request = serde_json::json!({
                "version": VERIFIER_PROTOCOL_V2,
                "nonce": uuid::Uuid::new_v4().to_string(),
                "checker_sha256": checker_sha,
                "module_axi": module_axi,
                "certificate_json": certificate_json,
                "expected_prepared_query_digest": certificate["proof"]["prepared_query_digest_v1"],
                "expected_answer_digest": certificate["proof"]["answer_digest_v1"],
            });
            let (status, stdout, _) = run_stdio_with_timeout(
                verifier,
                &serde_json::to_vec(&request)?,
                Duration::from_secs(10),
            )?;
            let receipt: serde_json::Value = serde_json::from_slice(&stdout)?;
            assert_eq!(status.success(), should_accept, "{relative}");
            assert_eq!(
                receipt["decision"],
                if should_accept {
                    "accepted"
                } else {
                    "rejected"
                },
                "{relative}"
            );
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn regulated_shipment_exact_path_query_is_complete_and_missing_rows_reject() -> Result<()> {
        let Some((config, _)) = lean_checker_fixture()? else {
            return Ok(());
        };
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let module_axi = crate::security::read_utf8_file_bounded(
            &repo.join("examples/regulated_shipment/RegulatedShipment.axi"),
            crate::security::MAX_TEXT_INPUT_BYTES,
            "CLI input",
        )?;
        let mut db = axiograph_pathdb::PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(
            &mut db,
            &module_axi,
        )?;
        db.build_indexes();
        let meta = axiograph_pathdb::axi_semantics::MetaPlaneIndex::from_db(&db)?;
        let query: crate::query_ir::QueryIrV1 = serde_json::from_value(serde_json::json!({
            "version": 1,
            "select_vars": ["?certificate"],
            "where_atoms": [{
                "kind": "edge",
                "left": "Shipment_RX_1007",
                "path": "ShipmentContainsBatch/BatchHasCertificate",
                "right": "?certificate"
            }],
            "max_hops": 2,
            "limit": 10
        }))?;
        let mut prepared = query.compile_with_meta(&db, Some(&meta))?;
        let validated = prepared.execute_answer(&db, Some(&meta))?;
        assert_eq!(validated.result().rows.len(), 1);
        assert_eq!(validated.selected_rows_v1()[0].projections.len(), 1);
        assert_eq!(
            validated.selected_rows_v1()[0].projections[0].entity,
            "CoA_RX_42"
        );

        let emitted = prepared.certify_answer_with_anchors(
            validated,
            &db,
            Some(&meta),
            RevisionDigestV2::from_accepted_text(&module_axi),
        )?;
        let prepared_digest = emitted
            .prepared_query_digest_v1()
            .ok_or_else(|| anyhow!("regulated shipment query omitted prepared digest"))?
            .clone();
        let answer_digest = emitted.answer_digest_v1().clone();
        let receipt = verify_certificate_with_lean(
            &config,
            &module_axi,
            emitted.certificate_text(),
            &prepared_digest,
            &answer_digest,
        )?;
        assert!(receipt.accepted(), "{}", receipt.message());
        assert_eq!(receipt.claim_kind, "finite_exact_complete");

        let mut missing = emitted.certificate().clone();
        missing.proof.rows.clear();
        missing.proof.answer_digest_v1 = missing
            .proof
            .recompute_answer_digest_v1()
            .map_err(anyhow::Error::msg)?;
        let missing_answer = missing.proof.answer_digest_v1.clone();
        let missing_receipt = verify_certificate_with_lean(
            &config,
            &module_axi,
            &serde_json::to_string_pretty(&missing)?,
            &prepared_digest,
            &missing_answer,
        )?;
        assert!(
            !missing_receipt.accepted(),
            "trusted finite checker accepted a missing-row answer"
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn approved_lean_checker_matches_prepared_ast_goldens() -> Result<()> {
        let Some((config, _)) = lean_checker_fixture()? else {
            return Ok(());
        };
        let module_axi = r#"module Demo
schema S:
  object Person
  relation Parent(child: Person, parent: Person)
instance I of S:
  Person = {Alice, Bob}
  Parent = {(child=Alice, parent=Bob)}
"#
        .to_string();
        let mut db = axiograph_pathdb::PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(
            &mut db,
            &module_axi,
        )?;
        db.build_indexes();
        let meta = axiograph_pathdb::axi_semantics::MetaPlaneIndex::from_db(&db)?;
        let cases = [
            (
                "finite_type_enumeration",
                serde_json::json!({
                    "version": 1,
                    "select_vars": ["?person"],
                    "where_atoms": [{"kind":"type","term":"?person","type":"Person"}],
                    "limit": 10
                }),
            ),
            (
                "finite_bounded_path",
                serde_json::json!({
                    "version": 1,
                    "select_vars": ["?parent"],
                    "where_atoms": [{"kind":"edge","left":"Alice","path":"Parent","right":"?parent"}],
                    "max_hops": 2,
                    "limit": 10
                }),
            ),
            (
                "finite_disjunction",
                serde_json::json!({
                    "version": 1,
                    "select_vars": ["?person"],
                    "disjuncts": [
                        [{"kind":"attr_eq","term":"?person","key":"name","value":"Alice"}],
                        [{"kind":"attr_eq","term":"?person","key":"name","value":"Bob"}]
                    ],
                    "limit": 10
                }),
            ),
        ];

        for (label, value) in cases {
            let query: crate::query_ir::QueryIrV1 = serde_json::from_value(value)?;
            let mut prepared = query.compile_with_meta(&db, Some(&meta))?;
            let validated = prepared.execute_answer(&db, Some(&meta))?;
            let emitted = prepared.certify_answer_with_anchors(
                validated,
                &db,
                Some(&meta),
                RevisionDigestV2::from_accepted_text(&module_axi),
            )?;
            let certificate_json = emitted.certificate_text().to_string();
            let receipt = verify_certificate_with_lean(
                &config,
                &module_axi,
                &certificate_json,
                emitted
                    .prepared_query_digest_v1()
                    .ok_or_else(|| anyhow!("{label}: missing prepared digest"))?,
                emitted.answer_digest_v1(),
            )?;
            assert!(receipt.accepted(), "{label}: {}", receipt.message());

            if label == "finite_bounded_path" {
                fn forge_first_fact_id(value: &mut serde_json::Value) -> bool {
                    match value {
                        serde_json::Value::Object(object) => {
                            if let Some(fact_id) = object.get_mut("axi_fact_id") {
                                *fact_id = serde_json::json!(format!(
                                    "axi:fact:v2:sha256:{}",
                                    "0".repeat(64)
                                ));
                                true
                            } else {
                                object.values_mut().any(forge_first_fact_id)
                            }
                        }
                        serde_json::Value::Array(values) => {
                            values.iter_mut().any(forge_first_fact_id)
                        }
                        _ => false,
                    }
                }

                let mut forged: serde_json::Value = serde_json::from_str(&certificate_json)?;
                assert!(
                    forge_first_fact_id(&mut forged),
                    "bounded path certificate must contain a canonical fact witness"
                );
                let forged_receipt = verify_certificate_with_lean(
                    &config,
                    &module_axi,
                    &serde_json::to_string(&forged)?,
                    emitted
                        .prepared_query_digest_v1()
                        .ok_or_else(|| anyhow!("{label}: missing prepared digest"))?,
                    emitted.answer_digest_v1(),
                )?;
                assert!(
                    !forged_receipt.accepted(),
                    "forged canonical path fact was accepted: {forged}"
                );
            }
        }
        Ok(())
    }
}
