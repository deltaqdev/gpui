#![allow(clippy::disallowed_methods, reason = "build scripts are exempt")]

fn main() {
    println!("cargo::rustc-check-cfg=cfg(gles)");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    if target_os == "windows" {
        #[cfg(feature = "windows-manifest")]
        embed_resource();
    }
}

/// Embeds the application manifest. An app that needs its own (long paths, a
/// different execution level) points `GPUI_WINDOWS_MANIFEST` at it, usually
/// from its `.cargo/config.toml` `[env]`; one manifest per exe is all Windows takes.
#[cfg(feature = "windows-manifest")]
fn embed_resource() {
    println!("cargo:rerun-if-env-changed=GPUI_WINDOWS_MANIFEST");
    if let Some(manifest) = std::env::var_os("GPUI_WINDOWS_MANIFEST") {
        let manifest = std::path::PathBuf::from(manifest);
        println!("cargo:rerun-if-changed={}", manifest.display());
        let out_dir = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
        let rc_file = out_dir.join("app.rc");
        let path = manifest.to_string_lossy().replace('\\', "/");
        std::fs::write(
            &rc_file,
            format!("#define RT_MANIFEST 24\n1 RT_MANIFEST \"{path}\"\n"),
        )
        .unwrap();
        embed_resource::compile(rc_file, embed_resource::NONE)
            .manifest_required()
            .unwrap();
        return;
    }
    let resource_dir = std::path::Path::new("resources/windows");
    let manifest = resource_dir.join("gpui.manifest.xml");
    let rc_file = resource_dir.join("gpui.rc");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rerun-if-changed={}", rc_file.display());
    embed_resource::compile(rc_file, embed_resource::ParamsIncludeDirs([resource_dir]))
        .manifest_required()
        .unwrap();
}
