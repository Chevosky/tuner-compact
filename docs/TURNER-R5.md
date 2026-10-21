# Turner 0.1.13 · r5

## User-approved scope

Instrument-first selector with a neutral sidebar and left-aligned tuning rows.
Browsing an instrument does not activate a tuning. A green check identifies only
the active tuning; selection commits the canonical preset index and closes the
picker. Escape and an outside click cancel browsing. Opening again restores the
active instrument. Guitar variants and baritone ukulele are grouped with their
instrument families. Search is global and bilingual, with accent normalization.

The popover is bounded to the real application viewport, including 520 × 290.
Its width is at most 480 logical points. The sidebar and tuning list scroll
independently. Notes, including the split-octave courses of charango and ronroco,
appear in the row instead of in an overlapping hover tooltip.

Includes the already prepared system-language preference (Automatic / Español /
English) and the charango and Argentine ronroco presets. The original 33 preset
indices are preserved and the two new presets are appended. The application
icon, microphone jack drawing, audio lifecycle, detector, A4 controls, tones,
analysis views and shared update protocol are not redesigned here.

## Verification

`cargo test --locked -p audio-input -p guitar-tuner-egui` exercises the actual
egui event loop, pointer interactions, cancellation, search + Enter, language
switching, family grouping and viewport geometry, in addition to the existing
localization, audio and updater suites. Publication is gated by native Intel
tests, package checks and updater validation of the newly built ZIP.

Automated UI tests do not establish microphone performance or complete update
replacement/relaunch on the user's Big Sur iMac. Those remain device checks.
