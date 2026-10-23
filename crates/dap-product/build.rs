// The design tokens and fonts come from Blackbird UI's published outputs, never from its
// sources: BLACKBIRD_UI points at a checkout, defaulting to the workspace layout
// (public/system next to public/blackbird-ui).
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let ui = std::env::var_os("BLACKBIRD_UI")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest.join("../../../blackbird-ui"));
    let tokens = ui.join("export/slint");
    let fonts = ui.join("fonts");
    for path in [&tokens, &fonts] {
        if !path.is_dir() {
            panic!(
                "{} not found: set BLACKBIRD_UI to a blackbird-ui checkout (run tokens/generate.py there)",
                path.display()
            );
        }
    }
    println!("cargo:rerun-if-env-changed=BLACKBIRD_UI");
    println!("cargo:rerun-if-changed={}", tokens.display());
    println!("cargo:rerun-if-changed={}", fonts.display());

    let config = slint_build::CompilerConfiguration::new()
        .with_library_paths([("blackbird".to_string(), ui.clone())].into());
    slint_build::compile_with_config("ui/product.slint", config).unwrap();
}
