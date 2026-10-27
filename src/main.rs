use clap::Parser;
use herdr_planbook::cli::{Cli, execute};
use herdr_planbook::documents::{json_text, terminal_text};

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    match execute(&cli) {
        Ok(code) => std::process::ExitCode::from(code),
        Err(error) => {
            if cli.json() {
                println!(
                    "{}",
                    json_text(&serde_json::json!({"error": format!("{error:#}")}))
                );
            } else {
                eprintln!("planbook: {}", terminal_text(&format!("{error:#}")));
            }
            std::process::ExitCode::FAILURE
        }
    }
}
