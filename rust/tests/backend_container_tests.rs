//! Container-backed backend readback tests for the current advanced backend targets.
//!
//! These tests are intentionally ignored by default because they require:
//! - Docker + docker compose
//! - pulling third-party backend images
//! - a slower startup path than ordinary crate/integration tests
//!
//! Run them with:
//!   AXIOGRAPH_RUN_BACKEND_CONTAINER_TESTS=1 cargo test --test backend_container_tests -- --ignored --nocapture

use std::env;
use std::error::Error;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

const RUN_ENV: &str = "AXIOGRAPH_RUN_BACKEND_CONTAINER_TESTS";
const KEEP_ENV: &str = "AXIOGRAPH_KEEP_BACKEND_TEST_CONTAINERS";
const COMPOSE_RELATIVE_PATH: &str = "tests/docker/graph_backends.compose.yml";

#[derive(Debug)]
struct DockerComposeFixture {
    compose_file: PathBuf,
    project_name: String,
    keep_containers: bool,
}

#[derive(Debug)]
struct CommandResult {
    stdout: String,
    stderr: String,
}

impl DockerComposeFixture {
    fn new() -> TestResult<Self> {
        let cargo_manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let compose_file = cargo_manifest_dir.join(COMPOSE_RELATIVE_PATH);
        if !compose_file.exists() {
            return Err(format!("missing compose file `{}`", compose_file.display()).into());
        }

        let keep_containers = env::var_os(KEEP_ENV).is_some();
        let unique_suffix = unique_suffix();
        let project_name = format!("axiograph_backends_{unique_suffix}");

        Ok(Self {
            compose_file,
            project_name,
            keep_containers,
        })
    }

    fn up(&self) -> TestResult {
        self.compose_checked(["up", "-d"])?;
        Ok(())
    }

    fn compose_checked<const N: usize>(&self, args: [&str; N]) -> TestResult<CommandResult> {
        let output = self.compose(args)?.output()?;
        Self::decode_output(args.as_slice(), output)
    }

    fn compose_exec_checked(&self, service: &str, cmd: &[&str]) -> TestResult<CommandResult> {
        let mut command = self.compose(["exec", "-T", service])?;
        command.args(cmd);
        let output = command.output()?;
        Self::decode_output(cmd, output)
    }

    fn wait_for_service(
        &self,
        label: &str,
        timeout: Duration,
        probe: impl Fn() -> TestResult,
    ) -> TestResult {
        let start = Instant::now();
        let mut last_error: Option<String> = None;
        while start.elapsed() < timeout {
            match probe() {
                Ok(()) => return Ok(()),
                Err(err) => {
                    last_error = Some(err.to_string());
                    sleep(Duration::from_secs(2));
                }
            }
        }

        Err(format!(
            "timed out waiting for {label} after {:?}: {}",
            timeout,
            last_error.unwrap_or_else(|| "unknown error".to_string())
        )
        .into())
    }

    fn typedb_readback(&self) -> TestResult {
        self.wait_for_service("TypeDB HTTP endpoint", Duration::from_secs(90), || {
            let base_url = self.typedb_http_base_url()?;
            let _ = self.typedb_sign_in(&base_url)?;
            Ok(())
        })?;

        let base_url = self.typedb_http_base_url()?;
        let token = self.typedb_sign_in(&base_url)?;
        let db_name = format!("axiograph_backend_readback_{}", unique_suffix());
        self.typedb_post_empty(&base_url, &token, &format!("/v1/databases/{db_name}"))?;
        self.typedb_post_json(
            &base_url,
            &token,
            "/v1/query",
            json!({
                "databaseName": db_name,
                "transactionType": "schema",
                "commit": true,
                "query": "define attribute name value string; entity node, owns name;"
            }),
        )?;
        self.typedb_post_json(
            &base_url,
            &token,
            "/v1/query",
            json!({
                "databaseName": db_name,
                "transactionType": "write",
                "commit": true,
                "query": "insert $n isa node, has name \"typedb-readback\";"
            }),
        )?;
        let query = self.typedb_post_json(
            &base_url,
            &token,
            "/v1/query",
            json!({
                "databaseName": db_name,
                "transactionType": "read",
                "query": "match $n isa node, has name $name; fetch { \"name\": $name };"
            }),
        )?;

        if !query.to_string().contains("typedb-readback") {
            return Err(format!(
                "TypeDB readback query did not surface inserted data; output was:\n{query}"
            )
            .into());
        }
        Ok(())
    }

    fn terminusdb_readback(&self) -> TestResult {
        self.wait_for_service("TerminusDB CLI", Duration::from_secs(90), || {
            let _ =
                self.compose_exec_checked("terminusdb", &["./terminusdb", "list", "-b", "-j"])?;
            Ok(())
        })?;

        let database_spec = format!("admin/axiograph-readback-{}", unique_suffix());
        let _ = self.compose_exec_checked(
            "terminusdb",
            &["./terminusdb", "db", "create", &database_spec],
        )?;
        let listed =
            self.compose_exec_checked("terminusdb", &["./terminusdb", "list", "-b", "-j"])?;
        let combined = format!("{}\n{}", listed.stdout, listed.stderr);
        if !combined.contains(&database_spec) {
            return Err(format!(
                "TerminusDB list output did not include `{database_spec}`; output was:\n{combined}"
            )
            .into());
        }
        Ok(())
    }

    fn compose<const N: usize>(&self, args: [&str; N]) -> TestResult<Command> {
        let mut command = Command::new("docker");
        command.arg("compose");
        command.arg("-p").arg(&self.project_name);
        command.arg("-f").arg(&self.compose_file);
        command.args(args);
        Ok(command)
    }

    fn typedb_http_base_url(&self) -> TestResult<String> {
        let port = self.compose_checked(["port", "typedb", "8000"])?;
        let endpoint = port
            .stdout
            .lines()
            .last()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .ok_or_else(|| {
                "docker compose port did not return a published TypeDB HTTP port".to_string()
            })?;
        Ok(format!("http://{endpoint}"))
    }

    fn typedb_sign_in(&self, base_url: &str) -> TestResult<String> {
        let body = self.typedb_post_json_unauthenticated(
            base_url,
            "/v1/signin",
            json!({
                "username": "admin",
                "password": "password"
            }),
        )?;
        let token = body
            .get("token")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("TypeDB sign-in response did not include a token: {body}"))?;
        Ok(token.to_string())
    }

    fn typedb_post_empty(&self, base_url: &str, token: &str, path: &str) -> TestResult {
        let url = format!("{base_url}{path}");
        tokio::runtime::Runtime::new()?.block_on(async {
            let client = reqwest::Client::new();
            let response = client
                .post(&url)
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .send()
                .await?;
            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                return Err(format!("TypeDB POST {path} failed with {status}: {body}").into());
            }
            Ok::<(), Box<dyn Error>>(())
        })
    }

    fn typedb_post_json(
        &self,
        base_url: &str,
        token: &str,
        path: &str,
        payload: Value,
    ) -> TestResult<Value> {
        let url = format!("{base_url}{path}");
        tokio::runtime::Runtime::new()?.block_on(async {
            let client = reqwest::Client::new();
            let response = client
                .post(&url)
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .header(CONTENT_TYPE, "application/json")
                .json(&payload)
                .send()
                .await?;
            let status = response.status();
            let body = response.text().await?;
            if !status.is_success() {
                return Err(format!("TypeDB POST {path} failed with {status}: {body}").into());
            }
            let json: Value = serde_json::from_str(&body)?;
            Ok::<Value, Box<dyn Error>>(json)
        })
    }

    fn typedb_post_json_unauthenticated(
        &self,
        base_url: &str,
        path: &str,
        payload: Value,
    ) -> TestResult<Value> {
        let url = format!("{base_url}{path}");
        tokio::runtime::Runtime::new()?.block_on(async {
            let client = reqwest::Client::new();
            let response = client
                .post(&url)
                .header(CONTENT_TYPE, "application/json")
                .json(&payload)
                .send()
                .await?;
            let status = response.status();
            let body = response.text().await?;
            if !status.is_success() {
                return Err(format!("TypeDB POST {path} failed with {status}: {body}").into());
            }
            let json: Value = serde_json::from_str(&body)?;
            Ok::<Value, Box<dyn Error>>(json)
        })
    }

    fn decode_output(args: &[&str], output: Output) -> TestResult<CommandResult> {
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if !output.status.success() {
            return Err(format!(
                "docker compose command failed for `{}`\nstdout:\n{}\nstderr:\n{}",
                args.join(" "),
                stdout,
                stderr
            )
            .into());
        }
        Ok(CommandResult { stdout, stderr })
    }
}

impl Drop for DockerComposeFixture {
    fn drop(&mut self) {
        if self.keep_containers {
            eprintln!(
                "keeping backend test containers for compose project `{}` because {} is set",
                self.project_name, KEEP_ENV
            );
            return;
        }

        let _ = Command::new("docker")
            .arg("compose")
            .arg("-p")
            .arg(&self.project_name)
            .arg("-f")
            .arg(&self.compose_file)
            .args(["down", "-v", "--remove-orphans"])
            .output();
    }
}

#[test]
#[ignore = "requires Docker images and explicit opt-in via AXIOGRAPH_RUN_BACKEND_CONTAINER_TESTS=1"]
fn graph_backend_containers_boot_and_answer_readback_operations() -> TestResult {
    if env::var_os(RUN_ENV).is_none() {
        eprintln!("skipping backend container tests because {RUN_ENV} is not set");
        return Ok(());
    }

    let fixture = DockerComposeFixture::new()?;
    fixture.up()?;
    fixture.typedb_readback()?;
    fixture.terminusdb_readback()?;
    Ok(())
}

fn unique_suffix() -> String {
    let pid = std::process::id();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0));
    format!("{pid}_{:x}", now.as_nanos())
}
