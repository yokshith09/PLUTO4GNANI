fn main() {
    // The Mac system-audio capture (ScreenCaptureKit bindings) is partly Swift. The Swift runtime
    // ships with macOS in /usr/lib/swift; tell the linker to look there when the app starts.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    }
    tauri_build::build()
}
