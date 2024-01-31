use anyhow::{Context, Result};
use std::process::Command;

/// Build a wasm file from a cargo project.
///
/// This function will block the current thread until the build is finished.
pub fn build(cargo: &str, script: &str) -> Result<Vec<u8>> {
    let cargo_toml = format!(
        r#"
[package]
name = "udf"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies.arrow-udf]
version = "0.1"

[dependencies.genawaiter]
version = "0.99"

{cargo}"#,
    );

    // create a new cargo package at temporary directory
    let dir = tempfile::tempdir()?;
    std::fs::create_dir(dir.path().join("src"))?;
    std::fs::write(dir.path().join("src/lib.rs"), script)?;
    std::fs::write(dir.path().join("Cargo.toml"), cargo_toml)?;

    let output = Command::new("cargo")
        .arg("build")
        .arg("--release")
        .arg("--target")
        .arg("wasm32-wasi")
        .current_dir(dir.path())
        .output()
        .context("failed to run cargo build")?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "failed to build wasm ({})\n--- stdout\n{}\n--- stderr\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let binary_path = dir.path().join("target/wasm32-wasi/release/udf.wasm");
    println!("binary_path: {:?}", binary_path);
    // strip the wasm binary if wasm-tools exists
    if Command::new("wasm-strip").arg("--version").output().is_ok() {
        let output = Command::new("wasm-strip")
            .arg(&binary_path)
            .output()
            .context("failed to strip wasm")?;
        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "failed to strip wasm. ({})\n--- stdout\n{}\n--- stderr\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    let binary = std::fs::read(binary_path).context("failed to read wasm binary")?;
    Ok(binary)
}
