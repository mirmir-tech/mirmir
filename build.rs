fn main() -> Result<(), Box<dyn std::error::Error>> {
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
