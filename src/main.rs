#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::process::ExitCode;

fn main() -> ExitCode {
    match seeusage::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("SeeUsage: {error}");
            ExitCode::FAILURE
        }
    }
}
