//! Thin process host for `pr0_runtime`. All server startup, routing and
//! engine orchestration live in the library crate (`src/lib.rs`); this binary
//! only handles CLI flags that must run outside the async runtime and then
//! hands off to `pr0_runtime::run`.

#[tokio::main]
async fn main() -> std::process::ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("--version" | "-V") => {
            println!("{}", pr0_runtime::build_info::display());
            return std::process::ExitCode::SUCCESS;
        }
        Some("--build-info") => {
            println!(
                "pr0former-build-info-v1 {} {} {}",
                pr0_runtime::build_info::GIT,
                pr0_runtime::build_info::DIRTY,
                pr0_runtime::build_info::BUILT
            );
            return std::process::ExitCode::SUCCESS;
        }
        _ => {}
    }
    // The runtime reports failure through its exit code; nothing below this
    // host calls `std::process::exit`, so shutdown always completes first.
    pr0_runtime::run().await
}
