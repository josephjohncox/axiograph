//! Explicit integration driver: production TS client + actual built bundle over
//! genuine temporary repository-bound materialization receipts. Not a browser,
//! checker or pinned-release gate. Default cargo tests do not run this driver.
#[path = "../tests/support/db_server_fixture.rs"]
mod fixture;
use anyhow::{anyhow, Context, Result};
use axiograph_store::AxpdLimits;
use fixture::*;
use std::fs;
use std::process::{Command, Stdio};

fn main() -> Result<()> {
    let frontend = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../frontend/viz");
    if !axiograph_bin().is_file()
        || !frontend.join("dist/index.html").is_file()
        || !frontend.join("node_modules/esbuild/package.json").is_file()
    {
        return Err(anyhow!("Prerequisites missing: build axiograph; npm ci --ignore-scripts and npm run build in frontend/viz. No skipped-success path."));
    }
    let node = Command::new("node")
        .arg("--version")
        .output()
        .context("Node is required for this explicitly selected integration driver")?;
    if !node.status.success() {
        return Err(anyhow!("Node prerequisite failed"));
    }
    println!(
        "Available tool (not pinned release approval): {}",
        String::from_utf8_lossy(&node.stdout).trim()
    );
    for (oversized_ui, missing_assets) in [(false, false), (true, false), (false, true)] {
        let store = tempfile::tempdir()?;
        let axi_store = init_store(store.path(), "finite-client");
        let receipt = axi_store.publish_axpd(
            finite_client_spec("finite-client", oversized_ui),
            &AxpdLimits::default(),
        )?;
        let ready = store.path().join("ready.json");
        let asset_root = tempfile::tempdir()?;
        if missing_assets {
            let dist = asset_root.path().join("frontend/viz/dist");
            fs::create_dir_all(&dist)?;
            fs::write(
                dist.join("index.html"),
                "<head><script type=\"module\" src=\"./missing.js\"></script></head><body></body>",
            )?;
        }
        let mut child = ServerChild(
            Command::new(axiograph_bin())
                .current_dir(asset_root.path())
                .args([
                    "db",
                    "serve",
                    "--dir",
                    store
                        .path()
                        .to_str()
                        .ok_or_else(|| anyhow!("non-UTF8 temporary path"))?,
                    "--materialization",
                    receipt.materialization_id.as_str(),
                    "--listen",
                    "127.0.0.1:0",
                    "--ready-file",
                    ready
                        .to_str()
                        .ok_or_else(|| anyhow!("non-UTF8 ready path"))?,
                ])
                .stdout(Stdio::null())
                .spawn()?,
        );
        let payload = wait_for_ready(&ready, &mut child.0);
        let listen = payload["listen"]
            .as_str()
            .ok_or_else(|| anyhow!("missing listener address"))?;
        let output = Command::new("node")
            .current_dir(&frontend)
            .args([
                "tests/read-only-client-live.mjs",
                &format!("http://{listen}"),
                if missing_assets {
                    "ui-missing-assets"
                } else if oversized_ui {
                    "ui-unavailable"
                } else {
                    "ui-available"
                },
            ])
            .output()?;
        println!("{}", String::from_utf8_lossy(&output.stdout));
        if !output.status.success() {
            return Err(anyhow!(
                "Production client workflow failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    Ok(())
}
