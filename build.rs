//! Embeds the app icon and the Windows manifest (modern controls, per-monitor
//! DPI awareness) into the exe.

#[path = "src/icon.rs"]
#[allow(dead_code)]
mod icon;

use std::path::PathBuf;
use std::{env, fs};

fn main() {
    println!("cargo:rerun-if-changed=src/icon.rs");
    println!("cargo:rerun-if-changed=res/silkscroll.manifest");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let ico = out.join("silkscroll.ico");
    fs::write(&ico, icon::ico_file(&[16, 20, 24, 32, 40, 48, 64, 256])).unwrap();

    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("res/silkscroll.manifest");
    let rc_path = |p: &PathBuf| p.display().to_string().replace('\\', "/");
    let rc = format!("1 ICON \"{}\"\n1 24 \"{}\"\n", rc_path(&ico), rc_path(&manifest));
    let rc_file = out.join("silkscroll.rc");
    fs::write(&rc_file, rc).unwrap();

    embed_resource::compile(&rc_file, embed_resource::NONE).manifest_required().unwrap();
}
