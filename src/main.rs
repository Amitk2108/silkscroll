//! SilkScroll: smooth scrolling for every Windows app.
//!
//! One exe, three modes:
//! - no arguments: the engine (mouse hook + tray icon). If it is already
//!   running, this opens the settings window instead.
//! - `--settings`: the settings window.
//! - `--quit`: asks a running engine to exit.

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

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("--settings") => settings::run(),
        Some("--quit") => {
            instance::notify_engine(instance::WM_APP_QUIT);
        }
        _ => run_engine(),
    }
}

fn run_engine() {
    let Some(_lock) = instance::acquire(instance::ENGINE_MUTEX) else {
        // Starting SilkScroll again while it runs opens the settings.
        settings::run();
        return;
    };
    let cfg = config::load();
    let engine = engine::start(&cfg);
    tray::run(engine, cfg);
}
