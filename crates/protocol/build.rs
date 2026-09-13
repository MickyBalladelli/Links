fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = "../../proto";
    let files = [
        "user",
        "media",
        "receipts",
        "message",
        "envelope",
        "sync",
        "transport",
        "prekeys",
        "queue",
    ]
    .map(|name| format!("{root}/links/v1/{name}.proto"));
    for file in &files {
        println!("cargo:rerun-if-changed={file}");
    }
    let mut config = prost_build::Config::new();
    // These types can hold plaintext, identity credentials, or access tokens.
    config.skip_debug(["."]);
    config.protoc_executable(protoc_bin_vendored::protoc_bin_path()?);
    config.file_descriptor_set_path(
        std::path::PathBuf::from(std::env::var("OUT_DIR")?).join("links.bin"),
    );
    config.compile_protos(&files, &[root])?;
    Ok(())
}
