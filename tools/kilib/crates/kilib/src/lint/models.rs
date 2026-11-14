//! 3D models: every part has one; its path starts with `${VHRD_3DMODEL_DIR}/` (the library's own and the makers'
//! models) or `${KICAD10_3DMODEL_DIR}/` (KiCad's stock models); the file exists where the variables point.
//!
//! The fix moves a stock model from an older KiCad's variable (`${KICAD6_3DMODEL_DIR}` to `9`) to
//! `${KICAD10_3DMODEL_DIR}`, and a stock `.wrl` to its `.step` (KiCad 10's stock models are STEP only). A missing
//! model, a path through another variable or an absolute one, and a file that does not exist are reported: they
//! need the model found or made. The files are checked only when [`super::Context::vars`] is given, and only
//! those under a variable it sets.

use super::{Class, Input, Outcome};
use crate::migrate::Edit;
use crate::sexpr;

pub const VARS: [&str; 2] = ["VHRD_3DMODEL_DIR", "KICAD10_3DMODEL_DIR"];

/// The variable a path starts with: `KICAD9_3DMODEL_DIR` for `${KICAD9_3DMODEL_DIR}/x.step`.
fn leading_var(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("${")?;
    Some(&rest[..rest.find('}')?])
}

/// An older KiCad's stock model variable, moved to KiCad 10's.
fn modern(path: &str) -> Option<String> {
    let var = leading_var(path)?;
    let version: u32 = var.strip_prefix("KICAD")?.strip_suffix("_3DMODEL_DIR")?.parse().ok()?;
    if version >= 10 {
        return None;
    }
    let rest = &path[var.len() + 3..];
    let rest = match rest.strip_suffix(".wrl") {
        Some(stem) => format!("{stem}.step"),
        None => rest.to_string(),
    };
    Some(format!("${{KICAD10_3DMODEL_DIR}}{rest}"))
}

pub fn run(i: &Input) -> Outcome {
    let mut out = Outcome::default();
    let shown: Vec<_> = i.fp.models.iter().filter(|m| !m.hidden).collect();
    if shown.is_empty() {
        match i.class {
            Class::Part => out.left("no 3D model"),
            Class::Mechanical => out.left("no 3D model (mechanical)"),
            _ => {}
        }
    }
    for m in &i.fp.models {
        let mut path = m.path.clone();
        if let Some(new) = modern(&path) {
            out.fixed(format!("3D model {path} → {new}"));
            out.edits.push(Edit { range: m.path_span.clone(), text: sexpr::quote(&new) });
            path = new;
        } else if !leading_var(&path).is_some_and(|v| VARS.contains(&v)) {
            out.left(format!("3D model {path}: not under ${{VHRD_3DMODEL_DIR}} or ${{KICAD10_3DMODEL_DIR}}"));
        }
        // A variable this PC does not set leaves the file unchecked (the caller says so once).
        let unset = leading_var(&path).is_some_and(|v| i.ctx.vars.as_ref().is_some_and(|vars| vars.get(v).is_none()));
        if let Some(vars) = i.ctx.vars.as_ref().filter(|_| !unset) {
            let here = std::path::Path::new(".");
            if let Some(why) = crate::check::model_problem(&path, here, vars) {
                out.left(format!("3D model {path}: {why}"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::tests::{fp, run, settled};
    use super::super::{Context, Rule, lint_text};
    use crate::testdir::TestDir;
    use crate::vars::Vars;

    const PAD: &str = r#"(pad "1" smd rect (at 0 0) (size 1 1) (layers "F.Cu" "F.Mask"))"#;

    #[test]
    fn old_stock_variables_move_to_kicad10() {
        let t = fp("R", &format!("{PAD}\n(model \"${{KICAD6_3DMODEL_DIR}}/Resistor_SMD.3dshapes/R_0402.wrl\")"));
        let (new, f) = run("VHRD_Resistor_SMD", &t, Rule::Models);
        assert!(new.unwrap().contains("(model \"${KICAD10_3DMODEL_DIR}/Resistor_SMD.3dshapes/R_0402.step\")"));
        assert!(f[0].fixed);
        settled("VHRD_Resistor_SMD", &t, Rule::Models);
    }

    #[test]
    fn missing_and_foreign_models_are_reported() {
        let (_, f) = run("VHRD_Package_SO", &fp("IC", PAD), Rule::Models);
        assert_eq!(f[0].message, "no 3D model");
        let (_, f) = run("VHRD_TestPoint", &fp("TP", PAD), Rule::Models);
        assert!(f.is_empty(), "a test point needs no model: {f:?}");
        let t = fp("IC", &format!("{PAD}\n(model \"${{KIPRJMOD}}/x.step\")"));
        let (new, f) = run("VHRD_Package_SO", &t, Rule::Models);
        assert!(new.is_none());
        assert!(f[0].message.contains("not under ${VHRD_3DMODEL_DIR}"), "{f:?}");
    }

    #[test]
    fn files_are_checked_with_the_variables() {
        let dir = TestDir::new("lint_models");
        dir.write("models/own/X.3dshapes/a.step", "");
        let mut vars = Vars::new();
        vars.set("VHRD_3DMODEL_DIR", dir.path().join("models").to_string_lossy());
        let ctx = Context { vars: Some(vars), ..Context::default() };
        let t = fp(
            "IC",
            &format!(
                "{PAD}\n(model \"${{VHRD_3DMODEL_DIR}}/own/X.3dshapes/a.step\")\n(model \"${{VHRD_3DMODEL_DIR}}/own/X.3dshapes/b.step\")"
            ),
        );
        let (_, _, f) = lint_text(&t, "VHRD_X", &[Rule::Models], &ctx).unwrap();
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].message.starts_with("3D model ${VHRD_3DMODEL_DIR}/own/X.3dshapes/b.step: missing"), "{f:?}");
    }
}
