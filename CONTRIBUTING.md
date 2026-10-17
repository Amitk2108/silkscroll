# Contributing to SilkScroll

Thanks for helping! This guide covers everything from reporting a bug to shipping a release.

## Ways to contribute

- **Report a bug** using the *Bug report* issue template. Include your Windows version, the app you were
  scrolling in, and your `config.json` if you changed settings.
- **Report app compatibility**, for example "app X scrolls too fast with SilkScroll", using the *App
  compatibility* section of the bug template. These reports help us improve defaults and docs.
- **Suggest a feature** using the *Feature request* template. Please describe the problem before the solution.
- **Improve the docs**, from typos to whole guides.
- **Send code**: bug fixes, features from the issue tracker (look for `good first issue`), tests.

For larger changes, open an issue first so we can agree on the approach before you spend time on it.

## Development setup

1. Install the prerequisites listed in the README under
   [Build from source](README.md#build-from-source): Rust (MSVC toolchain), Visual Studio Build Tools with
   *Desktop development with C++*, and Git.
2. Fork the repository on GitHub and clone your fork:

   ```powershell
   git clone https://github.com/<you>/silkscroll.git
   cd silkscroll
   cargo build
   ```

3. Run the debug build: `cargo run`. Exit any other copy of SilkScroll (and any other smooth-scrolling tool)
   first; only one engine can run at a time.

### Testing with a separate config

To keep your own settings safe while developing, point SilkScroll at a scratch folder by overriding `APPDATA`
in your PowerShell session (it only affects programs started from that window):

```powershell
$env:APPDATA = "$PWD\target\dev-appdata"; cargo run
```

### Useful commands

```powershell
cargo fmt                                   # format (CI runs cargo fmt --check)
cargo clippy --all-targets -- -D warnings   # lint (CI treats warnings as errors)
cargo test                                  # unit tests
cargo run --example wheel_probe -- --inject 3 --seconds 3   # watch the smoothing end to end
cargo run -- --quit                         # stop a running engine
```

## Code guidelines

- **Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) before touching `engine.rs`.** The hook runs for every
  mouse event on the PC: keep it fast, never block in it, and fail open (pass the event through) when in doubt.
- **Keep logic testable.** Math and data handling go in `animator.rs`, `easing.rs` and `config.rs`, which
  don't call Win32, and get unit tests. Win32 code stays thin.
- **`unsafe`:** only around Win32 calls. Keep each block small and don't hold `RefCell` borrows across calls that
  pump messages (`MessageBoxW`, `TrackPopupMenu`, `GetOpenFileNameW`).
- **Formatting** follows `rustfmt.toml` (120 columns). Run `cargo fmt`.
- **Comments** explain *why*, not *what*. Public items get a short doc comment.
- **User-facing text** is short and plain; tooltips explain what a setting does and which way to turn it.
- **No new runtime dependencies** without discussing it in an issue first. SilkScroll stays small.

### Adding a new setting (checklist)

1. `src/config.rs`: add the field to `Profile` (per-app) or `Config` (global) with a doc comment, add its default
   in `Default`, add a `LIMITS_*` range and clamp it in `sanitized()` if it's a number, and extend the tests.
2. `src/engine.rs` (or wherever it applies): use it.
3. `src/settings.rs`: add the control, its position in `layout()`, the tooltip in `add_tooltips()`, show it in
   `fill()`, and handle it in `on_control()`.
4. `README.md`: add a row to the settings reference table.
5. `CHANGELOG.md`: add an entry under *Unreleased*.

## Manual test checklist

Unit tests cover the math; please also run through what your change touches:

- [ ] Wheel probe: `events` > 1 and `total_delta` is exactly `120 x notches` with SilkScroll running.
- [ ] Scroll in a browser, VS Code or another editor, and File Explorer.
- [ ] Ctrl + wheel still zooms in a browser.
- [ ] Settings window: add an app with **+**, change a value, remove it; changes apply without restarting.
- [ ] Tray: *Disable for app*, *Disable for All Apps*, *Start with Windows*, *Exit*.
- [ ] At 100% and 150% display scaling (Settings → Display → Scale) the settings window looks right.

## Commits and pull requests

1. Create a branch from `main`: `git checkout -b fix/hook-timeout` or `feat/per-app-lines`.
2. Keep commits focused. Messages in the imperative mood, for example *Skip injected events from other tools*.
   [Conventional Commits](https://www.conventionalcommits.org) prefixes (`feat:`, `fix:`, `docs:`) are welcome
   but not required.
3. Make sure `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test` pass.
4. Open a pull request and fill in the template: what changed, why, and how you tested it. Screenshots help for
   UI changes.
5. A maintainer will review. Please respond to comments by pushing new commits; we squash when merging.

By contributing, you agree that your contributions are licensed under the project's [MIT License](LICENSE).

## Releasing (maintainers)

1. Update `version` in `Cargo.toml` and in `res/silkscroll.manifest`, and move *Unreleased* entries in
   `CHANGELOG.md` under the new version.
2. Commit, then tag and push:

   ```powershell
   git tag v0.2.0
   git push origin main --tags
   ```

3. The *Release* workflow tests and builds `silkscroll.exe` on Windows, then publishes a GitHub release for the
   tag with the exe, its SHA-256 checksum and generated release notes. It appears on the Releases page within a
   few minutes; edit the notes there if needed.
