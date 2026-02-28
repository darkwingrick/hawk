fn main() {
    if let Ok(bundled) = std::env::var("HAWK_BUNDLE") {
        println!("cargo:rustc-env=HAWK_BUNDLE={}", bundled);
    }
}
