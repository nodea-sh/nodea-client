//! Windows (MSVC) only: link the cpp-httplib library llama.cpp's build made.
//!
//! llama-cpp-sys-4 0.11.0 builds llama.cpp's `common` library, which links
//! cpp-httplib privately for its model-download code, but it only links the
//! libraries CMake *installs*, and cpp-httplib stays in the build tree. The
//! GNU and Apple linkers drop common's unused download objects, so this only
//! breaks MSVC, as LNK1120 on `httplib::Client` symbols. We never call that
//! code; it just has to resolve.
//!
//! The sys crate puts its CMake tree under `%LOCALAPPDATA%\llcb\<hash>` (its
//! MAX_PATH workaround), keyed by a hash we cannot reproduce, so this takes the
//! most recently built `cpp-httplib.lib` there. Cargo runs this script after
//! the sys crate's (it declares `links = "llama"`), so that is the one just
//! built. Delete this file once the sys crate links cpp-httplib itself.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if !std::env::var("TARGET")
        .unwrap_or_default()
        .contains("windows-msvc")
    {
        return;
    }

    let base = ["LOCALAPPDATA", "TEMP", "TMP"]
        .iter()
        .find_map(|var| std::env::var_os(var).map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("C:\\Temp"));

    let mut newest: Option<(SystemTime, PathBuf)> = None;
    if let Ok(builds) = std::fs::read_dir(base.join("llcb")) {
        for build in builds.flatten() {
            let vendor = build
                .path()
                .join("build")
                .join("vendor")
                .join("cpp-httplib");
            find_lib(&vendor, &mut newest);
        }
    }

    match newest {
        Some((_, lib)) => {
            let dir = lib.parent().unwrap();
            println!("cargo:rustc-link-search=native={}", dir.display());
            println!("cargo:rustc-link-lib=static=cpp-httplib");
            // cpp-httplib's own system dependency.
            println!("cargo:rustc-link-lib=ws2_32");
        }
        None => println!(
            "cargo:warning=no cpp-httplib.lib under {}; linking may fail with LNK1120",
            base.join("llcb").display()
        ),
    }
}

fn find_lib(dir: &Path, newest: &mut Option<(SystemTime, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            find_lib(&path, newest);
        } else if path
            .file_name()
            .is_some_and(|n| n.eq_ignore_ascii_case("cpp-httplib.lib"))
        {
            let modified = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            if newest.as_ref().is_none_or(|(t, _)| modified > *t) {
                *newest = Some((modified, path));
            }
        }
    }
}
