//! meta.json written next to every result: seed, git commit and parameters.

use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn git_state() -> Value {
    json!({
        "commit": git(&["rev-parse", "HEAD"]),
        "dirty": git(&["status", "--porcelain", "--", ":(top,exclude)results"]).map(|s| !s.is_empty()),
    })
}

pub fn write(path: &Path, meta: &Value) -> std::io::Result<()> {
    std::fs::write(path, serde_json::to_string_pretty(meta)? + "\n")
}
