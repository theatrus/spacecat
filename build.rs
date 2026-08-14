//! Embeds the git SHA into the binary as `SPACECAT_GIT_SHA`.
//!
//! Resolution order:
//!   1. a `SPACECAT_GIT_SHA` environment variable set at build time — used
//!      by CI release/RPM builds, which compile from a source tarball with
//!      no `.git` directory;
//!   2. `git rev-parse --short=12 HEAD` (with a `-dirty` suffix when the
//!      working tree has uncommitted changes);
//!   3. the literal `"unknown"`.

use std::process::Command;

fn git_sha() -> Option<String> {
    let sha = Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())?;

    let dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false);

    Some(if dirty { format!("{sha}-dirty") } else { sha })
}

fn main() {
    let sha = std::env::var("SPACECAT_GIT_SHA")
        .ok()
        .filter(|s| !s.is_empty())
        // CI passes the full 40-char SHA; shorten it to match git's output
        .map(|s| {
            if s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit()) {
                s[..12].to_string()
            } else {
                s
            }
        })
        .or_else(git_sha)
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=SPACECAT_GIT_SHA={sha}");
    println!("cargo:rerun-if-env-changed=SPACECAT_GIT_SHA");
    // Re-run when HEAD moves so the SHA stays current across commits and
    // branch switches.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");
}
