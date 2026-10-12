//! Embeds assets/icons/<kind>/<id>.png into the exe (ICONS in $OUT_DIR/icons.rs, sorted by "<kind>/<file>"), so a
//! release build is a single file. On Windows also the exe's own icon (ulala.rc), which Explorer and pinned taskbar
//! buttons show.

use std::fmt::Write;
use std::path::Path;

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("ulala.rc", embed_resource::NONE).manifest_required().unwrap();
    }
    println!("cargo:rerun-if-changed=assets/img/icon.ico");
    println!("cargo:rerun-if-changed=assets/icons");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/icons");
    let mut icons = Vec::new();
    for kind in std::fs::read_dir(&root).unwrap().flatten().filter(|e| e.path().is_dir()) {
        for f in std::fs::read_dir(kind.path()).unwrap().flatten() {
            let rel = format!("{}/{}", kind.file_name().to_string_lossy(), f.file_name().to_string_lossy());
            icons.push((rel, f.path()));
        }
    }
    icons.sort();
    let mut out = String::from("pub static ICONS: &[(&str, &[u8])] = &[\n");
    for (rel, path) in icons {
        writeln!(out, "    ({rel:?}, include_bytes!({:?})),", path.to_string_lossy()).unwrap();
    }
    out.push_str("];\n");
    std::fs::write(Path::new(&std::env::var("OUT_DIR").unwrap()).join("icons.rs"), out).unwrap();
}
