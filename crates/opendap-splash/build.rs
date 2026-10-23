// The splash screen's lockup is Blackbird UI's published brand file, found through
// ODAP_BLACKBIRD_UI_DIR or the workspace's repos.tsv, never its sources.
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let ui =
        opendap_workspace::repo_dir("blackbird-ui", &manifest).unwrap_or_else(|e| panic!("{e}"));
    let lockup = ui.join("brand/svg/opendap-vertical-light.svg");
    if !lockup.is_file() {
        panic!("{} not found", lockup.display());
    }
    println!(
        "cargo:rerun-if-env-changed={}",
        opendap_workspace::env_var("blackbird-ui")
    );
    println!("cargo:rerun-if-changed={}", lockup.display());
    println!(
        "cargo:rustc-env=OPENDAP_LOCKUP_SVG={}",
        lockup.canonicalize().unwrap().display()
    );
}
