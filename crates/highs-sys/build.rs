fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=highs_wrapper.h");
    println!("cargo:rerun-if-changed=highs_wrapper.cc");

    if std::env::var("CARGO_FEATURE_LINK").is_ok() {
        build_with_highs();
    }
}

fn build_with_highs() {
    use std::{env, path::PathBuf};

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let highs_build = env::var("FERROX_HIGHS_ROOT")
        .map_or_else(|_| workspace_root.join("vendor/highs/build"), PathBuf::from);

    let highs_src = highs_build.parent().unwrap().to_path_buf();

    assert!(
        highs_build.exists(),
        "HiGHS build not found at {}.\nRun `make highs` from the ferrox workspace root, or set FERROX_HIGHS_ROOT.",
        highs_build.display()
    );

    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .file("highs_wrapper.cc")
        .include(highs_src.join("highs"))
        .include(&highs_build)
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-deprecated-declarations")
        .compile("highs_wrapper");

    let lib_dir = highs_build.join("lib");
    copy_runtime_libraries(&lib_dir, &out_dir);
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:LIB_DIR={}", lib_dir.display());
    println!("cargo:rustc-link-lib=dylib=highs");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", out_dir.display());

    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-lib=c++");
    #[cfg(target_os = "linux")]
    println!("cargo:rustc-link-lib=stdc++");
}

fn copy_runtime_libraries(lib_dir: &std::path::Path, out_dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(lib_dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };

        let is_dylib = path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("dylib"));
        let is_highs_runtime = name.starts_with("libhighs") && (is_dylib || name.contains(".so"));
        if is_highs_runtime {
            let _ = std::fs::copy(&path, out_dir.join(name));
        }
    }
}
