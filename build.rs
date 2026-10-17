//! Embeds the app icon, the Windows manifest (modern controls, per-monitor
//! DPI awareness) and the version information shown in the exe's
//! Properties → Details tab.

#[path = "src/icon.rs"]
#[allow(dead_code)]
mod icon;

use std::path::PathBuf;
use std::{env, fs};

const PRODUCT: &str = "SilkScroll";
const DESCRIPTION: &str = "SilkScroll";
const COMMENTS: &str = "Smooth scrolling for every Windows app. https://github.com/Amitk2108/silkscroll";
const COMPANY: &str = "SilkScroll contributors";
const COPYRIGHT: &str = "Copyright (c) 2026 SilkScroll contributors. MIT License.";

fn main() {
    println!("cargo:rerun-if-changed=src/icon.rs");
    println!("cargo:rerun-if-changed=res/silkscroll.manifest");
    // The version comes from Cargo.toml, which the release workflow stamps.
    println!("cargo:rerun-if-changed=Cargo.toml");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let ico = out.join("silkscroll.ico");
    fs::write(&ico, icon::ico_file(&[16, 20, 24, 32, 40, 48, 64, 256])).unwrap();

    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("res/silkscroll.manifest");
    let rc_path = |p: &PathBuf| p.display().to_string().replace('\\', "/");
    let rc = format!("1 ICON \"{}\"\n1 24 \"{}\"\n{}", rc_path(&ico), rc_path(&manifest), version_info());
    let rc_file = out.join("silkscroll.rc");
    fs::write(&rc_file, rc).unwrap();

    embed_resource::compile(&rc_file, embed_resource::NONE).manifest_required().unwrap();
}

/// A `VERSIONINFO` resource: product name, description, version, copyright and
/// language, as Windows shows them in the file's properties.
fn version_info() -> String {
    let part = |name: &str| env::var(name).ok().and_then(|v| v.parse::<u16>().ok()).unwrap_or(0);
    let (major, minor, patch) =
        (part("CARGO_PKG_VERSION_MAJOR"), part("CARGO_PKG_VERSION_MINOR"), part("CARGO_PKG_VERSION_PATCH"));
    let version = env::var("CARGO_PKG_VERSION").unwrap();

    format!(
        r#"
1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEFLAGSMASK 0x3F
FILEFLAGS 0x0
FILEOS 0x40004
FILETYPE 0x1
FILESUBTYPE 0x0
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "{COMPANY}"
      VALUE "FileDescription", "{DESCRIPTION}"
      VALUE "FileVersion", "{major}.{minor}.{patch}.0"
      VALUE "InternalName", "silkscroll"
      VALUE "LegalCopyright", "{COPYRIGHT}"
      VALUE "OriginalFilename", "silkscroll.exe"
      VALUE "ProductName", "{PRODUCT}"
      VALUE "ProductVersion", "{version}"
      VALUE "Comments", "{COMMENTS}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    )
}
