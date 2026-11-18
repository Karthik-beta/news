//! Embeds every file under `contracts/cli/` so `news schema` serves exactly what is in the repository.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Every `.json` file under `dir`. Any read error is returned: a partial catalogue must not build.
fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for e in std::fs::read_dir(dir)? {
        let p = e?.path();
        if p.is_dir() {
            walk(&p, out)?;
        } else if p.extension().is_some_and(|x| x == "json") {
            out.push(p);
        }
    }
    Ok(())
}

fn main() {
    // Board links the GUI DLLs; delay-load them so the hundreds of CLI calls inside `claude -p` never
    // map OpenGL, user32 and UI Automation (spike item 5: median +270 ms per call otherwise).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        for dll in [
            "opengl32.dll",
            "gdi32.dll",
            "user32.dll",
            "dwmapi.dll",
            "imm32.dll",
            "uxtheme.dll",
            "uiautomationcore.dll",
            "ole32.dll",
            "oleaut32.dll",
        ] {
            println!("cargo:rustc-link-arg-bins=/DELAYLOAD:{dll}");
        }
        println!("cargo:rustc-link-arg-bins=delayimp.lib");
    }
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let root = manifest.join("../../../contracts/cli");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files = Vec::new();
    if let Err(e) = walk(&root, &mut files) {
        panic!("cannot read the contracts in {}: {e}", root.display());
    }
    if files.is_empty() {
        panic!("no contract files in {}", root.display());
    }
    files.sort();
    let mut src = String::from(
        "/// Every contract file: (path relative to contracts/cli, contents).\npub static CONTRACTS: &[(&str, &str)] = &[\n",
    );
    for f in &files {
        println!("cargo:rerun-if-changed={}", f.display());
        let rel = f
            .strip_prefix(&root)
            .map(|r| r.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let _ = writeln!(
            src,
            "    ({rel:?}, include_str!({:?})),",
            f.to_string_lossy()
        );
    }
    src.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_default()).join("contracts.rs");
    if let Err(e) = std::fs::write(&out, src) {
        panic!("cannot write {}: {e}", out.display());
    }
}
