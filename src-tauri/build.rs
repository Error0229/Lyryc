fn main() {
    // tauri-build would put the app manifest (Common Controls v6) in the app
    // binary only. The test binaries link the same Windows code, and whether
    // it imports comctl32's TaskDialogIndirect (v6 only) depends on what the
    // linker happens to pull in: without the manifest they then fail to start
    // (STATUS_ENTRYPOINT_NOT_FOUND). So every binary gets it, from the linker,
    // and tauri-build embeds none (`new_without_app_manifest`): a manifest
    // from both would be a duplicate resource and fail the link.
    let windows = tauri_build::WindowsAttributes::new_without_app_manifest();
    if let Err(e) = tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows)) {
        panic!("tauri-build failed: {e:#}");
    }
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
        let manifest = std::path::Path::new(&dir).join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
}
