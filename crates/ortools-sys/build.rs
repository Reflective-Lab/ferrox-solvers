fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-changed=wrapper.cc");
    println!("cargo:rerun-if-env-changed=FERROX_ORTOOLS_ROOT");

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

    let external_root = env::var("FERROX_ORTOOLS_ROOT").ok();
    let ortools_build = external_root.as_deref().map_or_else(
        || workspace_root.join("vendor/ortools/build"),
        PathBuf::from,
    );

    let ortools_src = ortools_build.parent().unwrap().to_path_buf();
    emit_identity_metadata(
        external_root.is_some(),
        &ortools_src,
        "FERROX_ORTOOLS_SOURCE_MODE",
        "FERROX_ORTOOLS_SOURCE_COMMIT",
    );

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
    //
    // Linking mode: prefer the static archive (libortools.a) if present —
    // that's what containerized deploys build (BUILD_SHARED_LIBS=OFF), since
    // the shared-lib chain has init-ordering problems in some sandboxed
    // runtimes (std::bad_alloc at static init under Cloud Run, 2026-06-13).
    // Fall back to dylib for macOS dev where Karl's local Makefile builds
    // shared.
    let prefer_static = lib_dir.join("libortools.a").exists();
    if prefer_static {
        println!("cargo:rustc-link-lib=static=ortools");
    } else {
        println!("cargo:rustc-link-lib=dylib=ortools");
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", out_dir.display());
        link_ortools_dylib_dependencies(&lib_dir);
    }

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

fn emit_identity_metadata(
    external_root: bool,
    source_dir: &std::path::Path,
    mode_env: &str,
    commit_env: &str,
) {
    let source_mode = if external_root {
        "external-root"
    } else {
        "vendored"
    };
    let source_commit = git_head(source_dir).unwrap_or_else(|| "unavailable".to_string());
    println!("cargo:rustc-env={mode_env}={source_mode}");
    println!("cargo:rustc-env={commit_env}={source_commit}");
}

fn git_head(source_dir: &std::path::Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(source_dir)
        .arg("rev-parse")
        .arg("HEAD")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let commit = String::from_utf8(output.stdout).ok()?;
    let commit = commit.trim();
    (!commit.is_empty()).then(|| commit.to_string())
}

// Both macOS and Linux branches consume `lib_dir` to walk libortools'
// transitive deps. macOS uses `otool -L` on libortools.dylib; Linux uses
// `readelf -d` on libortools.so to read DT_NEEDED entries. Windows is
// currently unhandled (would need dumpbin.exe).
#[cfg_attr(
    not(any(target_os = "macos", target_os = "linux")),
    allow(unused_variables)
)]
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

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;

        let libortools = lib_dir.join("libortools.so");
        let Ok(output) = Command::new("readelf").arg("-d").arg(&libortools).output() else {
            return;
        };
        if !output.status.success() {
            return;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut linked = std::collections::BTreeSet::new();
        for line in stdout.lines() {
            // readelf -d emits lines like:
            //   0x0000000000000001 (NEEDED)  Shared library: [libabsl_log_internal_message.so.2508.0.0]
            let Some(idx) = line.find("Shared library: [") else {
                continue;
            };
            let after = &line[idx + "Shared library: [".len()..];
            let Some(file_name) = after.strip_suffix(']') else {
                continue;
            };
            // libabsl_log_internal_message.so.2508.0.0 → absl_log_internal_message
            let Some(lib_name) = file_name
                .strip_prefix("lib")
                .and_then(|s| s.split(".so").next())
            else {
                continue;
            };
            // Skip self + libs rustc/gcc handle by default.
            if matches!(
                lib_name,
                "ortools"
                    | "stdc++"
                    | "gcc_s"
                    | "c"
                    | "m"
                    | "pthread"
                    | "dl"
                    | "rt"
                    | "util"
                    | "ld-linux-x86-64"
            ) || !linked.insert(lib_name.to_string())
            {
                continue;
            }
            println!("cargo:rustc-link-lib=dylib={lib_name}");
        }
    }
}
