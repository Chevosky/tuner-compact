//! The Compact revision is shared by the updater and the macOS packager.
//! Increase it for every published Compact release, even when upstream's
//! package version stays the same.

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const COMPACT_REVISION: u64 = 3;

pub fn display_version() -> String {
    format!("{} · r{}", APP_VERSION, COMPACT_REVISION)
}
