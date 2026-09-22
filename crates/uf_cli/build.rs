fn main() {
    // The unoptimized clap dispatch exceeds the default Windows main stack.
    // Reserve space for every CLI entry point; pages are committed on demand.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/STACK:8388608");
    }
}
