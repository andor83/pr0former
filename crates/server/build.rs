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
/// Resolve HEAD without a `git` binary (build environments with a minimal PATH).
fn head_from_files(root: &Path) -> Option<String> {
    let git_dir = root.join(".git");
    let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let head = head.trim();
    let Some(reference) = head.strip_prefix("ref: ") else {
        return (head.len() == 40).then(|| head.to_owned());
    };
    if let Ok(hash) = std::fs::read_to_string(git_dir.join(reference)) {
        return Some(hash.trim().to_owned());
    }
    let packed = std::fs::read_to_string(git_dir.join("packed-refs")).ok()?;
    packed
        .lines()
        .filter_map(|line| line.split_once(' '))
        .find(|(_, name)| *name == reference)
        .map(|(hash, _)| hash.to_owned())
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
    let hash = git(&root, &["rev-parse", "HEAD"])
        .or_else(|| head_from_files(&root))
        .unwrap_or_else(|| "unknown".into());
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
