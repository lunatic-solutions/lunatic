use std::process::Stdio;

use anyhow::{anyhow, Result};
use log::{debug, info};

use crate::mode::deploy::artefact::get_target_dir;

pub(crate) async fn start_build() -> Result<()> {
    let target_dir = get_target_dir();
    info!("Starting build");
    debug!("Executing the command `cargo build --release`");
    std::process::Command::new("cargo")
        .env("CARGO_TARGET_DIR", target_dir)
        .args(["build", "--release"])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
        .map_err(|e| anyhow!("failed to execute build {e:?}"))?;
    info!("Successfully built artefacts");
    Ok(())
}
