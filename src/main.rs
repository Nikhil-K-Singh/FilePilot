use clap::{Arg, Command};
use std::path::PathBuf;

mod file_system;
mod search;
mod ui;
mod file_sharing;

use file_system::FileExplorer;
use search::SearchEngine;
use ui::run_ui;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = Command::new("FilePilot")
        .version("0.1.0")
        .author("Nikhil Singh")
        .about("A file explorer with system-wide search capabilities")
        .arg(
            Arg::new("path")
                .short('p')
                .long("path")
                .value_name("PATH")
                .help("Starting directory path")
                .default_value("."),
        )
        .arg(
            Arg::new("search")
                .short('s')
                .long("search")
                .value_name("PATTERN")
                .help("Search pattern"),
        )
        .get_matches();

    let start_path = PathBuf::from(matches.get_one::<String>("path").unwrap());
    let search_pattern = matches.get_one::<String>("search");

    let explorer = FileExplorer::new(start_path)?;
    let search_engine = SearchEngine::new();

    if let Some(pattern) = search_pattern {
        // Command-line search mode
        match search_engine.search(&explorer.current_path(), pattern).await {
            Ok(results) => {
                for result in results {
                    println!("{}", result.file_info.path.display());
                }
            }
            Err(e) => {
                eprintln!("Search error: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        // Interactive UI mode
        run_ui(explorer, search_engine).await?;
    }

    Ok(())
}
