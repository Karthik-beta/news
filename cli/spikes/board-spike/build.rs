fn main() {
    println!("cargo:rerun-if-changed=detached-console.manifest");
    if std::env::var_os("CARGO_FEATURE_DETACHED_CONSOLE").is_some() {
        let m = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default())
            .join("detached-console.manifest");
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}", m.display());
    }
}
