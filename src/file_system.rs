use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub path: PathBuf,
    pub name: String,
    pub is_directory: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

impl FileInfo {
    pub fn from_path(path: &Path) -> Result<Self, std::io::Error> {
        let metadata = fs::metadata(path)?;
        
        Ok(FileInfo {
            path: path.to_path_buf(),
            name: path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string(),
            is_directory: metadata.is_dir(),
            size: metadata.len(),
            modified: metadata.modified().ok(),
        })
    }
}

pub struct FileExplorer {
    current_path: PathBuf,
    files: Vec<FileInfo>,
}

impl FileExplorer {
    pub fn new(path: PathBuf) -> Result<Self, std::io::Error> {
        let mut explorer = FileExplorer {
            current_path: path.canonicalize()?,
            files: Vec::new(),
        };
        explorer.refresh()?;
        Ok(explorer)
    }

    pub fn current_path(&self) -> &Path {
        &self.current_path
    }

    pub fn files(&self) -> &[FileInfo] {
        &self.files
    }

    pub fn navigate_to(&mut self, path: PathBuf) -> Result<(), std::io::Error> {
        if path.is_dir() {
            self.current_path = path.canonicalize()?;
            self.refresh()?;
        }
        Ok(())
    }

    pub fn go_up(&mut self) -> Result<(), std::io::Error> {
        if let Some(parent) = self.current_path.parent() {
            self.current_path = parent.to_path_buf();
            self.refresh()?;
        }
        Ok(())
    }

    pub fn refresh(&mut self) -> Result<(), std::io::Error> {
        self.files.clear();
        
        for entry in fs::read_dir(&self.current_path)? {
            let entry = entry?;
            if let Ok(file_info) = FileInfo::from_path(&entry.path()) {
                self.files.push(file_info);
            }
        }

        // Sort: directories first, then by name
        self.files.sort_by(|a, b| {
            match (a.is_directory, b.is_directory) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            }
        });

        Ok(())
    }

    pub fn open_file(&self, file_info: &FileInfo) -> Result<(), std::io::Error> {
        if file_info.is_directory {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Cannot open directory as file. Use navigate_to instead.",
            ));
        }

        // Use the system's default application to open the file
        match open::that(&file_info.path) {
            Ok(_) => Ok(()),
            Err(e) => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to open file: {}", e),
            )),
        }
    }

    pub fn reveal_in_file_manager(&self, file_info: &FileInfo) -> Result<(), std::io::Error> {
        // On most systems, this will open the file manager and highlight the file
        let path_to_reveal = if file_info.is_directory {
            &file_info.path
        } else {
            // For files, reveal the parent directory
            file_info.path.parent().unwrap_or(&file_info.path)
        };

        match open::that(path_to_reveal) {
            Ok(_) => Ok(()),
            Err(e) => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to reveal in file manager: {}", e),
            )),
        }
    }
}
