// The design tokens, fonts and icons come from Blackbird UI's published outputs (its export/),
// found through ODAP_BLACKBIRD_UI_DIR or the workspace's repos.tsv, never from its sources.
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let ui = opendap_workspace::repo_dir("blackbird-ui", &manifest).unwrap_or_else(|e| panic!("{e}"));
    let export = ui.join("export");
    for sub in ["slint", "fonts", "icons"] {
        if !export.join(sub).is_dir() {
            panic!("{} not found: run blackbird-ui's generators (tokens/generate.py, icons/icons.py)", export.join(sub).display());
        }
    }
    println!("cargo:rerun-if-env-changed={}", opendap_workspace::env_var("blackbird-ui"));
    println!("cargo:rerun-if-changed={}", export.display());

    let config = slint_build::CompilerConfiguration::new()
        .with_library_paths([("blackbird".to_string(), export)].into());
    slint_build::compile_with_config("ui/product.slint", config).unwrap();
}
