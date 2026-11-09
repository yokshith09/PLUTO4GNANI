fn main() {
    // On a Mac the capture library is partly Swift; its runtime lives in /usr/lib/swift. This lets
    // this crate's own test program find it (the app's build.rs does the same for the app).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    }
}
