//! KiCad path variables: `${NAME}` and `$(NAME)` in library table URIs and 3D model paths.

use std::collections::BTreeMap;

/// The variables a path is expanded with, in the order KiCad gives them weight: the process environment wins over
/// the "Configure Paths" values of `kicad_common.json` (KiCad warns about that override), and `KIPRJMOD` is the
/// project's folder.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Vars {
    map: BTreeMap<String, String>,
}

/// A path names variables that no source defines.
#[derive(Debug, Clone, PartialEq)]
pub struct Unset(pub Vec<String>);

impl Vars {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.map.insert(name.into(), value.into());
    }

    /// Sets `name` only when nothing set it before.
    pub fn set_default(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.map.entry(name.into()).or_insert_with(|| value.into());
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.map.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// The value of `name`. A versioned KiCad variable that is not set (`KICAD6_3DMODEL_DIR` on KiCad 10) falls
    /// back to the newest version of the same variable that is, as KiCad itself does for boards made with an older
    /// version.
    pub fn get(&self, name: &str) -> Option<&str> {
        if let Some(v) = self.map.get(name) {
            return Some(v);
        }
        let (_, suffix) = split_versioned(name)?;
        self.map
            .iter()
            .filter_map(|(k, v)| split_versioned(k).filter(|(_, s)| *s == suffix).map(|(ver, _)| (ver, v)))
            .max_by_key(|(ver, _)| *ver)
            .map(|(_, v)| v.as_str())
    }

    /// Replaces every `${NAME}` and `$(NAME)`; a `$` that starts neither stays as it is.
    pub fn expand(&self, s: &str) -> Result<String, Unset> {
        let mut out = String::with_capacity(s.len());
        let mut unset = Vec::new();
        let mut rest = s;
        while let Some(i) = rest.find('$') {
            out.push_str(&rest[..i]);
            let after = &rest[i + 1..];
            let close = match after.as_bytes().first() {
                Some(b'{') => '}',
                Some(b'(') => ')',
                _ => {
                    out.push('$');
                    rest = after;
                    continue;
                }
            };
            let Some(end) = after.find(close) else {
                out.push_str(&rest[i..]);
                rest = "";
                break;
            };
            let name = &after[1..end];
            match self.get(name) {
                Some(v) => out.push_str(v),
                None => {
                    if !unset.iter().any(|u| u == name) {
                        unset.push(name.to_string());
                    }
                }
            }
            rest = &after[end + 1..];
        }
        out.push_str(rest);
        if unset.is_empty() { Ok(out) } else { Err(Unset(unset)) }
    }
}

/// `KICAD9_3DMODEL_DIR` is `(9, "3DMODEL_DIR")`.
fn split_versioned(name: &str) -> Option<(u32, &str)> {
    let rest = name.strip_prefix("KICAD")?;
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    let version = rest[..digits].parse().ok()?;
    let suffix = rest[digits..].strip_prefix('_')?;
    Some((version, suffix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_both_forms() {
        let mut v = Vars::new();
        v.set("A", "/a");
        v.set("KIPRJMOD", "/p");
        assert_eq!(v.expand("${A}/x/$(KIPRJMOD)/y").unwrap(), "/a/x//p/y");
        assert_eq!(v.expand("no vars, $5 and ${unclosed").unwrap(), "no vars, $5 and ${unclosed");
        assert_eq!(v.expand("${B}/${C}/${B}").unwrap_err(), Unset(vec!["B".into(), "C".into()]));
    }

    #[test]
    fn older_kicad_versions_fall_back_to_the_newest_set() {
        let mut v = Vars::new();
        v.set("KICAD9_3DMODEL_DIR", "/nine");
        v.set("KICAD10_3DMODEL_DIR", "/ten");
        v.set("KICAD8_SYMBOL_DIR", "/sym8");
        assert_eq!(v.get("KICAD6_3DMODEL_DIR"), Some("/ten"));
        assert_eq!(v.get("KICAD9_3DMODEL_DIR"), Some("/nine"));
        assert_eq!(v.get("KICAD10_SYMBOL_DIR"), Some("/sym8"));
        assert_eq!(v.get("KICAD_USER_TEMPLATE_DIR"), None);
        assert_eq!(v.get("VHRD_3DMODEL_DIR"), None);
    }
}
