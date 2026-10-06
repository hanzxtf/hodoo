//! Stamps the commit into `hodoo --version`, so a binary on a server can be traced
//! back to the source it was built from.
//!
//! Outside a git checkout (a crates.io tarball, a vendored copy) the stamp is
//! `unknown` rather than a failed build: the version number alone still holds.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn main() {
    let hash = git(&["rev-parse", "--short=10", "HEAD"]);
    // Tracked changes only: a stray scratch file is not a different build.
    let dirty = git(&["status", "--porcelain", "--untracked-files=no"])
        .is_some_and(|status| !status.is_empty());
    let stamp = match hash {
        Some(hash) if dirty => format!("{hash}-dirty"),
        Some(hash) => hash,
        None => "unknown".to_owned(),
    };
    println!("cargo:rustc-env=HODOO_COMMIT={stamp}");

    // Rerun when the commit moves (HEAD, the branch ref, a pack) or the source
    // changes, which is what can flip the -dirty mark. Without these, cargo would
    // only rerun this script when a file of this crate changed, and a fresh commit
    // would keep reporting the previous hash.
    if let Some(dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        println!("cargo:rerun-if-changed={dir}/HEAD");
        println!("cargo:rerun-if-changed={dir}/index");
        println!("cargo:rerun-if-changed={dir}/packed-refs");
        if let Some(branch) = git(&["symbolic-ref", "-q", "HEAD"]) {
            println!("cargo:rerun-if-changed={dir}/{branch}");
        }
    }
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=../hodoo/src");
}
