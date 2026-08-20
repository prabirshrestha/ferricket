mod cli;
mod github;
mod preferences;
mod query;
mod session_activity;
mod storage;
mod tui;
mod tui_input;
mod ui;

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    match cli::run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if let Some(exit) = error.downcast_ref::<cli::PluginExit>() {
                return exit
                    .0
                    .code()
                    .and_then(|code| u8::try_from(code).ok())
                    .map(ExitCode::from)
                    .unwrap_or(ExitCode::FAILURE);
            }
            if !error.to_string().is_empty() {
                eprintln!("{error}");
            }
            ExitCode::FAILURE
        }
    }
}
