//! Where KiCad keeps its user config (the global library tables and `kicad_common.json`) and the path variables
//! it defines.

use std::path::{Path, PathBuf};

use crate::vars::Vars;

/// Looks up an environment variable. The library never reads the process environment itself, so tests and the
/// CLI decide what it sees.
pub type Env<'a> = &'a dyn Fn(&str) -> Option<String>;

/// The versioned config folder KiCad reads, like `~/.config/kicad/10.0`.
///
/// The base is `KICAD_CONFIG_HOME` when set, else the platform's config folder plus `kicad`; KiCad appends its
/// settings version to both. The newest version folder there wins. `None` when no base can be found.
pub fn config_dir(env: Env) -> Option<PathBuf> {
    let base = config_base(env)?;
    Some(newest_version_dir(&base).unwrap_or(base))
}

fn config_base(env: Env) -> Option<PathBuf> {
    if let Some(dir) = env("KICAD_CONFIG_HOME").filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    if cfg!(windows) {
        return env("APPDATA").map(|d| Path::new(&d).join("kicad"));
    }
    if cfg!(target_os = "macos") {
        return env("HOME").map(|h| Path::new(&h).join("Library/Preferences/kicad"));
    }
    if let Some(xdg) = env("XDG_CONFIG_HOME").filter(|s| !s.is_empty()) {
        return Some(Path::new(&xdg).join("kicad"));
    }
    env("HOME").map(|h| Path::new(&h).join(".config/kicad"))
}

/// The subfolder with the highest `MAJOR.MINOR` name.
fn newest_version_dir(base: &Path) -> Option<PathBuf> {
    std::fs::read_dir(base)
        .ok()?
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let (major, minor) = name.split_once('.')?;
            Some(((major.parse::<u32>().ok()?, minor.parse::<u32>().ok()?), e.path()))
        })
        .max_by_key(|(v, _)| *v)
        .map(|(_, p)| p)
}

/// The "Configure Paths" variables in `kicad_common.json` (`environment.vars`).
pub fn common_vars(config_dir: &Path) -> Result<Vec<(String, String)>, String> {
    let path = config_dir.join("kicad_common.json");
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let Some(vars) = json.pointer("/environment/vars").and_then(|v| v.as_object()) else {
        return Ok(Vec::new());
    };
    Ok(vars.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect())
}

/// The variables boards are resolved with: `kicad_common.json`, then the process environment over it (KiCad
/// expands any environment variable in a path, and one set there beats "Configure Paths"), then the stock folders
/// of a system KiCad (`/usr/share/kicad/...`) for `KICAD10_*_DIR` still unset. `KIPRJMOD` is set per project by
/// the checker.
pub fn vars(
    config_dir: Option<&Path>,
    env: impl IntoIterator<Item = (String, String)>,
    system_share: Option<&Path>,
) -> Result<Vars, String> {
    let mut vars = Vars::new();
    if let Some(dir) = config_dir {
        for (k, v) in common_vars(dir)? {
            vars.set(k, v);
        }
    }
    for (k, v) in env {
        if !v.is_empty() {
            vars.set(k, v);
        }
    }
    if let Some(share) = system_share {
        for (suffix, dir) in [("SYMBOL_DIR", "symbols"), ("FOOTPRINT_DIR", "footprints"), ("3DMODEL_DIR", "3dmodels")] {
            let path = share.join(dir);
            if path.is_dir() && vars.get(&format!("KICAD10_{suffix}")).is_none() {
                vars.set(format!("KICAD10_{suffix}"), path.to_string_lossy());
            }
        }
    }
    Ok(vars)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
        let map: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |k| map.get(k).cloned()
    }

    #[test]
    fn config_home_and_newest_version() {
        let dir = crate::testdir::TestDir::new("config_home");
        for v in ["8.0", "10.0", "9.99", "colors"] {
            std::fs::create_dir_all(dir.path().join(v)).unwrap();
        }
        let base = dir.path().to_string_lossy().to_string();
        let e = env(&[("KICAD_CONFIG_HOME", &base), ("HOME", "/nonexistent")]);
        assert_eq!(config_dir(&e), Some(dir.path().join("10.0")));
        if cfg!(all(unix, not(target_os = "macos"))) {
            let e = env(&[("HOME", &base)]);
            assert_eq!(config_dir(&e), Some(dir.path().join(".config/kicad")));
        }
    }

    #[test]
    fn environment_wins_over_kicad_common() {
        let dir = crate::testdir::TestDir::new("common_vars");
        std::fs::write(
            dir.path().join("kicad_common.json"),
            r#"{"environment": {"vars": {"VHRD_3DMODEL_DIR": "/cfg/3d", "MY_DIR": "/cfg/my", "KICAD10_3DMODEL_DIR": "/cfg/k"}}}"#,
        )
        .unwrap();
        let e = [("MY_DIR", "/env/my"), ("HOME", "/home/x")].map(|(k, v)| (k.to_string(), v.to_string()));
        let v = vars(Some(dir.path()), e, None).unwrap();
        assert_eq!(v.get("VHRD_3DMODEL_DIR"), Some("/cfg/3d"));
        assert_eq!(v.get("MY_DIR"), Some("/env/my"));
        assert_eq!(v.get("HOME"), Some("/home/x"));
        assert_eq!(v.get("KICAD6_3DMODEL_DIR"), Some("/cfg/k"));
    }

    #[test]
    fn missing_or_empty_common_is_no_vars() {
        let dir = crate::testdir::TestDir::new("no_common");
        assert!(common_vars(dir.path()).unwrap().is_empty());
        std::fs::write(dir.path().join("kicad_common.json"), r#"{"environment": {"vars": null}}"#).unwrap();
        assert!(common_vars(dir.path()).unwrap().is_empty());
    }
}
