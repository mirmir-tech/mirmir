use std::{fs, path::Path, process::Command};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    build_dashboard()?;
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    let mut prost = prost_build::Config::new();
    prost.protoc_executable(protoc);
    prost.type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]");
    tonic_prost_build::configure().compile_with_config(
        prost,
        &["proto/mirmir.proto"],
        &["proto"],
    )?;
    println!("cargo:rerun-if-changed=proto/mirmir.proto");
    Ok(())
}

fn build_dashboard() -> Result<(), Box<dyn std::error::Error>> {
    for path in ["web/Cargo.toml", "web/Cargo.lock", "web/Trunk.toml", "web/index.html"] {
        println!("cargo:rerun-if-changed={path}");
    }
    watch_tree(Path::new("web/src"))?;
    let status = Command::new("trunk")
        .args(["build", "--release"])
        .current_dir(Path::new("web"))
        .status()
        .map_err(|error| format!("Trunk is required to build the Leptos dashboard: {error}"))?;
    if !status.success() {
        return Err("building the Leptos dashboard with Trunk failed".into());
    }
    Ok(())
}

fn watch_tree(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    for entry in fs::read_dir(path)? {
        let path = entry?.path();
        if path.is_dir() {
            watch_tree(&path)?;
        } else {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
    Ok(())
}
