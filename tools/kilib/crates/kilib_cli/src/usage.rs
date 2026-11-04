//! Each invocation adds one line to `$XDG_STATE_HOME/kilib/usage.tsv` (default `~/.local/state`): the time,
//! the subcommand and the flags with a meaning of their own, so parts nobody uses can be found and removed. It
//! stays on this PC; a failure to write is ignored.

use std::io::Write;
use std::path::PathBuf;

fn path() -> Option<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?;
    Some(state.join("kilib/usage.tsv"))
}

pub fn record(command: &str, flags: &[(&str, bool)]) {
    let Some(path) = path() else { return };
    let _ = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(path.parent().expect("a parent"))?;
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let flags: Vec<&str> = flags.iter().filter(|(_, on)| *on).map(|(n, _)| *n).collect();
        writeln!(f, "{secs}\t{command}\t{}", flags.join(","))
    })();
}
