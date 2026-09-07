use serde_json::{Value, json};
pub const GIT: &str = env!("PR0_BUILD_GIT");
pub const DIRTY: &str = env!("PR0_BUILD_DIRTY");
pub const BUILT: &str = env!("PR0_BUILD_UNIX");
pub fn display() -> String {
    format!(
        "pr0former {} · git {}{} · built at Unix {}",
        env!("CARGO_PKG_VERSION"),
        GIT,
        match DIRTY {
            "true" => " (uncommitted changes)",
            "false" => "",
            _ => " (working tree unknown)",
        },
        BUILT
    )
}
pub fn json() -> Value {
    json!({"git_commit":GIT,"dirty":match DIRTY {"true"=>Some(true),"false"=>Some(false),_=>None},"built_at_unix":BUILT.parse::<u64>().ok()})
}
