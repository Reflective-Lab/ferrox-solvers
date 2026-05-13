fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let proto_dir = manifest_dir.join("../../proto");
    let proto = proto_dir.join("ferrox.proto");

    println!("cargo:rerun-if-changed={}", proto.display());

    // Propagate rpath from highs-sys / ortools-sys to this binary.
    for dep in ["DEP_HIGHS_LIB_DIR", "DEP_ORTOOLS_LIB_DIR"] {
        if let Ok(lib_dir) = std::env::var(dep) {
            println!("cargo:rustc-link-arg=-Wl,-rpath,{lib_dir}");
        }
    }

    tonic_prost_build::configure()
        .build_server(true)
        .build_client(false)
        .compile_protos(&[proto], &[proto_dir])?;

    Ok(())
}
