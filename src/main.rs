use clap::{Arg, Command};
use std::path::PathBuf;

mod file_system;
mod search;
mod ui;
mod file_sharing;
mod config;

use file_system::FileExplorer;
use search::SearchEngine;
use ui::run_ui;
use config::Config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = Command::new("FilePilot")
        .version("0.4.0")
        .author("Nikhil Singh")
        .about("A file explorer with system-wide search capabilities")
        .arg(
            Arg::new("path")
                .short('p')
                .long("path")
                .value_name("PATH")
                .help("Starting directory path (defaults to current dir, or ~ for better performance)")
                .default_value("."),
        )
        .arg(
            Arg::new("search")
                .short('s')
                .long("search")
                .value_name("PATTERN")
                .help("Search pattern"),
        )
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("CONFIG_FILE")
                .help("Path to configuration file"),
        )
        .arg(
            Arg::new("create-config")
                .long("create-config")
                .action(clap::ArgAction::SetTrue)
                .help("Create a default configuration file"),
        )
        .get_matches();

    let start_path = PathBuf::from(matches.get_one::<String>("path").unwrap());
    let search_pattern = matches.get_one::<String>("search");
    let config_file = matches.get_one::<String>("config");
    let create_config = matches.get_flag("create-config");

    // Smart default path selection for better search performance
    let smart_start_path = if matches.get_one::<String>("path").unwrap() == "." {
        // User didn't specify a path, so we're using the default
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let current_str = current_dir.to_string_lossy();
        
        // Check if we're in a potentially slow search location
        if current_str == "/" || 
           current_str == std::env::var("HOME").unwrap_or_default() ||
           current_str.starts_with("/System") ||
           current_str.starts_with("/usr") ||
           current_str.starts_with("/Library") {
            // Default to home directory for better performance
            if let Ok(home) = std::env::var("HOME") {
                eprintln!("Auto-selected home directory (~) for better search performance.");
                eprintln!("   Use -p /path to specify a different starting directory.");
                PathBuf::from(home)
            } else {
                current_dir
            }
        } else {
            current_dir
        }
    } else {
        // User explicitly specified a path, respect their choice
        start_path
    };

    // Handle config creation
    if create_config {
        match Config::create_default_config_file() {
            Ok(path) => {
                println!("✅ Created default configuration file at: {}", path.display());
                println!("You can now edit this file to customize your key bindings.");
                println!("Run FilePilot again to use the new configuration.");
                return Ok(());
            }
            Err(e) => {
                eprintln!("❌ Failed to create configuration file: {}", e);
                std::process::exit(1);
            }
        }
    }

    let explorer = FileExplorer::new(smart_start_path.clone())?;
    let search_engine = SearchEngine::new();
    
    // Warn users about potentially slow search locations
    if let Some(path_str) = smart_start_path.to_str() {
        match path_str {
            "/" => eprintln!("⚠️  Warning: Starting from root directory may cause slow search performance."),
            path if path == std::env::var("HOME").unwrap_or_default() => {
                eprintln!("Starting from home directory. Search performance should be good.");
            }
            _ => {}
        }
    }
    
    // Load configuration from specified file or use auto-discovery
    let config = if let Some(config_path) = config_file {
        match Config::load_from_file(config_path) {
            Ok(config) => {
                eprintln!("Loaded configuration from: {}", config_path);
                config
            }
            Err(e) => {
                eprintln!("Failed to load config from {}: {}", config_path, e);
                eprintln!("Using default configuration.");
                Config::default()
            }
        }
    } else {
        Config::load_default()
    };

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
        run_ui(explorer, search_engine, config).await?;
    }

    Ok(())
}
