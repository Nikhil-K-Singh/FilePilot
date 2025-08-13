use crate::file_system::{FileExplorer, FileInfo};
use crate::search::{SearchEngine, SearchResult};
use crate::file_sharing::FileShareServer;
use crate::config::Config;
use arboard::Clipboard;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use std::io;
use std::time::Instant;
use std::path::PathBuf;
use tokio::time::{sleep, Duration};

#[derive(Debug, Clone, PartialEq)]
pub enum SearchStrategy {
    Fast,        // Quick search with limited depth and results
    Comprehensive, // Full search with all features
    LocalOnly,   // Search only in current directory files
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClipboardOperation {
    Cut,
    Copy,
}

#[derive(Debug, Clone)]
pub struct ClipboardEntry {
    pub file_path: PathBuf,
    pub operation: ClipboardOperation,
}

#[derive(Debug, Clone)]
pub enum MessageType {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct StatusMessage {
    pub text: String,
    pub message_type: MessageType,
    pub timestamp: Instant,
    pub fade_duration: Duration,
}

impl SearchStrategy {
    pub fn next(&self) -> Self {
        match self {
            SearchStrategy::Fast => SearchStrategy::Comprehensive,
            SearchStrategy::Comprehensive => SearchStrategy::LocalOnly,
            SearchStrategy::LocalOnly => SearchStrategy::Fast,
        }
    }

    pub fn description(&self) -> &str {
        match self {
            SearchStrategy::Fast => "Fast (limited depth)",
            SearchStrategy::Comprehensive => "Comprehensive (full search)",
            SearchStrategy::LocalOnly => "Local (current dir only)",
        }
    }
}

pub struct App {
    pub explorer: FileExplorer,
    pub search_engine: SearchEngine,
    pub file_share_server: FileShareServer,
    pub config: Config,
    pub list_state: ListState,
    pub search_mode: bool,
    pub search_input: String,
    pub search_results: Vec<SearchResult>,
    pub search_list_state: ListState,
    pub status_message: Option<StatusMessage>,
    pub search_strategy: SearchStrategy,
    pub showing_search_results: bool,
    pub clipboard: Option<ClipboardEntry>,
}

impl App {
    pub fn new(explorer: FileExplorer, search_engine: SearchEngine, config: Config) -> App {
        let mut app = App {
            explorer,
            search_engine,
            file_share_server: FileShareServer::new(),
            config,
            list_state: ListState::default(),
            search_mode: false,
            search_input: String::new(),
            search_results: Vec::new(),
            search_list_state: ListState::default(),
            status_message: Some(StatusMessage {
                text: "Press '/' to search, 'q' to quit, Enter to navigate, 'x' to cut, 'c' to copy, 'v' to paste".to_string(),
                message_type: MessageType::Info,
                timestamp: Instant::now(),
                fade_duration: Duration::from_secs(u64::MAX), // Never fade the default message
            }),
            search_strategy: SearchStrategy::Fast,
            showing_search_results: false,
            clipboard: None,
        };
        app.list_state.select(Some(0));
        app
    }

    pub fn set_message(&mut self, text: String, message_type: MessageType, fade_duration: Duration) {
        self.status_message = Some(StatusMessage {
            text,
            message_type,
            timestamp: Instant::now(),
            fade_duration,
        });
    }

    pub fn set_info_message(&mut self, text: String) {
        self.set_message(text, MessageType::Info, Duration::from_secs(u64::MAX));
    }

    pub fn set_warning_message(&mut self, text: String) {
        self.set_message(text, MessageType::Warning, Duration::from_secs(5));
    }

    pub fn set_error_message(&mut self, text: String) {
        self.set_message(text, MessageType::Error, Duration::from_secs(8));
    }

    pub fn update_message_fade(&mut self) {
        if let Some(msg) = &self.status_message {
            if msg.timestamp.elapsed() > msg.fade_duration {
                self.status_message = Some(StatusMessage {
                    text: "Press '/' to search, 'q' to quit, Enter to navigate, 'x' to cut, 'c' to copy, 'v' to paste".to_string(),
                    message_type: MessageType::Info,
                    timestamp: Instant::now(),
                    fade_duration: Duration::from_secs(u64::MAX),
                });
            }
        }
    }

    pub fn get_current_message(&self) -> &str {
        self.status_message.as_ref().map(|m| m.text.as_str()).unwrap_or("")
    }

    pub fn get_message_style(&self) -> Style {
        match self.status_message.as_ref().map(|m| &m.message_type) {
            Some(MessageType::Error) => Style::default().fg(Color::Red),
            Some(MessageType::Warning) => Style::default().fg(Color::Yellow),
            Some(MessageType::Info) => Style::default().fg(Color::White),
            None => Style::default().fg(Color::White),
        }
    }

    pub fn next_item(&mut self) {
        if (self.search_mode || self.showing_search_results) && !self.search_results.is_empty() {
            let i = match self.search_list_state.selected() {
                Some(i) => {
                    if i >= self.search_results.len() - 1 {
                        0
                    } else {
                        i + 1
                    }
                }
                None => 0,
            };
            self.search_list_state.select(Some(i));
        } else if !self.explorer.files().is_empty() {
            let i = match self.list_state.selected() {
                Some(i) => {
                    if i >= self.explorer.files().len() - 1 {
                        0
                    } else {
                        i + 1
                    }
                }
                None => 0,
            };
            self.list_state.select(Some(i));
        }
    }

    pub fn previous_item(&mut self) {
        if (self.search_mode || self.showing_search_results) && !self.search_results.is_empty() {
            let i = match self.search_list_state.selected() {
                Some(i) => {
                    if i == 0 {
                        self.search_results.len() - 1
                    } else {
                        i - 1
                    }
                }
                None => 0,
            };
            self.search_list_state.select(Some(i));
        } else if !self.explorer.files().is_empty() {
            let i = match self.list_state.selected() {
                Some(i) => {
                    if i == 0 {
                        self.explorer.files().len() - 1
                    } else {
                        i - 1
                    }
                }
                None => 0,
            };
            self.list_state.select(Some(i));
        }
    }

    pub async fn perform_search(&mut self) {
        if !self.search_input.is_empty() {
            // Show searching indicator
            self.set_info_message(format!("Searching for '{}' in {}...", 
                self.search_input,
                self.explorer.current_path().display()
            ));

            let result = match self.search_strategy {
                SearchStrategy::Fast => {
                    self.search_engine.search_fast(self.explorer.current_path(), &self.search_input, 100).await
                }
                SearchStrategy::Comprehensive => {
                    self.search_engine.search(self.explorer.current_path(), &self.search_input).await
                }
                SearchStrategy::LocalOnly => {
                    let results = self.search_engine.search_in_files(self.explorer.files(), &self.search_input);
                    Ok(results)
                }
            };

            match result {
                Ok(results) => {
                    self.search_results = results;
                    self.search_list_state.select(if self.search_results.is_empty() { None } else { Some(0) });
                    if self.search_results.is_empty() {
                        self.set_warning_message(format!("No results found for '{}' ({})", 
                            self.search_input,
                            self.search_strategy.description()
                        ));
                    } else {
                        self.set_info_message(format!("Found {} results ({})", 
                            self.search_results.len(), 
                            self.search_strategy.description()
                        ));
                    }
                }
                Err(e) => {
                    self.set_error_message(format!("Search error: {}", e));
                }
            }
        }
    }

    pub fn toggle_search_strategy(&mut self) {
        self.search_strategy = self.search_strategy.next();
        self.set_info_message(format!("Search strategy: {}", self.search_strategy.description()));
        
        // Re-run search if we're in search mode and have input
        if self.search_mode && !self.search_input.is_empty() {
            // We'll trigger a search on the next event loop iteration
            if let Some(ref mut msg) = self.status_message {
                msg.text.push_str(" - type to search again");
            }
        }
    }

    pub fn navigate_to_selected(&mut self) -> Result<(), std::io::Error> {
        if self.search_mode || self.showing_search_results {
            if let Some(selected) = self.search_list_state.selected() {
                if let Some(result) = self.search_results.get(selected) {
                    if result.file_info.is_directory {
                        self.explorer.navigate_to(result.file_info.path.clone())?;
                        self.clear_search_results();
                    }
                }
            }
        } else if let Some(selected) = self.list_state.selected() {
            if let Some(file) = self.explorer.files().get(selected) {
                if file.is_directory {
                    self.explorer.navigate_to(file.path.clone())?;
                    self.list_state.select(Some(0));
                }
            }
        }
        Ok(())
    }

    pub fn go_up(&mut self) -> Result<(), std::io::Error> {
        self.explorer.go_up()?;
        self.list_state.select(Some(0));
        Ok(())
    }

    pub fn enter_search_mode(&mut self) {
        self.search_mode = true;
        self.showing_search_results = false;
        self.search_input.clear();
        self.search_results.clear();
        self.set_info_message(format!("Search mode: {} - Type to search, F2 to toggle strategy, ESC to exit, Enter to keep results", 
            self.search_strategy.description()));
    }

    pub fn exit_search_mode(&mut self) {
        if !self.search_results.is_empty() {
            // Keep search results and switch to showing them
            self.search_mode = false;
            self.showing_search_results = true;
            self.set_info_message(format!("Search results ({} items) - Navigate with ↑↓, Enter to open, '/' to search again", 
                self.search_results.len()));
        } else {
            // No results, clear everything
            self.search_mode = false;
            self.showing_search_results = false;
            self.search_input.clear();
            self.set_info_message("Press '/' to search, 'q' to quit, Enter to navigate".to_string());
        }
    }

    pub fn clear_search_results(&mut self) {
        self.search_mode = false;
        self.showing_search_results = false;
        self.search_input.clear();
        self.search_results.clear();
        self.search_list_state = ListState::default();
        self.list_state.select(Some(0));
        self.set_info_message("Press '/' to search, 'q' to quit, Enter to navigate, 'x' to cut, 'c' to copy, 'v' to paste".to_string());
    }

    pub fn open_selected_file(&mut self) -> Result<String, String> {
        let selected_file = self.get_selected_file()?;

        if selected_file.is_directory {
            return Err("Cannot open directory as file. Use Enter to navigate.".to_string());
        }

        match self.explorer.open_file(selected_file) {
            Ok(_) => Ok(format!("Opened '{}' with default application", selected_file.name)),
            Err(e) => Err(format!("Failed to open '{}': {}", selected_file.name, e)),
        }
    }

    pub fn reveal_selected_in_file_manager(&mut self) -> Result<String, String> {
        let selected_file = self.get_selected_file()?;

        match self.explorer.reveal_in_file_manager(selected_file) {
            Ok(_) => Ok(format!("Revealed '{}' in file manager", selected_file.name)),
            Err(e) => Err(format!("Failed to reveal '{}': {}", selected_file.name, e)),
        }
    }

    fn get_selected_file(&self) -> Result<&FileInfo, String> {
        if self.showing_search_results {
            if let Some(selected_idx) = self.search_list_state.selected() {
                if selected_idx < self.search_results.len() {
                    Ok(&self.search_results[selected_idx].file_info)
                } else {
                    Err("Invalid selection".to_string())
                }
            } else {
                Err("No file selected".to_string())
            }
        } else {
            if let Some(selected_idx) = self.list_state.selected() {
                if selected_idx < self.explorer.files().len() {
                    Ok(&self.explorer.files()[selected_idx])
                } else {
                    Err("Invalid selection".to_string())
                }
            } else {
                Err("No file selected".to_string())
            }
        }
    }

    pub async fn share_selected_file(&mut self) -> Result<String, String> {
        let selected_file_path = {
            let selected_file = self.get_selected_file()?;
            if selected_file.is_directory {
                return Err("Cannot share directories. Please select a file.".to_string());
            }
            selected_file.path.clone()
        };

        let file_name = selected_file_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        match self.file_share_server.share_file(&selected_file_path).await {
            Ok(url) => Ok(format!("Shared '{}' - Link copied to clipboard: {}", file_name, url)),
            Err(e) => Err(format!("Failed to share '{}': {}", file_name, e)),
        }
    }

    pub fn cut_selected_file(&mut self) -> Result<String, String> {
        let (file_path, file_name) = {
            let selected_file = self.get_selected_file()?;
            (selected_file.path.clone(), selected_file.name.clone())
        };
        
        self.clipboard = Some(ClipboardEntry {
            file_path,
            operation: ClipboardOperation::Cut,
        });
        
        Ok(format!("Cut '{}' - navigate to destination and press 'v' to paste", file_name))
    }

    pub fn copy_selected_file(&mut self) -> Result<String, String> {
        let (file_path, file_name) = {
            let selected_file = self.get_selected_file()?;
            (selected_file.path.clone(), selected_file.name.clone())
        };
        
        self.clipboard = Some(ClipboardEntry {
            file_path,
            operation: ClipboardOperation::Copy,
        });
        
        Ok(format!("Copied '{}' - navigate to destination and press 'v' to paste", file_name))
    }

    pub fn paste_file(&mut self) -> Result<String, String> {
        let clipboard_entry = match &self.clipboard {
            Some(entry) => entry.clone(),
            None => return Err("Nothing to paste - cut or copy a file first".to_string()),
        };

        // Check if source file still exists
        if !clipboard_entry.file_path.exists() {
            self.clipboard = None;
            return Err("Source file no longer exists".to_string());
        }

        let source_path = &clipboard_entry.file_path;
        let current_dir = self.explorer.current_path();
        
        // Get the filename from the source path
        let file_name = source_path.file_name()
            .ok_or("Invalid source file path")?;
        
        let destination_path = current_dir.join(file_name);

        // Check if destination already exists
        if destination_path.exists() {
            return Err(format!("File '{}' already exists in destination directory", file_name.to_string_lossy()));
        }

        // Check if we're trying to move/copy to the same directory
        if let Some(source_parent) = source_path.parent() {
            if source_parent == current_dir {
                return Err("Cannot paste file to the same directory".to_string());
            }
        }

        match clipboard_entry.operation {
            ClipboardOperation::Copy => {
                match self.copy_file_operation(source_path, &destination_path) {
                    Ok(_) => {
                        self.explorer.refresh().map_err(|e| format!("Failed to refresh: {}", e))?;
                        Ok(format!("Copied '{}' to current directory", file_name.to_string_lossy()))
                    }
                    Err(e) => Err(format!("Failed to copy file: {}", e)),
                }
            }
            ClipboardOperation::Cut => {
                match self.move_file_operation(source_path, &destination_path) {
                    Ok(_) => {
                        self.clipboard = None; // Clear clipboard after successful cut operation
                        self.explorer.refresh().map_err(|e| format!("Failed to refresh: {}", e))?;
                        Ok(format!("Moved '{}' to current directory", file_name.to_string_lossy()))
                    }
                    Err(e) => Err(format!("Failed to move file: {}", e)),
                }
            }
        }
    }

    pub fn copy_selected_file_path(&self) -> Result<String, String> {
        let file_info = if self.showing_search_results {
            if let Some(selected) = self.search_list_state.selected() {
                if selected < self.search_results.len() {
                    &self.search_results[selected].file_info
                } else {
                    return Err("No file selected".to_string());
                }
            } else {
                return Err("No file selected".to_string());
            }
        } else {
            if let Some(selected) = self.list_state.selected() {
                let files = self.explorer.files();
                if selected < files.len() {
                    &files[selected]
                } else {
                    return Err("No file selected".to_string());
                }
            } else {
                return Err("No file selected".to_string());
            }
        };

        let path_str = file_info.path.to_string_lossy().to_string();
        
        // Copy to system clipboard
        match Clipboard::new().and_then(|mut clipboard| clipboard.set_text(&path_str)) {
            Ok(_) => Ok(format!("Copied path to clipboard: {}", path_str)),
            Err(e) => Err(format!("Failed to copy path to clipboard: {}", e)),
        }
    }

    fn copy_file_operation(&self, source: &PathBuf, destination: &PathBuf) -> Result<(), std::io::Error> {
        if source.is_dir() {
            self.copy_directory_recursive(source, destination)
        } else {
            std::fs::copy(source, destination)?;
            Ok(())
        }
    }

    fn copy_directory_recursive(&self, source: &PathBuf, destination: &PathBuf) -> Result<(), std::io::Error> {
        std::fs::create_dir_all(destination)?;
        
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            let source_path = entry.path();
            let dest_path = destination.join(entry.file_name());
            
            if source_path.is_dir() {
                self.copy_directory_recursive(&source_path, &dest_path)?;
            } else {
                std::fs::copy(&source_path, &dest_path)?;
            }
        }
        
        Ok(())
    }

    fn move_file_operation(&self, source: &PathBuf, destination: &PathBuf) -> Result<(), std::io::Error> {
        std::fs::rename(source, destination)
    }

    pub fn get_file_preview(&self) -> Vec<String> {
        let files = self.explorer.files();
        let selected_index = match self.list_state.selected() {
            Some(index) => index,
            None => return vec!["No file selected".to_string()],
        };
        
        if selected_index >= files.len() {
            return vec!["No file selected".to_string()];
        }
        
        let selected_file = &files[selected_index];

        if selected_file.is_directory {
            // For directories, show the contents
            match std::fs::read_dir(&selected_file.path) {
                Ok(entries) => {
                    let mut items = Vec::new();
                    items.push(format!("📁 Directory: {}", selected_file.name));
                    items.push("".to_string());
                    
                    let mut dir_entries: Vec<_> = entries.collect();
                    dir_entries.sort_by(|a, b| {
                        match (a.as_ref().unwrap().path().is_dir(), b.as_ref().unwrap().path().is_dir()) {
                            (true, false) => std::cmp::Ordering::Less,
                            (false, true) => std::cmp::Ordering::Greater,
                            _ => a.as_ref().unwrap().file_name().cmp(&b.as_ref().unwrap().file_name()),
                        }
                    });

                    for (i, entry) in dir_entries.iter().enumerate() {
                        if i >= 10 { // Limit to 10 items
                            items.push(format!("... and {} more items", dir_entries.len() - 10));
                            break;
                        }
                        if let Ok(entry) = entry {
                            let icon = if entry.path().is_dir() { "📁" } else { "📄" };
                            items.push(format!("{} {}", icon, entry.file_name().to_string_lossy()));
                        }
                    }
                    items
                }
                Err(_) => vec!["Error reading directory".to_string()],
            }
        } else {
            // For files, show the first 10 lines
            match std::fs::read_to_string(&selected_file.path) {
                Ok(content) => {
                    let mut lines = Vec::new();
                    lines.push(format!("📄 File: {} ({:.1} KB)", 
                        selected_file.name, 
                        selected_file.size as f64 / 1024.0));
                    lines.push("".to_string());
                    
                    let file_lines: Vec<&str> = content.lines().collect();
                    let preview_lines = if file_lines.len() > 10 {
                        &file_lines[..10]
                    } else {
                        &file_lines
                    };
                    
                    for (i, line) in preview_lines.iter().enumerate() {
                        // Truncate very long lines
                        let truncated_line = if line.len() > 60 {
                            format!("{}...", &line[..57])
                        } else {
                            line.to_string()
                        };
                        lines.push(format!("{:2}: {}", i + 1, truncated_line));
                    }
                    
                    if file_lines.len() > 10 {
                        lines.push("".to_string());
                        lines.push(format!("... ({} more lines)", file_lines.len() - 10));
                    }
                    
                    lines
                }
                Err(_) => {
                    // For binary files or files that can't be read as text
                    let extension = selected_file.path.extension()
                        .and_then(|ext| ext.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    
                    match extension.as_str() {
                        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "svg" | "ico" | "webp" => {
                            vec![
                                format!("Image: {}", selected_file.name),
                                format!("Size: {:.1} KB", selected_file.size as f64 / 1024.0),
                                "".to_string(),
                                "Image file - use 'o' to open".to_string(),
                                "or 's' to share via web".to_string(),
                            ]
                        }
                        "mp4" | "avi" | "mov" | "wmv" | "flv" | "webm" | "mkv" => {
                            vec![
                                format!("🎥 Video: {}", selected_file.name),
                                format!("Size: {:.1} MB", selected_file.size as f64 / (1024.0 * 1024.0)),
                                "".to_string(),
                                "Video file - use 'o' to open".to_string(),
                                "or 's' to share via web".to_string(),
                            ]
                        }
                        "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" => {
                            vec![
                                format!("🎵 Audio: {}", selected_file.name),
                                format!("Size: {:.1} MB", selected_file.size as f64 / (1024.0 * 1024.0)),
                                "".to_string(),
                                "Audio file - use 'o' to open".to_string(),
                                "or 's' to share via web".to_string(),
                            ]
                        }
                        "pdf" => {
                            vec![
                                format!("PDF: {}", selected_file.name),
                                format!("Size: {:.1} MB", selected_file.size as f64 / (1024.0 * 1024.0)),
                                "".to_string(),
                                "PDF document - use 'o' to open".to_string(),
                                "or 's' to share via web".to_string(),
                            ]
                        }
                        "zip" | "tar" | "gz" | "rar" | "7z" => {
                            vec![
                                format!("Archive: {}", selected_file.name),
                                format!("Size: {:.1} MB", selected_file.size as f64 / (1024.0 * 1024.0)),
                                "".to_string(),
                                "Archive file - use 'o' to open".to_string(),
                                "with system default".to_string(),
                            ]
                        }
                        _ => {
                            vec![
                                format!("Binary: {}", selected_file.name),
                                format!("Size: {:.1} KB", selected_file.size as f64 / 1024.0),
                                "".to_string(),
                                "Binary file - cannot preview".to_string(),
                                "Use 'o' to open with default app".to_string(),
                            ]
                        }
                    }
                }
            }
        }
    }
}

pub async fn run_ui(
    explorer: FileExplorer,
    search_engine: SearchEngine,
    config: Config,
) -> Result<(), Box<dyn std::error::Error>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = App::new(explorer, search_engine, config);

    let res = run_app(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err);
    }

    Ok(())
}

async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> io::Result<()> {
    loop {
        // Update message fade status
        app.update_message_fade();
        
        terminal.draw(|f| ui(f, app))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    // Handle search mode keys
                    if app.search_mode {
                        let key_bindings = &app.config.key_bindings;
                        if key_bindings.matches_key(&key_bindings.search_mode.exit_search, &key.code) {
                            app.exit_search_mode();
                        } else if key_bindings.matches_key(&key_bindings.search_mode.exit_to_results, &key.code) {
                            app.exit_search_mode();
                        } else if key_bindings.matches_key(&key_bindings.search_mode.toggle_strategy, &key.code) {
                            app.toggle_search_strategy();
                            // Re-run search if we have input
                            if !app.search_input.is_empty() {
                                sleep(Duration::from_millis(50)).await;
                                app.perform_search().await;
                            }
                        } else if key_bindings.matches_key(&key_bindings.search_mode.backspace, &key.code) {
                            app.search_input.pop();
                            if !app.search_input.is_empty() {
                                app.perform_search().await;
                            } else {
                                app.search_results.clear();
                            }
                        } else if key_bindings.matches_key(&key_bindings.navigation.up, &key.code) {
                            app.previous_item();
                        } else if key_bindings.matches_key(&key_bindings.navigation.down, &key.code) {
                            app.next_item();
                        } else if key_bindings.matches_key(&key_bindings.search_mode.navigate_tab, &key.code) {
                            app.navigate_to_selected().ok();
                        } else {
                            match key.code {
                                KeyCode::Char(c) => {
                                    app.search_input.push(c);
                                    // Shorter delay for more responsive search
                                    sleep(Duration::from_millis(100)).await;
                                    app.perform_search().await;
                                }
                                _ => {}
                            }
                        }
                    } else if app.showing_search_results {
                        // Handle search results viewing mode keys
                        let key_bindings = &app.config.key_bindings;
                        if key_bindings.matches_key(&key_bindings.actions.quit, &key.code) {
                            // Properly shutdown the file sharing server
                            let _ = app.file_share_server.shutdown().await;
                            return Ok(());
                        } else if key_bindings.matches_key(&key_bindings.actions.search, &key.code) {
                            app.enter_search_mode();
                        } else if key_bindings.matches_key(&key_bindings.actions.open, &key.code) {
                            match app.open_selected_file() {
                                Ok(msg) => app.set_info_message(msg),
                                Err(err) => app.set_error_message(err),
                            }
                        } else if key_bindings.matches_key(&key_bindings.actions.reveal, &key.code) {
                            match app.reveal_selected_in_file_manager() {
                                Ok(msg) => app.set_info_message(msg),
                                Err(err) => app.set_error_message(err),
                            }
                        } else if key_bindings.matches_key(&key_bindings.actions.share, &key.code) {
                            match app.share_selected_file().await {
                                Ok(msg) => {
                                    if msg.contains("Warning:") {
                                        app.set_warning_message(msg);
                                    } else {
                                        app.set_info_message(msg);
                                    }
                                },
                                Err(err) => app.set_error_message(err),
                            }
                        } else if key_bindings.matches_key(&key_bindings.actions.cut, &key.code) {
                            match app.cut_selected_file() {
                                Ok(msg) => app.set_info_message(msg),
                                Err(err) => app.set_error_message(err),
                            }
                        } else if key_bindings.matches_key(&key_bindings.actions.copy, &key.code) {
                            match app.copy_selected_file() {
                                Ok(msg) => app.set_info_message(msg),
                                Err(err) => app.set_error_message(err),
                            }
                        } else if key_bindings.matches_key(&key_bindings.actions.paste, &key.code) {
                            match app.paste_file() {
                                Ok(msg) => app.set_info_message(msg),
                                Err(err) => app.set_error_message(err),
                            }
                        } else if key_bindings.matches_key(&key_bindings.actions.copy_path, &key.code) {
                            match app.copy_selected_file_path() {
                                Ok(msg) => app.set_info_message(msg),
                                Err(err) => app.set_error_message(err),
                            }
                        } else if key_bindings.matches_key(&key_bindings.search_results.back, &key.code) {
                            app.clear_search_results();
                        } else if key_bindings.matches_key(&key_bindings.search_mode.toggle_strategy, &key.code) {
                            app.toggle_search_strategy();
                        } else if key_bindings.matches_key(&key_bindings.navigation.enter, &key.code) {
                            let _ = app.navigate_to_selected();
                        } else if key_bindings.matches_key(&key_bindings.navigation.up, &key.code) {
                            app.previous_item();
                        } else if key_bindings.matches_key(&key_bindings.navigation.down, &key.code) {
                            app.next_item();
                        } else if key_bindings.matches_key(&key_bindings.navigation.left, &key.code) {
                            app.clear_search_results();
                        }
                    } else {
                        // Handle normal navigation mode keys
                        let key_bindings = &app.config.key_bindings;
                        if key_bindings.matches_key(&key_bindings.actions.quit, &key.code) {
                            // Properly shutdown the file sharing server
                            let _ = app.file_share_server.shutdown().await;
                            return Ok(());
                        } else if key_bindings.matches_key(&key_bindings.actions.search, &key.code) {
                            app.enter_search_mode();
                        } else if key_bindings.matches_key(&key_bindings.actions.open, &key.code) {
                            match app.open_selected_file() {
                                Ok(msg) => app.set_info_message(msg),
                                Err(err) => app.set_error_message(err),
                            }
                        } else if key_bindings.matches_key(&key_bindings.actions.reveal, &key.code) {
                            match app.reveal_selected_in_file_manager() {
                                Ok(msg) => app.set_info_message(msg),
                                Err(err) => app.set_error_message(err),
                            }
                        } else if key_bindings.matches_key(&key_bindings.actions.share, &key.code) {
                            match app.share_selected_file().await {
                                Ok(msg) => {
                                    if msg.contains("Warning:") {
                                        app.set_warning_message(msg);
                                    } else {
                                        app.set_info_message(msg);
                                    }
                                },
                                Err(err) => app.set_error_message(err),
                            }
                        } else if key_bindings.matches_key(&key_bindings.actions.cut, &key.code) {
                            match app.cut_selected_file() {
                                Ok(msg) => app.set_info_message(msg),
                                Err(err) => app.set_error_message(err),
                            }
                        } else if key_bindings.matches_key(&key_bindings.actions.copy, &key.code) {
                            match app.copy_selected_file() {
                                Ok(msg) => app.set_info_message(msg),
                                Err(err) => app.set_error_message(err),
                            }
                        } else if key_bindings.matches_key(&key_bindings.actions.paste, &key.code) {
                            match app.paste_file() {
                                Ok(msg) => app.set_info_message(msg),
                                Err(err) => app.set_error_message(err),
                            }
                        } else if key_bindings.matches_key(&key_bindings.actions.copy_path, &key.code) {
                            match app.copy_selected_file_path() {
                                Ok(msg) => app.set_info_message(msg),
                                Err(err) => app.set_error_message(err),
                            }
                        } else if key_bindings.matches_key(&key_bindings.search_mode.toggle_strategy, &key.code) {
                            app.toggle_search_strategy();
                        } else if key_bindings.matches_key(&key_bindings.navigation.enter, &key.code) {
                            let _ = app.navigate_to_selected();
                        } else if key_bindings.matches_key(&key_bindings.navigation.up, &key.code) {
                            app.previous_item();
                        } else if key_bindings.matches_key(&key_bindings.navigation.down, &key.code) {
                            app.next_item();
                        } else if key_bindings.matches_key(&key_bindings.navigation.left, &key.code) {
                            let _ = app.go_up();
                        }
                    }
                }
            }
        }
    }
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(f.size());

    // Header
    let header = Paragraph::new(format!("FilePilot - {}", app.explorer.current_path().display()))
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::NONE));
    f.render_widget(header, chunks[0]);

    // Main content
    if (app.search_mode || app.showing_search_results) && !app.search_results.is_empty() {
        render_search_results(f, app, chunks[1]);
    } else {
        render_file_list(f, app, chunks[1]);
    }

    // Footer
    render_footer(f, app, chunks[2]);

    // Search input overlay
    if app.search_mode {
        render_search_input(f, app);
    }
}

fn render_file_list(f: &mut Frame, app: &App, area: Rect) {
    // Split the area into two columns: file list (60%) and preview (40%)
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(60),
            Constraint::Percentage(40),
        ])
        .split(area);

    // Render file list in the left column
    let items: Vec<ListItem> = app
        .explorer
        .files()
        .iter()
        .map(|file| {
            let icon = if file.is_directory { "📁" } else { "📄" };
            let style = if file.is_directory {
                Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            
            // Show file info as light gray text
            let mut info_parts = Vec::new();
            if !file.is_directory {
                info_parts.push(format_size(file.size));
            }
            if let Some(modified) = file.modified {
                if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                    let days_ago = duration.as_secs() / (24 * 60 * 60);
                    if days_ago == 0 {
                        info_parts.push("today".to_string());
                    } else if days_ago < 7 {
                        info_parts.push(format!("{}d ago", days_ago));
                    } else {
                        info_parts.push(format!("{}w ago", days_ago / 7));
                    }
                }
            }
            let info_str = if info_parts.is_empty() {
                String::new()
            } else {
                format!(" ({})", info_parts.join(", "))
            };
            
            ListItem::new(Line::from(vec![
                Span::raw(icon),
                Span::raw(" "),
                Span::styled(&file.name, style),
                Span::styled(info_str, Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Files"))
        .highlight_style(Style::default().bg(Color::DarkGray))
        .highlight_symbol("► ");

    f.render_stateful_widget(list, chunks[0], &mut app.list_state.clone());

    // Render preview in the right column
    let preview_lines = app.get_file_preview();
    let preview_items: Vec<ListItem> = preview_lines
        .iter()
        .map(|line| ListItem::new(line.as_str()))
        .collect();

    let preview_block = Block::default()
        .borders(Borders::ALL)
        .title(" Preview ")
        .border_style(Style::default().fg(Color::Green));

    let preview_list = List::new(preview_items).block(preview_block);
    f.render_widget(preview_list, chunks[1]);
}

fn render_search_results(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .search_results
        .iter()
        .map(|result| {
            let icon = if result.file_info.is_directory { "📁" } else { "📄" };
            
            // Show match type with different colors
            let match_indicator = match result.match_type {
                crate::search::MatchType::FileName => Span::styled("F", Style::default().fg(Color::Green)),
                crate::search::MatchType::FilePath => Span::styled("P", Style::default().fg(Color::Yellow)),
            };
            
            ListItem::new(Line::from(vec![
                Span::raw(icon),
                Span::raw(" "),
                match_indicator,
                Span::raw(" "),
                Span::raw(result.file_info.path.to_string_lossy()),
                Span::styled(format!(" ({})", result.score), Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();

    let title = format!("Search Results - F:FileName P:Path");
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().bg(Color::DarkGray))
        .highlight_symbol("► ");

    f.render_stateful_widget(list, area, &mut app.search_list_state.clone());
}

// Helper function to format file sizes
fn format_size(size: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = size as f64;
    let mut unit_index = 0;
    
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }
    
    if unit_index == 0 {
        format!("{:.0}{}", size, UNITS[unit_index])
    } else {
        format!("{:.1}{}", size, UNITS[unit_index])
    }
}

fn render_footer(f: &mut Frame, app: &App, area: Rect) {
    let kb = &app.config.key_bindings;
    let text = if app.search_mode {
        format!(
            "{}: Exit search | {}: Exit to results | {}: Toggle strategy | {}: Navigate | {}: Browse",
            kb.get_key_display(&kb.search_mode.exit_search),
            kb.get_key_display(&kb.search_mode.exit_to_results),
            kb.get_key_display(&kb.search_mode.toggle_strategy),
            kb.get_key_display(&kb.search_mode.navigate_tab),
            kb.get_key_display(&kb.navigation.up)
        )
    } else if app.showing_search_results {
        let clipboard_status = if let Some(clipboard) = &app.clipboard {
            let operation = match clipboard.operation {
                ClipboardOperation::Cut => "CUT",
                ClipboardOperation::Copy => "COPIED",
            };
            let file_name = clipboard.file_path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?");
            format!(" | {}: {} [{}]", 
                    kb.get_key_display(&kb.actions.paste), 
                    operation, 
                    file_name)
        } else {
            String::new()
        };
        
        format!(
            "{}: Quit | {}: New search | {}: Back | {}: Navigate | {}: Open/Navigate | {}: Open | {}: Reveal | {}: Share | {}: Cut | {}: Copy | {}: Copy path{}",
            kb.get_key_display(&kb.actions.quit),
            kb.get_key_display(&kb.actions.search),
            kb.get_key_display(&kb.search_results.back),
            kb.get_key_display(&kb.navigation.up),
            kb.get_key_display(&kb.navigation.enter),
            kb.get_key_display(&kb.actions.open),
            kb.get_key_display(&kb.actions.reveal),
            kb.get_key_display(&kb.actions.share),
            kb.get_key_display(&kb.actions.cut),
            kb.get_key_display(&kb.actions.copy),
            kb.get_key_display(&kb.actions.copy_path),
            clipboard_status
        )
    } else {
        let clipboard_status = if let Some(clipboard) = &app.clipboard {
            let operation = match clipboard.operation {
                ClipboardOperation::Cut => "CUT",
                ClipboardOperation::Copy => "COPIED",
            };
            let file_name = clipboard.file_path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?");
            format!(" | {}: {} [{}]", 
                    kb.get_key_display(&kb.actions.paste), 
                    operation, 
                    file_name)
        } else {
            String::new()
        };
        
        format!(
            "{}: Quit | {}: Search | {}: Navigate | {}: Open/Navigate | {}: Go up | {}: Open | {}: Reveal | {}: Share | {}: Cut | {}: Copy | {}: Copy path{}",
            kb.get_key_display(&kb.actions.quit),
            kb.get_key_display(&kb.actions.search),
            kb.get_key_display(&kb.navigation.up),
            kb.get_key_display(&kb.navigation.enter),
            kb.get_key_display(&kb.navigation.left),
            kb.get_key_display(&kb.actions.open),
            kb.get_key_display(&kb.actions.reveal),
            kb.get_key_display(&kb.actions.share),
            kb.get_key_display(&kb.actions.cut),
            kb.get_key_display(&kb.actions.copy),
            kb.get_key_display(&kb.actions.copy_path),
            clipboard_status
        )
    };
    
    let footer = Paragraph::new(vec![
        Line::from(text),
        Line::from(Span::styled(app.get_current_message(), app.get_message_style())),
    ])
    .block(Block::default().borders(Borders::ALL).title("Controls"));
    
    f.render_widget(footer, area);
}

fn render_search_input(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 3, f.size());
    
    f.render_widget(Clear, area);
    
    let title = format!("Search - {}", app.search_strategy.description());
    let input = Paragraph::new(app.search_input.as_str())
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::ALL).title(title));
    
    f.render_widget(input, area);
}

fn centered_rect(percent_x: u16, height: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height) / 2),
            Constraint::Length(height),
            Constraint::Percentage((100 - height) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
