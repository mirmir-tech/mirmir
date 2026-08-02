use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

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
    let output = PathBuf::from(env::var("OUT_DIR")?).join("dashboard");
    println!("cargo:rerun-if-env-changed=MIRMIR_REBUILD_DASHBOARD");
    if env::var_os("MIRMIR_REBUILD_DASHBOARD").is_none() {
        return copy_dashboard(Path::new("src/web/dashboard"), &output);
    }
    rebuild_dashboard()?;
    copy_dashboard(Path::new("web/dist"), &output)
}

fn rebuild_dashboard() -> Result<(), Box<dyn std::error::Error>> {
    if !Path::new("web/src").exists() {
        return Err("dashboard sources are not included in this package".into());
    }
    for path in ["web/Cargo.toml", "web/Cargo.lock", "web/Trunk.toml", "web/index.html"] {
        println!("cargo:rerun-if-changed={path}");
    }
    watch_tree(Path::new("web/src"))?;
    let status = Command::new("trunk")
        .args(["build", "--release"])
        .current_dir(Path::new("web"))
        .status();
    let status = match status {
        Ok(status) => status,
        Err(error) => {
            return Err(format!("Trunk is required to build the Leptos dashboard: {error}").into());
        },
    };
    if !status.success() {
        return Err("building the Leptos dashboard with Trunk failed".into());
    }
    Ok(())
}

fn copy_dashboard(source: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(output)?;
    for name in ["index.html", "mirmir-dashboard.js", "mirmir-dashboard_bg.wasm"] {
        println!("cargo:rerun-if-changed={}", source.join(name).display());
        fs::copy(source.join(name), output.join(name))?;
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
