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

    // Emit the encoded FileDescriptorSet so tonic-reflection can serve
    // grpc.reflection.v1.ServerReflection. The path is read at compile time
    // via `tonic::include_file_descriptor_set!("ferrox_descriptor")` in main.rs.
    let descriptor_path = std::path::PathBuf::from(std::env::var("OUT_DIR")?)
        .join("ferrox_descriptor.bin");

    tonic_prost_build::configure()
        .build_server(true)
        .build_client(false)
        .file_descriptor_set_path(&descriptor_path)
        .compile_protos(&[proto], &[proto_dir])?;

    Ok(())
}
