//! Finding another repository's checkout, for build scripts that consume its published outputs
//! (RFC 0008): `ODAP_<NAME>_DIR` when set, else the workspace's `repos.tsv` above `start`.
//! Never a guessed relative path.

use std::path::{Path, PathBuf};

/// The environment variable that points at repository `name`'s checkout.
pub fn env_var(name: &str) -> String {
    format!("ODAP_{}_DIR", name.to_uppercase().replace('-', "_"))
}

/// The checkout of the GitHub repository `name` (for example "blackbird-ui").
pub fn repo_dir(name: &str, start: &Path) -> Result<PathBuf, String> {
    let var = env_var(name);
    if let Some(v) = std::env::var_os(&var).filter(|v| !v.is_empty()) {
        let p = PathBuf::from(v);
        return if p.is_dir() { Ok(p) } else { Err(format!("{var}={} is not a directory", p.display())) };
    }
    for dir in start.ancestors() {
        let tsv = dir.join("repos.tsv");
        let Ok(text) = std::fs::read_to_string(&tsv) else { continue };
        for line in text.lines().skip(1) {
            let mut cols = line.split('\t');
            let (Some(path), Some(slug)) = (cols.next(), cols.next()) else { continue };
            if slug.trim().rsplit('/').next() == Some(name) && dir.join(path).is_dir() {
                return Ok(dir.join(path));
            }
        }
        return Err(format!("{name} is not checked out in {}: run scripts/bootstrap.sh, or set {var}", dir.display()));
    }
    Err(format!("no workspace (repos.tsv) above {}: set {var}", start.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_variable_after_the_repository() {
        assert_eq!(env_var("blackbird-ui"), "ODAP_BLACKBIRD_UI_DIR");
    }

    #[test]
    fn finds_a_repository_through_repos_tsv() {
        let root = std::env::temp_dir().join(format!("odap-ws-{}", std::process::id()));
        std::fs::create_dir_all(root.join("public/blackbird-ui")).unwrap();
        std::fs::create_dir_all(root.join("public/system/crates/x")).unwrap();
        std::fs::write(root.join("repos.tsv"), "path\trepository\npublic/blackbird-ui\topendap-project/blackbird-ui\n").unwrap();
        let found = repo_dir("blackbird-ui", &root.join("public/system/crates/x")).unwrap();
        assert_eq!(found, root.join("public/blackbird-ui"));
        assert!(repo_dir("nothing", &root.join("public/system")).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
