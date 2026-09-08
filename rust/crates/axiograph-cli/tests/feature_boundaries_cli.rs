//! Exercise the built CLI, not a second provider dispatcher. No provider credentials
//! or external endpoints are used; unavailable providers must not contact the canary.
use anyhow::Result;
use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Output, Stdio};

fn cli() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_axiograph"));
    command.env_clear().env("AXIOGRAPH_LLM_TIMEOUT_SECS", "1");
    command
}

fn request() -> Value {
    let source = "module FeatureTest\nschema S:\n  object A\n";
    json!({
        "protocol": axiograph_cli::proposal_adapter_boundary::PREDICTIVE_PROPOSAL_PROTOCOL_V1,
        "trace_id": "proposal::feature-test", "generated_at_unix_secs": 1,
        "input": {"axi_module_text": source,
                  "revision_digest_v2": axiograph_pathdb::AxiDigest::from_axi_text(source)},
        "options": {"max_new_proposals": 1}
    })
}

fn run_request(command: &mut Command, request: &Value) -> Result<Output> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(&serde_json::to_vec(request)?)?;
    Ok(child.wait_with_output()?)
}

fn proposals(path: &std::path::Path) -> Result<()> {
    std::fs::write(
        path,
        serde_json::to_vec(&json!({
            "version": 1, "generated_at": "0",
            "source": {"source_type": "test", "locator": "feature_boundaries_cli"},
            "proposals": []
        }))?,
    )?;
    Ok(())
}

#[test]
fn predictive_mock_validates_exact_source_before_returning_evidence() -> Result<()> {
    let mut req = request();
    let run = |req: &Value| {
        run_request(
            cli().args(["ingest", "predictive-proposals-llm", "--backend", "mock"]),
            req,
        )
    };
    let output = run(&req)?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(response["trace_id"], req["trace_id"]);
    assert_eq!(response["notes"], json!(["mock backend (no proposals)"]));
    assert_eq!(response["proposals"]["proposals"], json!([]));
    req["input"]["axi_module_text"] =
        json!("module FeatureTest\nschema S:\n  object A\n-- changed bytes\n");
    let rejected = run(&req)?;
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr)
        .contains("does not match canonical `.axi` digest"));
    assert!(rejected.stdout.is_empty());
    Ok(())
}

#[test]
fn discovery_without_provider_still_writes_candidates_and_rejects_conflicts() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let input = temp.path().join("proposals.json");
    proposals(&input)?;
    for action in ["draft-module", "augment-proposals"] {
        let out = temp.path().join(action);
        let output = cli()
            .args(["discover", action])
            .arg(&input)
            .arg("--out")
            .arg(&out)
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!std::fs::read(&out)?.is_empty());
        let bytes = std::fs::read(&out)?;
        let rejected = cli()
            .args(["discover", action])
            .arg(&input)
            .arg("--out")
            .arg(&out)
            .args(["--llm-openai", "--llm-ollama"])
            .output()?;
        assert!(!rejected.status.success());
        assert!(String::from_utf8_lossy(&rejected.stderr)
            .contains("choose at most one LLM integration"));
        assert_eq!(std::fs::read(&out)?, bytes);
    }
    Ok(())
}

#[cfg(any(
    not(feature = "llm-ollama"),
    not(feature = "llm-openai"),
    not(feature = "llm-anthropic")
))]
fn unavailable_provider(provider: &str, endpoint_flag: &str) -> Result<()> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let endpoint = format!("http://{}", listener.local_addr()?);
    let temp = tempfile::tempdir()?;
    let input = temp.path().join("proposals.json");
    proposals(&input)?;
    let expected = format!(
        "{provider} support not compiled (enable `axiograph-cli` feature `llm-{provider}`)"
    );
    for action in ["draft-module", "augment-proposals"] {
        let out = temp.path().join(action);
        std::fs::write(&out, b"output sentinel")?;
        let output = cli()
            .args(["discover", action])
            .arg(&input)
            .arg("--out")
            .arg(&out)
            .arg(format!("--llm-{provider}"))
            .arg(format!("--llm-{endpoint_flag}"))
            .arg(&endpoint)
            .args(["--llm-model", "not-a-real-model", "--llm-timeout-secs", "1"])
            .output()?;
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(&expected),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(std::fs::read(&out)?, b"output sentinel");
        assert!(!temp.path().join(format!("{action}.trace.json")).exists());
    }
    let output = run_request(
        cli()
            .args(["ingest", "predictive-proposals-llm", "--backend", provider])
            .arg(format!("--{endpoint_flag}"))
            .arg(&endpoint)
            .args(["--model", "not-a-real-model"]),
        &request(),
    )?;
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(&expected),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert_eq!(
        listener
            .accept()
            .expect_err("unavailable provider must not open a connection")
            .kind(),
        std::io::ErrorKind::WouldBlock
    );
    Ok(())
}

#[cfg(not(feature = "llm-ollama"))]
#[test]
fn unavailable_ollama_fails_at_production_entrypoints_without_network_or_output_mutation(
) -> Result<()> {
    unavailable_provider("ollama", "ollama-host")
}

#[cfg(not(feature = "llm-openai"))]
#[test]
fn unavailable_openai_fails_at_production_entrypoints_without_network_or_output_mutation(
) -> Result<()> {
    unavailable_provider("openai", "openai-base-url")
}

#[cfg(not(feature = "llm-anthropic"))]
#[test]
fn unavailable_anthropic_fails_at_production_entrypoints_without_network_or_output_mutation(
) -> Result<()> {
    unavailable_provider("anthropic", "anthropic-base-url")
}
