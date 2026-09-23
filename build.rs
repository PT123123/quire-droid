use std::path::PathBuf;

fn main() {
    // QUIRE_PROBE: compile a subset .slint entry instead of the app — used to
    // bisect compiler issues per component (see docs/DECISIONS.md debugging note).
    println!("cargo:rerun-if-env-changed=QUIRE_PROBE");
    let ui = std::env::var("QUIRE_PROBE").unwrap_or_else(|_| "ui/AppWindow.slint".into());
    slint_build::compile(&ui).expect("Slint build failed");

    // The exe's shell identity only exists where there is an exe to carry it.
    // `embed-resource` asks the *host* for rc.exe, and the host is Windows when
    // the Android `.so` is cross-compiled from a desktop, so without this gate
    // the Android build would try to stamp a resource into a shared library.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        write_windows_resource();
    }
}

/// The exe's shell identity (M8): the icon Explorer, the taskbar and the Start
/// menu show, plus the version block the file properties tab reads. The `.rc`
/// is generated here so the version can never drift from `[package].version`,
/// and `embed-resource` hands it to `rc.exe`/`link.exe` (no-op off Windows).
/// The picture itself is `install/quire.ico`, produced by
/// `install/make_icon.ps1`.
fn write_windows_resource() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let icon = manifest.join("install").join("quire.ico");
    println!("cargo:rerun-if-changed={}", icon.display());

    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
    let mut parts = version.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
    let (major, minor, patch) = (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    );
    let comma = format!("{major}, {minor}, {patch}, 0");
    let quoted = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\"));
    // StringFileInfo values are NUL-terminated; rc.exe reads `\0` inside the
    // literal, and the properties tab drops it.
    let text = |s: &str| format!("\"{}\\0\"", s.replace('\\', "\\\\"));
    let rc = format!(
        "#include <windows.h>\n\
         IDI_MAIN ICON {icon}\n\
         \n\
         VS_VERSION_INFO VERSIONINFO\n\
         \x20FILEVERSION {comma}\n\
         \x20PRODUCTVERSION {comma}\n\
         \x20FILEOS VOS_NT_WINDOWS32\n\
         \x20FILETYPE VFT_APP\n\
         \x20{{\n\
         \x20  BLOCK \"StringFileInfo\"\n\
         \x20  {{\n\
         \x20    BLOCK \"040904b0\"\n\
         \x20    {{\n\
         \x20      VALUE \"CompanyName\", {company}\n\
         \x20      VALUE \"FileDescription\", {description}\n\
         \x20      VALUE \"FileVersion\", {file_version}\n\
         \x20      VALUE \"InternalName\", {internal}\n\
         \x20      VALUE \"OriginalFilename\", {original}\n\
         \x20      VALUE \"ProductName\", {product}\n\
         \x20      VALUE \"ProductVersion\", {product_version}\n\
         \x20    }}\n\
         \x20  }}\n\
         \x20  BLOCK \"VarFileInfo\"\n\
         \x20  {{\n\
         \x20    VALUE \"Translation\", 0x0409, 0x04b0\n\
         \x20  }}\n\
         }}\n",
        icon = quoted(&icon.display().to_string()),
        company = text("Quire"),
        // ASCII only: rc.exe reads this file in the system codepage
        description = text("Quire - a local, fast editor for your own notes"),
        internal = text("quire"),
        original = text("quire.exe"),
        product = text("Quire"),
        file_version = text(&format!("{version}.0")),
        product_version = text(&format!("Quire {version}")),
    );
    let path = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("quire.rc");
    std::fs::write(&path, rc).expect("write the generated resource script");
    // `NotAttempted` (no rc.exe on this machine) stays a soft miss so the
    // library tests still build elsewhere; a real rc failure must not pass.
    embed_resource::compile(path, embed_resource::NONE)
        .manifest_optional()
        .expect("rc.exe rejected the generated resource script");
}
