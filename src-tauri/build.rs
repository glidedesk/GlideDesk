fn main() {
    // ScreenCaptureKit is used for this computer's sound (media/audio.rs). Its Rust
    // binding ships a small Swift bridge, and linking Swift code needs the Swift
    // runtime libraries. The binding adds that rpath for its own crate only, which
    // does not reach this binary, so add it here: on macOS the runtime lives in the
    // OS (reachable as /usr/lib/swift), and an older Swift toolchain keeps its
    // copies under the Command Line Tools or Xcode.
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rerun-if-env-changed=DEVELOPER_DIR");
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");

        if let Ok(output) = std::process::Command::new("xcode-select").arg("-p").output() {
            if output.status.success() {
                let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
                for path in [
                    format!("{root}/usr/lib/swift/macosx"),
                    format!("{root}/usr/lib/swift-5.5/macosx"),
                    format!("{root}/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift/macosx"),
                ] {
                    if std::path::Path::new(&path).is_dir() {
                        println!("cargo:rustc-link-search=native={path}");
                        println!("cargo:rustc-link-arg=-Wl,-rpath,{path}");
                    }
                }
            }
        }
    }

    tauri_build::build()
}
