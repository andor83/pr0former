use std::{path::Path, process::Command};

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    // Git changes must invalidate Cargo's cached build stamp, including commits,
    // checkouts, staged/unstaged edits, and changes in other workspace crates.
    for key in ["--absolute-git-dir", "--git-common-dir"] {
        if let Some(dir) = git(&root, &["rev-parse", key]) {
            let dir = root.join(dir);
            for name in ["HEAD", "index", "refs", "packed-refs"] {
                let path = dir.join(name);
                if path.exists() {
                    println!("cargo:rerun-if-changed={}", path.display());
                }
            }
        }
    }
    if let Some(files) = git(&root, &["ls-files", "-z"]) {
        for file in files.split('\0').filter(|s| !s.is_empty()) {
            println!("cargo:rerun-if-changed={}", root.join(file).display());
        }
    }
    println!("cargo:rerun-if-changed=build.rs");
    let hash = git(&root, &["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".into());
    let dirty = git(
        &root,
        &["status", "--porcelain", "--untracked-files=normal"],
    )
    .map(|s| (!s.is_empty()).to_string())
    .unwrap_or_else(|| "unknown".into());
    let built = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    println!("cargo:rustc-env=PR0_BUILD_GIT={hash}");
    println!("cargo:rustc-env=PR0_BUILD_DIRTY={dirty}");
    println!("cargo:rustc-env=PR0_BUILD_UNIX={built}");
}
