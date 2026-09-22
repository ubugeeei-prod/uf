fn main() {
    // The script reads target metadata only; checkout timestamps are not inputs.
    println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_ENV");
    // The unoptimized clap dispatch exceeds the default Windows main stack.
    // Reserve space for every CLI entry point; pages are committed on demand.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/STACK:8388608");
    }
}
