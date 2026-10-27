fn main() {
    // Register app commands so they can be granted to remote origins (kimi.com)
    // through the capability file. This generates the `allow-open-external`
    // permission automatically.
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(&["open_external"])),
    )
    .expect("failed to run tauri-build");
}
