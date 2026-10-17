//! SilkScroll: smooth scrolling for every Windows app.
//!
//! One exe, three modes:
//! - no arguments: the engine (mouse hook + tray icon). If this same exe is
//!   already running, this opens the settings window instead; if a different
//!   copy (say, an older version) is running, this one replaces it.
//! - `--settings`: the settings window.
//! - `--quit`: closes every running copy (engine and settings) and waits until
//!   they are gone. The installer uses this before replacing the exe.

#![cfg_attr(not(test), windows_subsystem = "windows")]

mod animator;
mod autostart;
mod config;
mod easing;
mod engine;
mod icon;
mod instance;
mod settings;
mod target;
mod tray;
mod win;

use std::time::Duration;

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("--settings") => settings::run(),
        Some("--quit") => {
            if !instance::quit_all(Duration::from_secs(10)) {
                std::process::exit(1);
            }
        }
        _ => run_engine(),
    }
}

fn run_engine() {
    let _lock = match instance::acquire(instance::ENGINE_MUTEX) {
        Some(lock) => lock,
        // Another copy, typically an older version: replace it so starting a
        // new download always runs the new code.
        None if instance::engine_is_other_copy() => {
            instance::quit_all(Duration::from_secs(5));
            match instance::acquire_within(instance::ENGINE_MUTEX, Duration::from_secs(5)) {
                Some(lock) => lock,
                None => return,
            }
        }
        // This same exe is already running: open its settings.
        None => {
            settings::run();
            return;
        }
    };
    let cfg = config::load();
    let engine = engine::start(&cfg);
    tray::run(engine, cfg);
}
