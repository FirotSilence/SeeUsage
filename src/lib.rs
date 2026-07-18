mod app;
mod autostart;
mod cache;
mod command;
mod format;
mod hud;
mod i18n;
mod icon;
mod model;
mod path_trust;
mod personal;
mod platform_ui;
mod providers;
mod settings;
mod tool_approval;

use std::env;

pub use model::AppSnapshot;

pub fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("--once") => run_once(),
        Some("--help" | "-h") => {
            print_help();
            Ok(())
        }
        Some("--version" | "-V") => {
            println!("SeeUsage {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some(argument) => Err(format!("unknown argument: {argument}\nTry SeeUsage --help")),
        None => app::run_tray(),
    }
}

fn run_once() -> Result<(), String> {
    let previous = cache::load().unwrap_or_default();
    let snapshot = providers::refresh(previous, false);
    if let Some(error) = &snapshot.codex.error {
        eprintln!("Codex: {error}");
    }
    if let Some(error) = &snapshot.minimax.error {
        eprintln!("MiniMax: {error}");
    }
    let output = serde_json::to_string_pretty(&snapshot)
        .map_err(|error| format!("could not serialize usage: {error}"))?;
    println!("{output}");

    if snapshot.codex.items.is_empty() && snapshot.minimax.items.is_empty() {
        Err("neither provider returned usage data".into())
    } else {
        Ok(())
    }
}

fn print_help() {
    println!(
        "SeeUsage {}\n\n\
         A tiny tray meter for Codex and MiniMax quotas.\n\n\
         USAGE:\n  SeeUsage [OPTION]\n\n\
         OPTIONS:\n  --once       Query both providers and print normalized JSON\n  -h, --help   Print help\n  -V, --version  Print version\n\n\
         ENVIRONMENT:\n  SEEUSAGE_CODEX_BIN       Override the Codex executable\n  SEEUSAGE_MMX_BIN         Override the mmx executable\n  SEEUSAGE_REFRESH_SECONDS Refresh interval (minimum 30, default 180)",
        env!("CARGO_PKG_VERSION")
    );
}
