//! The desktop launcher.
//!
//! All application behavior lives in the library target so that the same shell
//! can be started by a mobile entry point; this binary only runs the desktop
//! host. See `src/lib.rs`.
fn main() {
    pr0former_desktop_lib::run();
}
