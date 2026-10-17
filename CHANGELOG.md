# Changelog

All notable changes to SilkScroll are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org).

## [Unreleased]

### Changed

- Every change to the app pushed to `main` is now released automatically as a new patch version
  (0.2.1, 0.2.2, ...), with the exe and its checksum attached. Minor versions are still listed here.

## [0.2.0] - 2026-10-17

### Changed

- New default feel: step size 120, animation time 360 ms, acceleration delta 70 ms, acceleration max 7x and
  tail-to-head ratio 3. Existing settings are kept; click **Reset All** on *Default (All Applications)* to switch
  to the new defaults.
- *Start with Windows* is now on by default (release builds only).

## [0.1.0] - 2026-10-17

### Added

- Smooth scrolling engine: low-level mouse hook, per-frame animator, `SendInput` replay with exact distances.
- Easing curves: Pulse (with tail-to-head ratio), Ease-out and Linear.
- Acceleration for fast wheel spins, with configurable window and maximum.
- Per-app profiles keyed by exe name, with app icons in the settings window.
- Settings window with native Windows controls, per-value reset buttons, tooltips and live apply.
- Tray icon with *Disable for app*, *Disable for All Apps*, *Start with Windows* and more.
- Pass-through for Ctrl/Alt/Win + wheel, touchpads, hung windows, elevated apps and fullscreen games.
- Shift + wheel horizontal scrolling and reverse direction options.
- `--settings` and `--quit` command line switches.
- `wheel_probe` example for measuring the smoothing end to end.
