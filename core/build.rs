use std::fs;
use std::path::Path;

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let workspace = Path::new(&manifest_dir).parent().unwrap();
    let generated_dir = workspace.join("generated");

    // Generate C header file with exported functions
    let header = r#"#ifndef FIRE_BOX_CORE_H
#define FIRE_BOX_CORE_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/// Start the Fire Box core service.
/// Returns 0 on success, 1 on error, 2 if already running.
int32_t fire_box_start(void);

/// Stop the Fire Box core service.
/// Returns 0 on success, 1 if not running.
int32_t fire_box_stop(void);

/// Reload configuration from keyring.
/// Returns 0 on success, 1 if not running.
int32_t fire_box_reload(void);

/// Legacy entry point (calls fire_box_start).
int32_t fire_box_run_from_args(void);

#ifdef __cplusplus
}
#endif

#endif // FIRE_BOX_CORE_H
"#;

    // Create generated directory and write header
    fs::create_dir_all(&generated_dir).unwrap();
    fs::write(generated_dir.join("core.h"), header).unwrap();

    // Copy libcore.a to generated/ directory (best-effort)
    // OUT_DIR is like <target>/<profile>/build/<crate>-<hash>/out
    // Go up 3 levels to reach <target>/<profile>/
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let target_profile_dir = Path::new(&out_dir)
        .ancestors()
        .nth(3)
        .unwrap()
        .to_path_buf();
    let libcore_src = target_profile_dir.join("libcore.a");

    if libcore_src.exists() {
        let _ = fs::copy(&libcore_src, generated_dir.join("libcore.a"));
    }

    println!("cargo:rerun-if-changed=src/lib.rs");
}
