use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::env;
use crossterm::event::KeyCode;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBindings {
    pub navigation: NavigationKeys,
    pub actions: ActionKeys,
    pub search_mode: SearchModeKeys,
    pub search_results: SearchResultsKeys,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavigationKeys {
    pub up: Vec<String>,
    pub down: Vec<String>,
    pub left: Vec<String>,
    pub enter: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionKeys {
    pub quit: Vec<String>,
    pub search: Vec<String>,
    pub open: Vec<String>,
    pub reveal: Vec<String>,
    pub share: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchModeKeys {
    pub exit_search: Vec<String>,
    pub exit_to_results: Vec<String>,
    pub toggle_strategy: Vec<String>,
    pub navigate_tab: Vec<String>,
    pub backspace: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultsKeys {
    pub back: Vec<String>,
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            navigation: NavigationKeys {
                up: vec!["Up".to_string()],
                down: vec!["Down".to_string()],
                left: vec!["Left".to_string()],
                enter: vec!["Right".to_string()],
            },
            actions: ActionKeys {
                quit: vec!["q".to_string()],
                search: vec!["/".to_string()],
                open: vec!["o".to_string(), "O".to_string()],
                reveal: vec!["r".to_string(), "R".to_string()],
                share: vec!["s".to_string(), "S".to_string()],
            },
            search_mode: SearchModeKeys {
                exit_search: vec!["Esc".to_string()],
                exit_to_results: vec!["Enter".to_string()],
                toggle_strategy: vec!["F2".to_string()],
                navigate_tab: vec!["Tab".to_string()],
                backspace: vec!["Backspace".to_string()],
            },
            search_results: SearchResultsKeys {
                back: vec!["Esc".to_string(), "Left".to_string()],
            },
        }
    }
}

impl KeyBindings {
    pub fn matches_key(&self, key_lists: &[String], key_code: &KeyCode) -> bool {
        key_lists.iter().any(|key_str| {
            match key_str.as_str() {
                "Up" => matches!(key_code, KeyCode::Up),
                "Down" => matches!(key_code, KeyCode::Down),
                "Left" => matches!(key_code, KeyCode::Left),
                "Right" => matches!(key_code, KeyCode::Right),
                "Enter" => matches!(key_code, KeyCode::Enter),
                "Esc" => matches!(key_code, KeyCode::Esc),
                "Tab" => matches!(key_code, KeyCode::Tab),
                "Backspace" => matches!(key_code, KeyCode::Backspace),
                "F2" => matches!(key_code, KeyCode::F(2)),
                "F3" => matches!(key_code, KeyCode::F(3)),
                "F4" => matches!(key_code, KeyCode::F(4)),
                "F5" => matches!(key_code, KeyCode::F(5)),
                "F6" => matches!(key_code, KeyCode::F(6)),
                "F7" => matches!(key_code, KeyCode::F(7)),
                "F8" => matches!(key_code, KeyCode::F(8)),
                "F9" => matches!(key_code, KeyCode::F(9)),
                "F10" => matches!(key_code, KeyCode::F(10)),
                "F11" => matches!(key_code, KeyCode::F(11)),
                "F12" => matches!(key_code, KeyCode::F(12)),
                other => {
                    // Handle single character keys
                    if other.len() == 1 {
                        if let Some(c) = other.chars().next() {
                            matches!(key_code, KeyCode::Char(ch) if ch == &c)
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }
            }
        })
    }

    pub fn get_key_display(&self, key_lists: &[String]) -> String {
        key_lists.join("/")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub notification_endpoint: Option<String>,
    pub notification_enabled: bool,
    pub key_bindings: KeyBindings,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            notification_endpoint: None,
            notification_enabled: false,
            key_bindings: KeyBindings::default(),
        }
    }
}

impl Config {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let config: Config = serde_json::from_str(&content)?;
        Ok(config)
    }

    pub fn find_config_file() -> Option<PathBuf> {
        // List of potential config file locations in order of preference
        let mut candidates = Vec::new();
        
        // 1. Check current directory for src/config.json (for development)
        candidates.push(PathBuf::from("src/config.json"));
        
        // 2. Check current directory for config.json
        candidates.push(PathBuf::from("config.json"));
        
        // 3. Check if there's a .filepilot directory in current dir
        candidates.push(PathBuf::from(".filepilot/config.json"));
        
        // 4. Check user's home directory for .filepilot/config.json
        if let Ok(home) = env::var("HOME") {
            candidates.push(PathBuf::from(home).join(".filepilot").join("config.json"));
        }
        
        // 5. Check next to the executable
        if let Ok(exe_path) = env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                candidates.push(exe_dir.join("config.json"));
                candidates.push(exe_dir.join("src").join("config.json"));
            }
        }
        
        // Return the first config file that exists
        for candidate in candidates {
            if candidate.exists() {
                return Some(candidate);
            }
        }
        
        None
    }

    pub fn load_default() -> Self {
        // Try to find and load a config file, fallback to default
        if let Some(config_path) = Self::find_config_file() {
            if let Ok(config) = Self::load_from_file(&config_path) {
                eprintln!("Loaded configuration from: {}", config_path.display());
                return config;
            }
        }
        
        eprintln!("No configuration file found, using defaults. You can create a config.json file for custom key bindings.");
        Self::default()
    }

    pub fn create_default_config_file() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let config = Self::default();
        
        // Try to create config in user's home directory first
        let config_path = if let Ok(home) = env::var("HOME") {
            let config_dir = PathBuf::from(home).join(".filepilot");
            fs::create_dir_all(&config_dir)?;
            config_dir.join("config.json")
        } else {
            // Fallback to current directory
            PathBuf::from("config.json")
        };
        
        let config_json = serde_json::to_string_pretty(&config)?;
        fs::write(&config_path, config_json)?;
        
        Ok(config_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_binding_matching() {
        let bindings = KeyBindings::default();
        
        // Test single character key matching
        assert!(bindings.matches_key(&vec!["q".to_string()], &KeyCode::Char('q')));
        assert!(!bindings.matches_key(&vec!["q".to_string()], &KeyCode::Char('w')));
        
        // Test special key matching
        assert!(bindings.matches_key(&vec!["Up".to_string()], &KeyCode::Up));
        assert!(bindings.matches_key(&vec!["Enter".to_string()], &KeyCode::Enter));
        assert!(bindings.matches_key(&vec!["Esc".to_string()], &KeyCode::Esc));
        
        // Test multiple key bindings
        assert!(bindings.matches_key(&vec!["Up".to_string(), "k".to_string()], &KeyCode::Up));
        assert!(bindings.matches_key(&vec!["Up".to_string(), "k".to_string()], &KeyCode::Char('k')));
        assert!(!bindings.matches_key(&vec!["Up".to_string(), "k".to_string()], &KeyCode::Char('j')));
        
        // Test function keys
        assert!(bindings.matches_key(&vec!["F2".to_string()], &KeyCode::F(2)));
        assert!(!bindings.matches_key(&vec!["F2".to_string()], &KeyCode::F(3)));
    }

    #[test]
    fn test_key_display() {
        let bindings = KeyBindings::default();
        
        assert_eq!(bindings.get_key_display(&vec!["q".to_string()]), "q");
        assert_eq!(bindings.get_key_display(&vec!["Up".to_string(), "k".to_string()]), "Up/k");
        assert_eq!(bindings.get_key_display(&vec!["o".to_string(), "O".to_string()]), "o/O");
    }

    #[test]
    fn test_default_config() {
        let config = Config::default();
        
        // Test default navigation keys
        assert!(config.key_bindings.matches_key(&config.key_bindings.navigation.up, &KeyCode::Up));
        assert!(config.key_bindings.matches_key(&config.key_bindings.navigation.up, &KeyCode::Char('k')));
        assert!(config.key_bindings.matches_key(&config.key_bindings.navigation.down, &KeyCode::Down));
        assert!(config.key_bindings.matches_key(&config.key_bindings.navigation.down, &KeyCode::Char('j')));
        
        // Test default action keys
        assert!(config.key_bindings.matches_key(&config.key_bindings.actions.quit, &KeyCode::Char('q')));
        assert!(config.key_bindings.matches_key(&config.key_bindings.actions.search, &KeyCode::Char('/')));
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: Config = serde_json::from_str(&json).unwrap();
        
        // Test that serialization/deserialization preserves key bindings
        assert_eq!(config.key_bindings.navigation.up, parsed.key_bindings.navigation.up);
        assert_eq!(config.key_bindings.actions.quit, parsed.key_bindings.actions.quit);
    }
}
