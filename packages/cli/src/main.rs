use std::process::ExitCode;

use clap::Parser;

use rustrak_cli::cli::Cli;
use rustrak_cli::commands;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    match commands::dispatch::dispatch(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // Errors go to stderr, so a command's stdout stays parseable even
            // when it fails; the server's per-field detail follows the message.
            eprintln!("error: {error}");
            for field in error.fields() {
                eprintln!("  {}: {}", field.field, field.message);
            }
            if matches!(error, rustrak_cli::Error::Unauthenticated(_)) {
                eprintln!("hint: run `rustrak auth login --url <instance>`");
            }
            ExitCode::from(error.exit_code() as u8)
        }
    }
}
