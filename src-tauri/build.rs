fn main() {
    // tauri-build only embeds its manifest into the app binary, but unit and integration test
    // executables link against tauri too and need the Common Controls v6 manifest on Windows.
    // So embed our own manifest into every linked target and turn off tauri-build's copy.
    let mut windows = tauri_build::WindowsAttributes::new();
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        windows = tauri_build::WindowsAttributes::new_without_app_manifest();
        let manifest =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
    // Every app command must be listed here so that capabilities can allow it per window.
    // Each spec appends its own section.
    let commands: &[&str] = &[
        // --- app-shell ---
        "settings_get",
        "settings_update",
        "settings_check_save_dir",
        "hotkeys_status",
        "hotkeys_retry",
        "hotkeys_suspend",
        "hotkeys_resume",
        // --- capture-library ---
        "library_list",
        "library_open_in_editor",
        "library_copy",
        "library_reveal",
        "library_delete",
        "library_prune_missing",
        // --- screen-capture ---
        // --- annotation-editor ---
        "editor_open_session",
        "editor_read_original",
        "editor_save",
        "editor_copy",
        "editor_pick_export_path",
        "editor_write_export",
        "editor_flush_result",
    ];
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .windows_attributes(windows)
            .app_manifest(tauri_build::AppManifest::new().commands(commands)),
    )
    .expect("failed to run tauri-build");
}
