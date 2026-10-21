//! The Turner revision is shared by the updater and the macOS packager.
//! The counter continues the earlier Tuner Compact releases without resetting.
//! Increase it for every published release, even when upstream's
//! package version stays the same.

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const COMPACT_REVISION: u64 = 5;

pub fn display_version() -> String {
    format!("{} · r{}", APP_VERSION, COMPACT_REVISION)
}
