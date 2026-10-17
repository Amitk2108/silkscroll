# Changelog

All notable changes to SilkScroll are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org).

## [Unreleased]

## [0.3.0] - 2026-10-17

### Added

- **Installer** (`SilkScroll-Setup.exe`): per-user install with a choice of folder, Start menu and optional
  desktop shortcut, *Settings > Apps* uninstall entry, and launch on finish. Running a newer setup closes the
  running SilkScroll and upgrades it in place, keeping your settings. Uninstalling also removes the startup entry
  and offers to delete your settings.
- Starting a different copy of SilkScroll (for example a newer download) now closes the older copy that is running
  and takes over, instead of leaving the old version active.
- Version details (product name, version, copyright) in the exe's *Properties → Details*.
- Every change to the app pushed to `main` is released automatically as a new patch version, with the installer,
  the portable exe and `SHA256SUMS.txt` attached.

### Changed

- Default acceleration max is now 3x (defaults: step size 120, animation time 360 ms, acceleration delta 70 ms,
  acceleration max 3x, tail-to-head ratio 3).
- Settings saved by an older version whose defaults were never changed are upgraded to the new defaults
  automatically. Customized values and app profiles are kept.
- `--quit` now also closes the settings window and waits until SilkScroll has exited.

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
