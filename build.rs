//! Builds the non-Rust kernel sources (xlang `.xl`, C `.c`) into objects and
//! links them into the kernel. `src/boot` is the UEFI loader, built separately.

use std::path::{Path, PathBuf};
use std::process::Command;

const DIRS: &[&str] = &["src/kernel", "src/hal", "src/arch", "src/drivers"];

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if matches!(p.extension().and_then(|s| s.to_str()), Some("xl" | "c")) {
            out.push(p);
        }
    }
}

fn main() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    let ld = root.join("src/arch/x86_64/linker.ld");
    println!("cargo:rerun-if-changed={}", ld.display());
    println!("cargo:rustc-link-arg=-T{}", ld.display());

    let mut srcs = Vec::new();
    for d in DIRS {
        println!("cargo:rerun-if-changed={d}");
        walk(&root.join(d), &mut srcs);
    }

    for (i, src) in srcs.iter().enumerate() {
        println!("cargo:rerun-if-changed={}", src.display());
        let stem = src.file_stem().unwrap().to_string_lossy();
        let obj = out.join(format!("{i}_{stem}.o"));
        let ok = match src.extension().and_then(|s| s.to_str()) {
            // xlang has no *-none-elf target yet (plan/languages.md X1); the
            // linux-gnu ELF object is freestanding with --no-std (no undefined symbols).
            Some("xl") => Command::new("xl")
                .args(["build", "--emit=obj", "--no-std", "--target", "x86_64-unknown-linux-gnu", "-O2", "-o"])
                .arg(&obj).arg(src).status(),
            _ => Command::new("clang")
                .args(["-target", "x86_64-unknown-none-elf", "-ffreestanding", "-fno-stack-protector",
                       "-fno-pic", "-mno-red-zone", "-O2", "-c"])
                .arg(src).arg("-o").arg(&obj).status(),
        };
        assert!(ok.map(|s| s.success()).unwrap_or(false), "failed to build {}", src.display());
        println!("cargo:rustc-link-arg={}", obj.display());
    }
}
