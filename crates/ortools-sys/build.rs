fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-changed=wrapper.cc");

    if std::env::var("CARGO_FEATURE_LINK").is_ok() {
        build_with_ortools();
    }
}

fn build_with_ortools() {
    use std::{env, path::PathBuf};

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let workspace_root = manifest_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    let ortools_build = env::var("FERROX_ORTOOLS_ROOT").map_or_else(
        |_| workspace_root.join("vendor/ortools/build"),
        PathBuf::from,
    );

    let ortools_src = ortools_build.parent().unwrap().to_path_buf();

    assert!(
        ortools_build.exists(),
        "OR-Tools build not found at {}.\nRun `make ortools` from the ferrox workspace root, or set FERROX_ORTOOLS_ROOT.",
        ortools_build.display()
    );

    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .file("wrapper.cc")
        .include(&ortools_src)
        .include(&ortools_build)
        .include(ortools_build.join("_deps/absl-src"))
        .include(ortools_build.join("_deps/protobuf-src/src"))
        .include(ortools_build.join("_deps/protobuf-src/third_party/utf8_range"))
        .define("OR_PROTO_DLL", "")
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-deprecated-declarations")
        .compile("ortools_wrapper");

    let lib_dir = ortools_build.join("lib");
    copy_runtime_libraries(&lib_dir, &out_dir);
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:LIB_DIR={}", lib_dir.display());

    // v9.15 exposes Abseil/Protobuf symbols through wrapper object code, so
    // link the shared dependencies declared by libortools as direct inputs.
    println!("cargo:rustc-link-lib=dylib=ortools");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", out_dir.display());
    link_ortools_dylib_dependencies(&lib_dir);

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
        let is_runtime = name.starts_with("lib") && (is_dylib || name.contains(".so"));
        if is_runtime {
            let _ = std::fs::copy(&path, out_dir.join(name));
        }
    }
}

fn link_ortools_dylib_dependencies(lib_dir: &std::path::Path) {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        let libortools = lib_dir.join("libortools.dylib");
        let Ok(output) = Command::new("otool").arg("-L").arg(&libortools).output() else {
            return;
        };
        if !output.status.success() {
            return;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut linked = std::collections::BTreeSet::new();
        for line in stdout.lines().skip(1) {
            let Some(path) = line.split_whitespace().next() else {
                continue;
            };
            let Some(file_name) = std::path::Path::new(path).file_name() else {
                continue;
            };
            let Some(file_name) = file_name.to_str() else {
                continue;
            };
            let is_dylib = std::path::Path::new(file_name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("dylib"));
            if !file_name.starts_with("lib") || !is_dylib {
                continue;
            }
            let Some(lib_name) = file_name
                .strip_prefix("lib")
                .and_then(|name| name.strip_suffix(".dylib"))
                .and_then(|name| name.split('.').next())
            else {
                continue;
            };
            if matches!(lib_name, "ortools" | "c++" | "System") || !linked.insert(lib_name) {
                continue;
            }
            println!("cargo:rustc-link-lib=dylib={lib_name}");
        }
    }
}
