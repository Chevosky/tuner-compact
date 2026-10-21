# CPAL 0.15.3: CoreAudio stream lifetime backport

This directory contains the published CPAL 0.15.3 crate, under its original
Apache-2.0 license (see LICENSE). No version or dependency upgrades are made.

Only `src/host/coreaudio/macos/mod.rs` differs from the original crate source.
The CoreAudio device-disconnection listener now captures a `Weak` reference to
its owning `StreamInner`, instead of retaining a strong clone of the stream.
`play` and `pause` delegate to methods on `StreamInner` so the listener can stop
the stream without recursively acquiring its mutex.

The change is backported from the corresponding implementation in the official
CPAL v0.16.0 tag:
https://github.com/RustAudio/cpal/blob/v0.16.0/src/host/coreaudio/macos/mod.rs

Original upstream fix: https://github.com/RustAudio/cpal/pull/869

Without this correction, a stream opened using an enumerated device (including
a selected USB microphone) retains itself through its listener. Dropping the
stream then leaves its AudioUnit and input callback alive. The tuner must also
cancel processing explicitly and keep all platform audio lifecycle operations
away from the GUI thread; those changes live in audio-input and egui/src/audio.rs.

Cargo.toml at the workspace root patches crates.io CPAL to this local directory.
Cargo.lock retains the same CPAL version and other dependency versions.
