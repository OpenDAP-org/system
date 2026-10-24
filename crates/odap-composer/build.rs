// The system layers' design tokens and fonts come from Blackbird UI's published outputs (its
// export/), found through ODAP_BLACKBIRD_UI_DIR or the workspace's repos.tsv, never its sources.
// Glyphs are embedded for the software renderer: the composer needs no font files at run time.
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let ui =
        opendap_workspace::repo_dir("blackbird-ui", &manifest).unwrap_or_else(|e| panic!("{e}"));
    let export = ui.join("export");
    println!(
        "cargo:rerun-if-env-changed={}",
        opendap_workspace::env_var("blackbird-ui")
    );
    println!("cargo:rerun-if-changed={}", export.display());
    let config = slint_build::CompilerConfiguration::new()
        .with_library_paths([("blackbird".to_string(), export)].into())
        .embed_resources(slint_build::EmbedResourcesKind::EmbedForSoftwareRenderer);
    slint_build::compile_with_config("ui/layers.slint", config).unwrap();
}
