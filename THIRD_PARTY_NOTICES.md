# Third-party notices

## Pulse easing curve

The *Pulse* easing curve in `src/easing.rs` implements the "pulse" (viscous fluid) response described by Michael
Herf, in the form popularized by **smoothscroll.js** by Balázs Galambosi
(<https://github.com/gblazex/smoothscroll-for-websites>), which is released under the MIT License. SilkScroll's
implementation is an independent Rust reimplementation of the mathematical formula; no code from smoothscroll.js
is included.

## Rust crates

SilkScroll is built with the following crates, each used under one of its stated licenses:

| Crate | License | Used for |
|---|---|---|
| [`windows-sys`](https://crates.io/crates/windows-sys) | MIT or Apache-2.0 | Win32 API bindings |
| [`serde`](https://crates.io/crates/serde) | MIT or Apache-2.0 | config serialization |
| [`serde_json`](https://crates.io/crates/serde_json) | MIT or Apache-2.0 | config JSON format |
| [`embed-resource`](https://crates.io/crates/embed-resource) (build only) | MIT | embedding the icon and manifest |

Their transitive dependencies are listed in `Cargo.lock`. Run `cargo tree` to see the full dependency graph.
