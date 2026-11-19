fn main() {
    // tauri-build would put the app manifest (Common Controls v6) in the app
    // binary only. The test binaries link the same Windows code, and whether
    // it imports comctl32's TaskDialogIndirect (v6 only) depends on what the
    // linker happens to pull in: without the manifest they then fail to start
    // (STATUS_ENTRYPOINT_NOT_FOUND). So every binary gets it, from the linker.
    let windows = tauri_build::WindowsAttributes::new_without_app_manifest();
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows)).expect("failed to run tauri-build");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let manifest = std::env::current_dir().unwrap().join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
}
