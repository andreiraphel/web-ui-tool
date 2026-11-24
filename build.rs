use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo::rustc-check-cfg=cfg(cpseudo_bundled_rg)");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=CPSEUDO_BUNDLE_RG");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let rg_name = if target_os == "windows" {
        "rg.exe"
    } else {
        "rg"
    };
    let vendored = Path::new("vendor").join(rg_name);
    println!("cargo:rerun-if-changed={}", vendored.display());

    let source = env::var_os("CPSEUDO_BUNDLE_RG")
        .map(PathBuf::from)
        .or_else(|| vendored.is_file().then_some(vendored));

    let Some(source) = source else {
        return;
    };

    if !source.is_file() {
        println!(
            "cargo:warning=CPSEUDO_BUNDLE_RG points to {}, which does not exist; \
             building without embedded ripgrep",
            source.display()
        );
        return;
    }

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set"));
    let dest = out_dir.join("rg.exe");
    fs::copy(&source, &dest).expect("copy ripgrep for embedding");
    println!("cargo:rustc-cfg=cpseudo_bundled_rg");
}
