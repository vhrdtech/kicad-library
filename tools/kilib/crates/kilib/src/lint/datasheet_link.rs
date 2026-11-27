//! [`Rule::DatasheetLink`](super::Rule::DatasheetLink): a symbol's `Datasheet` that is an `mx3:` link (`mx3:${Mpn1}
//! ${Mpn2}`, `LIB-39`) references exactly the symbol's `Mpn` fields. KiCad has no wildcard over fields: a reference
//! to a field the symbol lacks shows unresolved, and a field added later goes unnamed. The fix writes `mx3:` and a
//! reference to each `Mpn` field, in field order. A vendor URL or an empty `Datasheet` is never looked at.

use crate::migrate::Edit;
use crate::sexpr::{self, Node};

/// How a link starts (any case).
pub const SCHEME: &str = "mx3:";

/// Whether a field holds a part number a link references: `Mpn` or `Mpn` and a number (`Mpn1`, `MPN12`).
pub fn mpn_field(name: &str) -> bool {
    name.get(..3).is_some_and(|p| p.eq_ignore_ascii_case("mpn")) && name[3..].bytes().all(|b| b.is_ascii_digit())
}

/// Whether a `Datasheet` is an `mx3:` link.
pub fn is_link(text: &str) -> bool {
    text.trim().get(..SCHEME.len()).is_some_and(|s| s.eq_ignore_ascii_case(SCHEME))
}

/// The field names `text` references as `${Name}`, in order; an unclosed `${` ends the list.
pub fn references(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("${") {
        let after = &rest[at + 2..];
        let Some(end) = after.find('}') else { break };
        out.push(&after[..end]);
        rest = &after[end + 1..];
    }
    out
}

/// The link of a symbol with these fields: `mx3:` and a reference to each `Mpn` field once, in field order; `None`
/// without one.
pub fn link_of<'a>(fields: impl IntoIterator<Item = &'a str>) -> Option<String> {
    let mut mpns: Vec<&str> = Vec::new();
    for n in fields.into_iter().filter(|n| mpn_field(n)) {
        if !mpns.iter().any(|m| m.eq_ignore_ascii_case(n)) {
            mpns.push(n);
        }
    }
    let refs: Vec<String> = mpns.iter().map(|n| format!("${{{n}}}")).collect();
    (!refs.is_empty()).then(|| format!("{SCHEME}{}", refs.join(" ")))
}

/// What is wrong with `link` given the symbol's field names: the `Mpn` fields it references and the symbol lacks,
/// and those the symbol has and it misses; `None` when it is no link or right (any case, any order).
pub fn stale<'a>(link: &str, fields: &[&'a str]) -> Option<(Vec<String>, Vec<&'a str>)> {
    if !is_link(link) {
        return None;
    }
    let named = references(link);
    let has = |list: &[&str], n: &str| list.iter().any(|m| m.eq_ignore_ascii_case(n));
    let mut unknown: Vec<String> = Vec::new();
    for n in named.iter().filter(|n| mpn_field(n) && !has(fields, n)) {
        if !unknown.iter().any(|u| u.eq_ignore_ascii_case(n)) {
            unknown.push((*n).to_string());
        }
    }
    let missing: Vec<&str> = fields.iter().copied().filter(|n| mpn_field(n) && !has(&named, n)).collect();
    (!unknown.is_empty() || !missing.is_empty()).then_some((unknown, missing))
}

/// One finding of a symbol file: the symbol, what is wrong, and whether the fix changes it.
pub type SymbolFinding = (String, String, bool);

/// The rule on the text of a `.kicad_sym` file (a whole library, or one file of a `.kicad_symdir`): the edits of
/// every symbol's stale link, and the findings.
pub fn run(text: &str) -> Result<(Vec<Edit>, Vec<SymbolFinding>), String> {
    let root = sexpr::parse(text).map_err(|e| e.to_string())?;
    if root.head() != Some("kicad_symbol_lib") {
        return Err("not a symbol library".to_string());
    }
    let mut edits = Vec::new();
    let mut findings = Vec::new();
    for sym in root.children("symbol") {
        let name = sym.arg(0).unwrap_or("").to_string();
        let props: Vec<&Node> = sym.children("property").collect();
        let fields: Vec<&str> = props.iter().filter_map(|p| p.arg(0)).collect();
        let Some(ds) = props.iter().find(|p| p.arg(0) == Some("Datasheet")) else { continue };
        let link = ds.arg(1).unwrap_or("");
        let Some((unknown, missing)) = stale(link, &fields) else { continue };
        let refs = |l: &[&str]| l.iter().map(|n| format!("${{{n}}}")).collect::<Vec<_>>().join(", ");
        let mut why = Vec::new();
        if !unknown.is_empty() {
            let u: Vec<&str> = unknown.iter().map(String::as_str).collect();
            why.push(format!("references {} it has no field for", refs(&u)));
        }
        if !missing.is_empty() {
            why.push(format!("misses {}", refs(&missing)));
        }
        let why = why.join(" and ");
        match (link_of(fields.iter().copied()), ds.items().get(2)) {
            (Some(new), Some(value)) if value.as_atom().is_some() => {
                edits.push(Edit { range: value.span(), text: sexpr::quote(&new) });
                findings.push((name, format!("Datasheet `{link}` {why}: now `{new}`"), true));
            }
            _ => findings.push((name, format!("Datasheet `{link}` {why}; no Mpn field to reference"), false)),
        }
    }
    Ok((edits, findings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrate::apply_edits;

    fn lib(symbols: &[(&str, &str, &[&str])]) -> String {
        let mut s = String::from("(kicad_symbol_lib\n\t(version 20251024)\n");
        for (name, ds, mpns) in symbols {
            s += &format!("\t(symbol \"{name}\"\n\t\t(property \"Reference\" \"U\"\n\t\t\t(at 0 0 0)\n\t\t)\n");
            s += &format!("\t\t(property \"Datasheet\" \"{ds}\"\n\t\t\t(at 0 0 0)\n\t\t)\n");
            for m in *mpns {
                s += &format!("\t\t(property \"{m}\" \"\"\n\t\t\t(at 0 0 0)\n\t\t)\n");
            }
            s += "\t)\n";
        }
        s + ")\n"
    }

    #[test]
    fn mpn_fields() {
        for n in ["Mpn", "Mpn1", "MPN12", "mpn3"] {
            assert!(mpn_field(n), "{n}");
        }
        for n in ["Mpn_Alt", "Mfg1", "Datasheet", "Mp"] {
            assert!(!mpn_field(n), "{n}");
        }
    }

    #[test]
    fn stale_links_are_rewritten() {
        let text = lib(&[
            ("RIGHT", "mx3:${Mpn1} ${Mpn2}", &["Mpn1", "Mpn2"]),
            ("TWELVE", "mx3:${Mpn1} ${Mpn2} ${Mpn3}", &["Mpn1"]),
            ("ADDED", "mx3:${Mpn1}", &["Mpn1", "Mpn2"]),
            ("URL", "https://www.example.com/ds.pdf", &["Mpn1"]),
            ("EMPTY", "", &["Mpn1"]),
            ("NONE", "mx3:${Mpn1}", &[]),
        ]);
        let (edits, findings) = run(&text).unwrap();
        let names: Vec<(&str, bool)> = findings.iter().map(|(n, _, f)| (n.as_str(), *f)).collect();
        assert_eq!(names, [("TWELVE", true), ("ADDED", true), ("NONE", false)]);
        assert!(findings[0].1.contains("references ${Mpn2}, ${Mpn3} it has no field for"), "{findings:?}");
        assert!(findings[1].1.contains("misses ${Mpn2}"), "{findings:?}");
        let new = apply_edits(&text, &edits);
        assert!(new.contains("\"TWELVE\"\n\t\t(property \"Reference\" \"U\"\n\t\t\t(at 0 0 0)\n\t\t)\n\t\t(property \"Datasheet\" \"mx3:${Mpn1}\"\n"));
        assert!(new.contains(
            "(property \"Datasheet\" \"mx3:${Mpn1} ${Mpn2}\"\n\t\t\t(at 0 0 0)\n\t\t)\n\t\t(property \"Mpn1\""
        ));
        assert_eq!(new.lines().count(), text.lines().count());
        let (again, left) = run(&new).unwrap();
        assert!(again.is_empty(), "a second run changes it again");
        assert_eq!(left.len(), 1);
    }

    #[test]
    fn references_and_links() {
        assert_eq!(references("mx3:${A} x ${B}${C} ${D"), ["A", "B", "C"]);
        assert_eq!(link_of(["Value", "Mpn1", "Mfg1", "Mpn12"]).as_deref(), Some("mx3:${Mpn1} ${Mpn12}"));
        assert_eq!(link_of(["Value"]), None);
        assert_eq!(stale("MX3:${MPN1}", &["Mpn1"]), None);
    }
}
