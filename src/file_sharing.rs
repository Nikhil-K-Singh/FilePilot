use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use warp::Filter;
use uuid::Uuid;
use arboard::Clipboard;
use local_ip_address::local_ip;
use csv::ReaderBuilder;
use calamine::{Reader, Xlsx, Xls, open_workbook};
use serde::{Deserialize, Serialize};
use crate::config::Config;

// Size limits for different file types
const MAX_JSON_CLIENT_SIZE: u64 = 5 * 1024 * 1024; // 5MB limit for client-side JSON processing
const MAX_NOTEBOOK_SIZE: u64 = 50 * 1024 * 1024; // 50MB limit for notebooks
const MAX_MARKDOWN_SIZE: u64 = 5 * 1024 * 1024; // 5MB limit for markdown
const MAX_SPREADSHEET_SIZE: u64 = 10 * 1024 * 1024; // 10MB limit for spreadsheets
const MAX_CSV_ROWS: usize = 1000; // Maximum rows to display for CSV
const MAX_EXCEL_ROWS: usize = 1000; // Maximum rows to display for Excel

#[derive(Clone, Serialize, Deserialize)]
pub struct FileShareNotification {
    pub file_id: String,
    pub file_name: String,
    pub file_path: String,
    pub share_url: String,
    pub file_size: Option<u64>,
    pub mime_type: String,
    pub timestamp: u64,
}

#[derive(Clone)]
struct FileInfo {
    id: String,
    name: String,
    path: String,
}

pub struct FileShareServer {
    shared_files: Arc<RwLock<HashMap<String, PathBuf>>>,
    server_port: u16,
    is_running: Arc<RwLock<bool>>,
    config: Config,
}

impl FileShareServer {
    pub fn new() -> Self {
        Self {
            shared_files: Arc::new(RwLock::new(HashMap::new())),
            server_port: 8080, // Default port
            is_running: Arc::new(RwLock::new(false)),
            config: Config::load_default(),
        }
    }

    async fn send_notification(&self, notification: FileShareNotification) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !self.config.notification_enabled {
            return Ok(());
        }

        let Some(endpoint) = &self.config.notification_endpoint else {
            return Ok(());
        };

        let client = reqwest::Client::builder()
            .build()?;

        // Try to send the notification - if it fails, we'll return the error
        // so the UI can display a warning message that will fade away
        let response = client
            .post(endpoint)
            .json(&notification)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!("Notification endpoint returned status: {}", response.status()).into());
        }

        Ok(())
    }

    pub async fn start_server(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        {
            let is_running = self.is_running.read().await;
            if *is_running {
                return Ok(()); // Server already running
            }
        }

        let shared_files = self.shared_files.clone();
        let shared_files_for_list = self.shared_files.clone();
        let shared_files_for_raw = self.shared_files.clone();
        let shared_files_for_download = self.shared_files.clone();
        let is_running_clone = self.is_running.clone();

        // Find an available port
        let port = self.find_available_port().await?;
        
        // Main file route - serves HTML viewer pages
        let files_route = warp::path("file")
            .and(warp::path::param::<String>())
            .and_then(move |file_id: String| {
                let shared_files = shared_files.clone();
                async move {
                    let files = shared_files.read().await;
                    if let Some(file_path) = files.get(&file_id) {
                        if file_path.exists() && file_path.is_file() {
                            // Create FileInfo for the viewer
                            let file_info = FileInfo {
                                id: file_id.clone(),
                                name: file_path.file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("unknown")
                                    .to_string(),
                                path: file_path.to_string_lossy().to_string(),
                            };
                            // Generate HTML viewer page for this file
                            let html = create_file_viewer_page(&file_info);
                            Ok(warp::reply::html(html))
                        } else {
                            Err(warp::reject::not_found())
                        }
                    } else {
                        Err(warp::reject::not_found())
                    }
                }
            });

        // Raw file route - serves actual file content for embedding/downloading
        let raw_route = warp::path("raw")
            .and(warp::path::param::<String>())
            .and(warp::header::optional::<String>("range"))
            .and_then(move |file_id: String, range_header: Option<String>| {
                let shared_files = shared_files_for_raw.clone();
                async move {
                    let files = shared_files.read().await;
                    if let Some(file_path) = files.get(&file_id) {
                        if file_path.exists() && file_path.is_file() {
                            let mime_type = get_mime_type(file_path);
                            
                            // Get file metadata
                            let metadata = tokio::fs::metadata(file_path).await
                                .map_err(|_| warp::reject::not_found())?;
                            let file_size = metadata.len();
                            
                            // Handle range requests for video streaming
                            if let Some(range) = range_header {
                                if let Some((start, end)) = parse_range(&range, file_size) {
                                    let mut file = tokio::fs::File::open(file_path).await
                                        .map_err(|_| warp::reject::not_found())?;
                                    
                                    // Seek to start position
                                    use tokio::io::AsyncSeekExt;
                                    file.seek(std::io::SeekFrom::Start(start)).await
                                        .map_err(|_| warp::reject::not_found())?;
                                    
                                    // Take only the requested range
                                    let content_length = end - start + 1;
                                    let limited_file = tokio::io::AsyncReadExt::take(file, content_length);
                                    let stream = tokio_util::io::ReaderStream::new(limited_file);
                                    let body = warp::hyper::Body::wrap_stream(stream);
                                    
                                    let response = warp::http::Response::builder()
                                        .status(206) // Partial Content
                                        .header("Content-Type", mime_type)
                                        .header("Content-Length", content_length.to_string())
                                        .header("Content-Range", format!("bytes {}-{}/{}", start, end, file_size))
                                        .header("Accept-Ranges", "bytes")
                                        .header("Cache-Control", "public, max-age=3600")
                                        .header("Access-Control-Allow-Origin", "*")
                                        .body(body)
                                        .map_err(|_| warp::reject::not_found())?;
                                    
                                    return Ok(response);
                                }
                            }
                            
                            // Serve full file if no range request
                            let file = tokio::fs::File::open(file_path).await
                                .map_err(|_| warp::reject::not_found())?;
                            
                            let stream = tokio_util::io::ReaderStream::new(file);
                            let body = warp::hyper::Body::wrap_stream(stream);
                            
                            let response = warp::http::Response::builder()
                                .header("Content-Type", mime_type)
                                .header("Content-Length", file_size.to_string())
                                .header("Cache-Control", "public, max-age=3600")
                                .header("Accept-Ranges", "bytes")
                                .header("Access-Control-Allow-Origin", "*")
                                .body(body)
                                .map_err(|_| warp::reject::not_found())?;
                            
                            Ok(response)
                        } else {
                            Err(warp::reject::not_found())
                        }
                    } else {
                        Err(warp::reject::not_found())
                    }
                }
            });

        // Download route - forces file download with proper filename
        let download_route = warp::path("download")
            .and(warp::path::param::<String>())
            .and_then(move |file_id: String| {
                let shared_files = shared_files_for_download.clone();
                async move {
                    let files = shared_files.read().await;
                    if let Some(file_path) = files.get(&file_id) {
                        if file_path.exists() && file_path.is_file() {
                            let file = tokio::fs::File::open(file_path).await
                                .map_err(|_| warp::reject::not_found())?;
                            
                            let stream = tokio_util::io::ReaderStream::new(file);
                            let body = warp::hyper::Body::wrap_stream(stream);
                            
                            let filename = file_path.file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("download");
                            
                            let mime_type = get_mime_type(file_path);
                            
                            // Force download with proper filename
                            let response = warp::http::Response::builder()
                                .header("Content-Type", mime_type)
                                .header("Content-Disposition", format!("attachment; filename=\"{}\"", filename))
                                .body(body)
                                .map_err(|_| warp::reject::not_found())?;
                            
                            Ok(response)
                        } else {
                            Err(warp::reject::not_found())
                        }
                    } else {
                        Err(warp::reject::not_found())
                    }
                }
            });

        let list_route = warp::path("list")
            .and_then(move || {
                let shared_files = shared_files_for_list.clone();
                async move {
                    let files = shared_files.read().await;
                    let file_list: Vec<_> = files.iter()
                        .map(|(id, path)| {
                            let name = path.file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("unknown");
                            
                            // Create different display based on file type
                            if should_display_inline(path) {
                                let extension = path.extension()
                                    .and_then(|ext| ext.to_str())
                                    .unwrap_or("")
                                    .to_lowercase();
                                
                                match extension.as_str() {
                                    "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "svg" => {
                                        format!(
                                            "<li><strong>{}</strong><br/>\
                                            <a href=\"/file/{}\" target=\"_blank\">\
                                            <img src=\"/raw/{}\" alt=\"{}\" style=\"max-width: 200px; max-height: 150px; border: 1px solid #ccc; margin: 5px;\"/>\
                                            </a></li>", 
                                            name, id, id, name
                                        )
                                    },
                                    "mp4" | "webm" | "ogv" | "mov" | "avi" | "mkv" | "m4v" | "wmv" | "flv" => {
                                        format!(
                                            "<li><strong>{}</strong><br/>\
                                            <video controls style=\"max-width: 300px; margin: 5px;\">\
                                            <source src=\"/raw/{}\" type=\"{}\">\
                                            Your browser does not support the video tag.\
                                            </video><br/>\
                                            <a href=\"/file/{}\" target=\"_blank\">View Full</a></li>", 
                                            name, id, get_mime_type(path), id
                                        )
                                    },
                                    "mp3" | "wav" | "m4a" | "aac" | "oga" | "ogg" | "flac" => {
                                        format!(
                                            "<li><strong>{}</strong><br/>\
                                            <audio controls style=\"margin: 5px; width: 300px;\">\
                                            <source src=\"/raw/{}\" type=\"{}\">\
                                            Your browser does not support the audio tag.\
                                            </audio><br/>\
                                            <a href=\"/file/{}\" target=\"_blank\">View Full</a></li>", 
                                            name, id, get_mime_type(path), id
                                        )
                                    },
                                    "json" | "geojson" | "xml" | "ipynb" => {
                                        let display_type = match extension.as_str() {
                                            "ipynb" => "Jupyter Notebook",
                                            _ => &format!("{} file", extension.to_uppercase())
                                        };
                                        format!(
                                            "<li><strong>{}</strong> - <em>{}</em><br/>\
                                            <a href=\"/file/{}\" target=\"_blank\">📄 View {} content</a> | \
                                            <a href=\"/download/{}\">⬇️ Download</a></li>", 
                                            name, display_type, id, extension.to_uppercase(), id
                                        )
                                    },
                                    "csv" | "xlsx" | "xls" => {
                                        let display_type = match extension.as_str() {
                                            "csv" => "CSV spreadsheet",
                                            "xlsx" => "Excel spreadsheet",
                                            "xls" => "Excel spreadsheet (legacy)",
                                            _ => "Spreadsheet"
                                        };
                                        format!(
                                            "<li><strong>{}</strong> - <em>{}</em><br/>\
                                            <a href=\"/file/{}\" target=\"_blank\">📊 View table data</a> | \
                                            <a href=\"/download/{}\">⬇️ Download</a></li>", 
                                            name, display_type, id, id
                                        )
                                    },
                                    "py" | "rs" | "js" | "html" | "css" | "c" | "cpp" | "java" | "go" | "php" => {
                                        format!(
                                            "<li><strong>{}</strong> - <em>{} source code</em><br/>\
                                            <a href=\"/file/{}\" target=\"_blank\">💻 View code</a> | \
                                            <a href=\"/download/{}\">⬇️ Download</a></li>", 
                                            name, extension.to_uppercase(), id, id
                                        )
                                    },
                                    "md" => {
                                        format!(
                                            "<li><strong>{}</strong> - <em>Markdown document</em><br/>\
                                            <a href=\"/file/{}\" target=\"_blank\">📝 View rendered</a> | \
                                            <a href=\"/download/{}\">⬇️ Download</a></li>", 
                                            name, id, id
                                        )
                                    },
                                    "pdf" => {
                                        format!(
                                            "<li><strong>{}</strong> - <em>PDF document</em><br/>\
                                            <a href=\"/file/{}\" target=\"_blank\">📋 View PDF</a> | \
                                            <a href=\"/download/{}\">⬇️ Download</a></li>", 
                                            name, id, id
                                        )
                                    },
                                    _ => {
                                        format!("<li><a href=\"/file/{}\" target=\"_blank\">{}</a></li>", id, name)
                                    }
                                }
                            } else {
                                format!("<li><a href=\"/file/{}\" download=\"{}\">{} (download)</a></li>", id, name, name)
                            }
                        })
                        .collect();
                    
                    let html = format!(
                        "<!DOCTYPE html>\
                        <html><head>\
                        <title>FilePilot - Shared Files</title>\
                        <meta charset=\"UTF-8\">\
                        <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\
                        <style>\
                        body {{ font-family: Arial, sans-serif; margin: 20px; background-color: #1a1a1a; color: #e0e0e0; }}\
                        h1 {{ color: #ffffff; border-bottom: 2px solid #0d7377; padding-bottom: 10px; }}\
                        ul {{ list-style-type: none; padding: 0; }}\
                        li {{ background: #2d2d2d; margin: 10px 0; padding: 15px; border-radius: 8px; box-shadow: 0 2px 4px rgba(0,0,0,0.3); }}\
                        a {{ color: #58a6ff; text-decoration: none; }}\
                        a:hover {{ text-decoration: underline; }}\
                        img {{ border-radius: 4px; }}\
                        video, audio {{ border-radius: 4px; }}\
                        </style>\
                        </head><body>\
                        <h1>🚀 FilePilot - Shared Files</h1>\
                        <p>Files shared from your FilePilot file explorer:</p>\
                        <ul>{}</ul>\
                        </body></html>",
                        file_list.join("")
                    );
                    
                    Ok::<_, warp::Rejection>(warp::reply::html(html))
                }
            });

        let routes = files_route.or(raw_route).or(download_route).or(list_route);

        let addr: SocketAddr = ([0, 0, 0, 0], port).into();
        
        // Start server in background
        tokio::spawn(async move {
            warp::serve(routes).run(addr).await;
            let mut running = is_running_clone.write().await;
            *running = false;
        });

        {
            let mut is_running = self.is_running.write().await;
            *is_running = true;
        }
        Ok(())
    }

    pub async fn share_file(&mut self, file_path: &Path) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        if !file_path.exists() {
            return Err("File does not exist".into());
        }

        if file_path.is_dir() {
            return Err("Cannot share directories (yet)".into());
        }

        // Start server if not running
        self.start_server().await?;

        // Generate unique ID for this file
        let file_id = Uuid::new_v4().to_string();
        
        // Add file to shared files
        let mut shared_files = self.shared_files.write().await;
        shared_files.insert(file_id.clone(), file_path.to_path_buf());
        drop(shared_files); // Release the lock early

        // Get local IP
        let local_ip = local_ip().unwrap_or_else(|_| "127.0.0.1".parse().unwrap());
        
        // Create shareable URL
        let url = format!("http://{}:{}/file/{}", local_ip, self.server_port, file_id);

        // Copy to clipboard
        if let Ok(mut clipboard) = Clipboard::new() {
            let _ = clipboard.set_text(&url);
        }

        // Get file metadata for notification
        let file_size = std::fs::metadata(file_path).ok().map(|m| m.len());
        let file_name = file_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        let mime_type = get_mime_type(file_path).to_string();

        // Create and send notification
        let notification = FileShareNotification {
            file_id: file_id.clone(),
            file_name,
            file_path: file_path.to_string_lossy().to_string(),
            share_url: url.clone(),
            file_size,
            mime_type,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };

        // Send notification (non-blocking)
        let notification_result = self.send_notification(notification).await;

        // Return URL with optional warning about notification failure
        match notification_result {
            Ok(()) => Ok(url),
            Err(e) => {
                // Return success with a warning message that will fade
                Ok(format!("{} (Warning: {})", url, e))
            }
        }
    }

    async fn find_available_port(&mut self) -> Result<u16, Box<dyn std::error::Error + Send + Sync>> {
        // Try ports starting from 8080
        for port in 8080..8090 {
            if self.is_port_available(port).await {
                self.server_port = port;
                return Ok(port);
            }
        }
        Err("No available ports found".into())
    }

    async fn is_port_available(&self, port: u16) -> bool {
        use std::net::TcpListener;
        TcpListener::bind(("127.0.0.1", port)).is_ok()
    }
}

fn should_display_inline(path: &Path) -> bool {
    let extension = path.extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_lowercase();

    match extension.as_str() {
        // Images - display inline
        "jpg" | "jpeg" | "png" | "gif" | "svg" | "webp" | "bmp" | "ico" => true,
        // Videos - stream inline
        "mp4" | "webm" | "ogv" | "mov" | "avi" | "mkv" | "m4v" | "wmv" | "flv" => true,
        // Audio - play inline
        "mp3" | "wav" | "m4a" | "aac" | "oga" | "ogg" | "flac" => true,
        // Text/Code files - display inline
        "txt" | "md" | "rst" | "log" | "json" | "geojson" | "xml" | "html" | "htm" | "css" | "js" | "ipynb" => true,
        // Programming languages - display inline
        "py" | "rs" | "c" | "cpp" | "h" | "java" | "go" | "php" | "rb" | "swift" | "kt" => true,
        // Config files - display inline
        "yml" | "yaml" | "toml" | "ini" | "cfg" | "conf" => true,
        // Spreadsheet files - display inline
        "csv" | "xlsx" | "xls" => true,
        // PDFs - display inline
        "pdf" => true,
        // Everything else - download
        _ => false,
    }
}

fn get_mime_type(path: &Path) -> &'static str {
    let extension = path.extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_lowercase();

    match extension.as_str() {
        // Web files
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" => "application/javascript",
        "xml" => "application/xml",
        // Text files
        "txt" | "md" | "rst" | "log" => "text/plain",
        "json" => "application/json",
        "geojson" => "application/geo+json",
        "ipynb" => "application/x-ipynb+json",
        // Programming languages
        "py" => "text/x-python",
        "rs" => "text/x-rust",
        "go" => "text/x-go",
        "php" => "text/x-php",
        "rb" => "text/x-ruby",
        "swift" => "text/x-swift",
        "kt" => "text/x-kotlin",
        // Config files
        "yml" | "yaml" => "text/x-yaml",
        "toml" => "text/x-toml",
        "ini" | "cfg" | "conf" => "text/plain",
        // Images
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        // Videos
        "mp4" => "video/mp4",
        "mkv" | "webm" => "video/webm",
        "ogv" => "video/ogg",
        "mov" => "video/quicktime",
        "avi" => "video/x-msvideo",
        "m4v" => "video/mp4",
        "wmv" => "video/x-ms-wmv",
        "flv" => "video/x-flv",
        // Audio
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "oga" => "audio/ogg",
        "ogg" => "audio/ogg", // Default OGG to audio
        "flac" => "audio/flac",
        // Documents
        "pdf" => "application/pdf",
        // Spreadsheets
        "csv" => "text/csv",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "xls" => "application/vnd.ms-excel",
        // Archives
        "zip" => "application/zip",
        "tar" => "application/x-tar",
        "gz" => "application/gzip",
        _ => "application/octet-stream",
    }
}

fn parse_csv_to_html(file_path: &Path, max_rows: usize) -> Result<String, Box<dyn std::error::Error>> {
    let file = std::fs::File::open(file_path)?;
    let mut reader = ReaderBuilder::new()
        .has_headers(true)
        .from_reader(file);
    
    let headers = reader.headers()?.clone();
    let mut html = String::new();
    
    // Table start with styling
    html.push_str(r#"<div class="table-container">
        <table class="data-table">
            <thead>
                <tr>"#);
    
    // Add headers
    for header in headers.iter() {
        html.push_str(&format!("<th>{}</th>", escape_html(header)));
    }
    html.push_str("</tr></thead><tbody>");
    
    // Add data rows (limited)
    let mut row_count = 0;
    for result in reader.records() {
        if row_count >= max_rows {
            html.push_str(&format!(
                r#"<tr><td colspan="{}" style="text-align: center; font-style: italic; color: #ffeb3b;">
                ... and {} more rows (showing first {} rows)
                </td></tr>"#, 
                headers.len(), 
                reader.records().count(), 
                max_rows
            ));
            break;
        }
        
        let record = result?;
        html.push_str("<tr>");
        for field in record.iter() {
            html.push_str(&format!("<td>{}</td>", escape_html(field)));
        }
        html.push_str("</tr>");
        row_count += 1;
    }
    
    html.push_str("</tbody></table></div>");
    Ok(html)
}

fn parse_excel_to_html(file_path: &Path, max_rows: usize) -> Result<String, Box<dyn std::error::Error>> {
    let extension = file_path.extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_lowercase();
    
    let mut html = String::new();
    
    match extension.as_str() {
        "xlsx" => {
            let mut workbook: Xlsx<_> = open_workbook(file_path)?;
            let sheet_names = workbook.sheet_names().to_owned();
            
            if sheet_names.is_empty() {
                return Ok("<p>No sheets found in workbook</p>".to_string());
            }
            
            // Process first sheet
            let sheet_name = &sheet_names[0];
            if let Ok(range) = workbook.worksheet_range(sheet_name) {
                html.push_str(&format!("<h3>Sheet: {}</h3>", escape_html(sheet_name)));
                html.push_str(r#"<div class="table-container">
                    <table class="data-table">
                        <tbody>"#);
                
                let mut row_count = 0;
                for row in range.rows() {
                    if row_count >= max_rows {
                        html.push_str(&format!(
                            r#"<tr><td colspan="{}" style="text-align: center; font-style: italic; color: #ffeb3b;">
                            ... and more rows (showing first {} rows)
                            </td></tr>"#, 
                            row.len(), 
                            max_rows
                        ));
                        break;
                    }
                    
                    html.push_str("<tr>");
                    for cell in row {
                        let cell_value = format!("{}", cell);
                        html.push_str(&format!("<td>{}</td>", escape_html(&cell_value)));
                    }
                    html.push_str("</tr>");
                    row_count += 1;
                }
                
                html.push_str("</tbody></table></div>");
            }
        },
        "xls" => {
            let mut workbook: Xls<_> = open_workbook(file_path)?;
            let sheet_names = workbook.sheet_names().to_owned();
            
            if sheet_names.is_empty() {
                return Ok("<p>No sheets found in workbook</p>".to_string());
            }
            
            // Process first sheet
            let sheet_name = &sheet_names[0];
            if let Ok(range) = workbook.worksheet_range(sheet_name) {
                html.push_str(&format!("<h3>Sheet: {}</h3>", escape_html(sheet_name)));
                html.push_str(r#"<div class="table-container">
                    <table class="data-table">
                        <tbody>"#);
                
                let mut row_count = 0;
                for row in range.rows() {
                    if row_count >= max_rows {
                        html.push_str(&format!(
                            r#"<tr><td colspan="{}" style="text-align: center; font-style: italic; color: #ffeb3b;">
                            ... and more rows (showing first {} rows)
                            </td></tr>"#, 
                            row.len(), 
                            max_rows
                        ));
                        break;
                    }
                    
                    html.push_str("<tr>");
                    for cell in row {
                        let cell_value = format!("{}", cell);
                        html.push_str(&format!("<td>{}</td>", escape_html(&cell_value)));
                    }
                    html.push_str("</tr>");
                    row_count += 1;
                }
                
                html.push_str("</tbody></table></div>");
            }
        },
        _ => return Err("Unsupported Excel format".into()),
    }
    
    Ok(html)
}

fn create_file_viewer_page(file_info: &FileInfo) -> String {
    let extension = Path::new(&file_info.name)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_lowercase();

    let viewer_content = match extension.as_str() {
        // Video files
        "mp4" | "webm" | "ogv" | "mov" | "avi" | "mkv" | "m4v" | "wmv" | "flv" => {
            format!(
                r#"<video controls autoplay name="media" style="width: 100%; max-width: 800px; height: auto;">
                    <source src="/raw/{}" type="{}">
                    Your browser does not support the video tag.
                </video>
                <br><br>
                <p><a href="/download/{}" class="download-btn">Download Video</a></p>"#,
                file_info.id, get_mime_type(&Path::new(&file_info.name)), file_info.id
            )
        },
        // Audio files
        "mp3" | "wav" | "m4a" | "aac" | "flac" | "oga" | "ogg" => {
            format!(
                r#"<div class="audio-viewer">
                    <audio controls style="width: 100%; max-width: 600px;">
                        <source src="/raw/{}" type="{}">
                        Your browser does not support the audio tag.
                    </audio>
                    <br><br>
                    <p><a href="/download/{}" class="download-btn">Download Audio</a></p>
                </div>"#,
                file_info.id, get_mime_type(&Path::new(&file_info.name)), file_info.id
            )
        },
        // Image files
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "svg" => {
            format!(
                r#"<img src="/raw/{}" alt="{}" style="max-width: 100%; height: auto; border: 1px solid #ddd; border-radius: 5px;">
                <br><br>
                <p><a href="/download/{}" class="download-btn">Download Image</a></p>"#,
                file_info.id, file_info.name, file_info.id
            )
        },
        // JSON files - formatted display
        "json" => {
            // Check file size first
            let file_path = Path::new(&file_info.path);
            if let Ok(metadata) = std::fs::metadata(file_path) {
                if metadata.len() > MAX_JSON_CLIENT_SIZE {
                    // For large JSON files, do server-side processing
                    let json_content = match std::fs::read_to_string(file_path) {
                        Ok(content) => {
                            match serde_json::from_str::<serde_json::Value>(&content) {
                                Ok(json_data) => {
                                    match serde_json::to_string_pretty(&json_data) {
                                        Ok(formatted) => format!(
                                            r#"<div class="json-viewer">
                                                <div style="text-align: left; max-width: 100%; overflow: auto;">
                                                    <pre><code class="language-json">{}</code></pre>
                                                </div>
                                                <br>
                                                <p>⚡ Large JSON file ({:.1} MB) - processed server-side for optimal performance</p>
                                                <p><a href="/download/{}" class="download-btn">Download JSON</a></p>
                                                <script>
                                                    // Apply syntax highlighting after content is loaded
                                                    Prism.highlightAll();
                                                </script>
                                            </div>"#,
                                            escape_html(&formatted), 
                                            metadata.len() as f64 / (1024.0 * 1024.0),
                                            file_info.id
                                        ),
                                        Err(_) => format!(
                                            r#"<div class="file-info">
                                                <h3>Large JSON File: {}</h3>
                                                <p>⚠️ JSON file too large for formatted preview ({:.1} MB)</p>
                                                <p>File contains malformed JSON that cannot be formatted.</p>
                                                <p><a href="/download/{}" class="download-btn">Download JSON</a></p>
                                                <p><a href="/raw/{}" target="_blank">View Raw Content</a></p>
                                            </div>"#,
                                            file_info.name, 
                                            metadata.len() as f64 / (1024.0 * 1024.0),
                                            file_info.id,
                                            file_info.id
                                        )
                                    }
                                },
                                Err(_) => format!(
                                    r#"<div class="file-info">
                                        <h3>Large JSON File: {}</h3>
                                        <p>⚠️ JSON file too large for formatted preview ({:.1} MB)</p>
                                        <p>File contains malformed JSON that cannot be parsed.</p>
                                        <p><a href="/download/{}" class="download-btn">Download JSON</a></p>
                                        <p><a href="/raw/{}" target="_blank">View Raw Content</a></p>
                                    </div>"#,
                                    file_info.name, 
                                    metadata.len() as f64 / (1024.0 * 1024.0),
                                    file_info.id,
                                    file_info.id
                                )
                            }
                        },
                        Err(_) => format!(
                            r#"<div class="file-info">
                                <h3>Error reading JSON file: {}</h3>
                                <p><a href="/download/{}" class="download-btn">Download File</a></p>
                            </div>"#,
                            file_info.name, file_info.id
                        )
                    };
                    json_content
                } else {
                    // For smaller JSON files, use client-side processing
                    format!(
                        r#"<div class="json-viewer">
                            <div style="text-align: left; max-width: 100%; overflow: auto;">
                                <pre><code class="language-json" id="code-content"></code></pre>
                            </div>
                            <br>
                            <p><a href="/download/{}" class="download-btn">Download JSON</a></p>
                            <script>
                                fetch('/raw/{}')
                                    .then(response => response.text())
                                    .then(data => {{
                                        try {{
                                            // Parse and format JSON with indentation
                                            const jsonData = JSON.parse(data);
                                            const formattedJson = JSON.stringify(jsonData, null, 2);
                                            document.getElementById('code-content').textContent = formattedJson;
                                        }} catch (e) {{
                                            // If parsing fails, display raw content
                                            document.getElementById('code-content').textContent = data;
                                        }}
                                        Prism.highlightAll();
                                    }});
                            </script>
                        </div>"#,
                        file_info.id, file_info.id
                    )
                }
            } else {
                format!(
                    r#"<div class="file-info">
                        <h3>Error reading file: {}</h3>
                        <p><a href="/download/{}" class="download-btn">Download File</a></p>
                    </div>"#,
                    file_info.name, file_info.id
                )
            }
        },
        // GeoJSON files - formatted display with JSON highlighting
        "geojson" => {
            // Check file size first
            let file_path = Path::new(&file_info.path);
            if let Ok(metadata) = std::fs::metadata(file_path) {
                if metadata.len() > MAX_JSON_CLIENT_SIZE {
                    // For large GeoJSON files, do server-side processing
                    let geojson_content = match std::fs::read_to_string(file_path) {
                        Ok(content) => {
                            match serde_json::from_str::<serde_json::Value>(&content) {
                                Ok(geojson_data) => {
                                    match serde_json::to_string_pretty(&geojson_data) {
                                        Ok(formatted) => format!(
                                            r#"<div class="json-viewer">
                                                <div style="text-align: left; max-width: 100%; overflow: auto;">
                                                    <pre><code class="language-json">{}</code></pre>
                                                </div>
                                                <br>
                                                <p>⚡ Large GeoJSON file ({:.1} MB) - processed server-side for optimal performance</p>
                                                <p><a href="/download/{}" class="download-btn">Download GeoJSON</a></p>
                                                <script>
                                                    // Apply syntax highlighting after content is loaded
                                                    Prism.highlightAll();
                                                </script>
                                            </div>"#,
                                            escape_html(&formatted), 
                                            metadata.len() as f64 / (1024.0 * 1024.0),
                                            file_info.id
                                        ),
                                        Err(_) => format!(
                                            r#"<div class="file-info">
                                                <h3>Large GeoJSON File: {}</h3>
                                                <p>⚠️ GeoJSON file too large for formatted preview ({:.1} MB)</p>
                                                <p>File contains malformed GeoJSON that cannot be formatted.</p>
                                                <p><a href="/download/{}" class="download-btn">Download GeoJSON</a></p>
                                                <p><a href="/raw/{}" target="_blank">View Raw Content</a></p>
                                            </div>"#,
                                            file_info.name, 
                                            metadata.len() as f64 / (1024.0 * 1024.0),
                                            file_info.id,
                                            file_info.id
                                        )
                                    }
                                },
                                Err(_) => format!(
                                    r#"<div class="file-info">
                                        <h3>Large GeoJSON File: {}</h3>
                                        <p>⚠️ GeoJSON file too large for formatted preview ({:.1} MB)</p>
                                        <p>File contains malformed GeoJSON that cannot be parsed.</p>
                                        <p><a href="/download/{}" class="download-btn">Download GeoJSON</a></p>
                                        <p><a href="/raw/{}" target="_blank">View Raw Content</a></p>
                                    </div>"#,
                                    file_info.name, 
                                    metadata.len() as f64 / (1024.0 * 1024.0),
                                    file_info.id,
                                    file_info.id
                                )
                            }
                        },
                        Err(_) => format!(
                            r#"<div class="file-info">
                                <h3>Error reading GeoJSON file: {}</h3>
                                <p><a href="/download/{}" class="download-btn">Download File</a></p>
                            </div>"#,
                            file_info.name, file_info.id
                        )
                    };
                    geojson_content
                } else {
                    // For smaller GeoJSON files, use client-side processing
                    format!(
                        r#"<div class="json-viewer">
                            <div style="text-align: left; max-width: 100%; overflow: auto;">
                                <pre><code class="language-json" id="code-content"></code></pre>
                            </div>
                            <br>
                            <p><a href="/download/{}" class="download-btn">Download GeoJSON</a></p>
                            <script>
                                fetch('/raw/{}')
                                    .then(response => response.text())
                                    .then(data => {{
                                        try {{
                                            // Parse and format GeoJSON with indentation
                                            const geoJsonData = JSON.parse(data);
                                            const formattedGeoJson = JSON.stringify(geoJsonData, null, 2);
                                            document.getElementById('code-content').textContent = formattedGeoJson;
                                        }} catch (e) {{
                                            // If parsing fails, display raw content
                                            document.getElementById('code-content').textContent = data;
                                        }}
                                        Prism.highlightAll();
                                    }});
                            </script>
                        </div>"#,
                        file_info.id, file_info.id
                    )
                }
            } else {
                format!(
                    r#"<div class="file-info">
                        <h3>Error reading file: {}</h3>
                        <p><a href="/download/{}" class="download-btn">Download File</a></p>
                    </div>"#,
                    file_info.name, file_info.id
                )
            }
        },
        // XML files - formatted display
        "xml" => {
            format!(
                r#"<div class="xml-viewer">
                    <div style="text-align: left; max-width: 100%; overflow: auto;">
                        <pre><code class="language-xml" id="code-content"></code></pre>
                    </div>
                    <br>
                    <p><a href="/download/{}" class="download-btn">Download XML</a></p>
                    <script>
                        fetch('/raw/{}')
                            .then(response => response.text())
                            .then(data => {{
                                document.getElementById('code-content').textContent = data;
                                Prism.highlightAll();
                            }});
                    </script>
                </div>"#,
                file_info.id, file_info.id
            )
        },
        // Python files - syntax highlighted display
        "py" => {
            format!(
                r#"<div class="code-viewer">
                    <div style="text-align: left; max-width: 100%; overflow: auto;">
                        <pre><code class="language-python" id="code-content"></code></pre>
                    </div>
                    <br>
                    <p><a href="/download/{}" class="download-btn">Download Python File</a></p>
                    <script>
                        fetch('/raw/{}')
                            .then(response => response.text())
                            .then(data => {{
                                document.getElementById('code-content').textContent = data;
                                Prism.highlightAll();
                            }});
                    </script>
                </div>"#,
                file_info.id, file_info.id
            )
        },
        // Rust files
        "rs" => {
            format!(
                r#"<div class="code-viewer">
                    <div style="text-align: left; max-width: 100%; overflow: auto;">
                        <pre><code class="language-rust" id="code-content"></code></pre>
                    </div>
                    <br>
                    <p><a href="/download/{}" class="download-btn">Download Rust File</a></p>
                    <script>
                        fetch('/raw/{}')
                            .then(response => response.text())
                            .then(data => {{
                                document.getElementById('code-content').textContent = data;
                                Prism.highlightAll();
                            }});
                    </script>
                </div>"#,
                file_info.id, file_info.id
            )
        },
        // JavaScript files
        "js" => {
            format!(
                r#"<div class="code-viewer">
                    <div style="text-align: left; max-width: 100%; overflow: auto;">
                        <pre><code class="language-javascript" id="code-content"></code></pre>
                    </div>
                    <br>
                    <p><a href="/download/{}" class="download-btn">Download JavaScript File</a></p>
                    <script>
                        fetch('/raw/{}')
                            .then(response => response.text())
                            .then(data => {{
                                document.getElementById('code-content').textContent = data;
                                Prism.highlightAll();
                            }});
                    </script>
                </div>"#,
                file_info.id, file_info.id
            )
        },
        // HTML files
        "html" | "htm" => {
            format!(
                r#"<div class="code-viewer">
                    <div style="text-align: left; max-width: 100%; overflow: auto;">
                        <pre><code class="language-html" id="code-content"></code></pre>
                    </div>
                    <br>
                    <p><a href="/download/{}" class="download-btn">Download HTML File</a></p>
                    <script>
                        fetch('/raw/{}')
                            .then(response => response.text())
                            .then(data => {{
                                document.getElementById('code-content').textContent = data;
                                Prism.highlightAll();
                            }});
                    </script>
                </div>"#,
                file_info.id, file_info.id
            )
        },
        // CSS files
        "css" => {
            format!(
                r#"<div class="code-viewer">
                    <div style="text-align: left; max-width: 100%; overflow: auto;">
                        <pre><code class="language-css" id="code-content"></code></pre>
                    </div>
                    <br>
                    <p><a href="/download/{}" class="download-btn">Download CSS File</a></p>
                    <script>
                        fetch('/raw/{}')
                            .then(response => response.text())
                            .then(data => {{
                                document.getElementById('code-content').textContent = data;
                                Prism.highlightAll();
                            }});
                    </script>
                </div>"#,
                file_info.id, file_info.id
            )
        },
        // C/C++ files
        "c" | "cpp" | "h" => {
            let lang = if extension == "cpp" { "cpp" } else { "c" };
            format!(
                r#"<div class="code-viewer">
                    <div style="text-align: left; max-width: 100%; overflow: auto;">
                        <pre><code class="language-{}" id="code-content"></code></pre>
                    </div>
                    <br>
                    <p><a href="/download/{}" class="download-btn">Download {} File</a></p>
                    <script>
                        fetch('/raw/{}')
                            .then(response => response.text())
                            .then(data => {{
                                document.getElementById('code-content').textContent = data;
                                Prism.highlightAll();
                            }});
                    </script>
                </div>"#,
                lang, file_info.id, extension.to_uppercase(), file_info.id
            )
        },
        // Java files
        "java" => {
            format!(
                r#"<div class="code-viewer">
                    <div style="text-align: left; max-width: 100%; overflow: auto;">
                        <pre><code class="language-java" id="code-content"></code></pre>
                    </div>
                    <br>
                    <p><a href="/download/{}" class="download-btn">Download Java File</a></p>
                    <script>
                        fetch('/raw/{}')
                            .then(response => response.text())
                            .then(data => {{
                                document.getElementById('code-content').textContent = data;
                                Prism.highlightAll();
                            }});
                    </script>
                </div>"#,
                file_info.id, file_info.id
            )
        },
        // Go files
        "go" => {
            format!(
                r#"<div class="code-viewer">
                    <div style="text-align: left; max-width: 100%; overflow: auto;">
                        <pre><code class="language-go" id="code-content"></code></pre>
                    </div>
                    <br>
                    <p><a href="/download/{}" class="download-btn">Download Go File</a></p>
                    <script>
                        fetch('/raw/{}')
                            .then(response => response.text())
                            .then(data => {{
                                document.getElementById('code-content').textContent = data;
                                Prism.highlightAll();
                            }});
                    </script>
                </div>"#,
                file_info.id, file_info.id
            )
        },
        // PHP files
        "php" => {
            format!(
                r#"<div class="code-viewer">
                    <div style="text-align: left; max-width: 100%; overflow: auto;">
                        <pre><code class="language-php" id="code-content"></code></pre>
                    </div>
                    <br>
                    <p><a href="/download/{}" class="download-btn">Download PHP File</a></p>
                    <script>
                        fetch('/raw/{}')
                            .then(response => response.text())
                            .then(data => {{
                                document.getElementById('code-content').textContent = data;
                                Prism.highlightAll();
                            }});
                    </script>
                </div>"#,
                file_info.id, file_info.id
            )
        },
        // YAML files
        "yml" | "yaml" => {
            format!(
                r#"<div class="code-viewer">
                    <div style="text-align: left; max-width: 100%; overflow: auto;">
                        <pre><code class="language-yaml" id="code-content"></code></pre>
                    </div>
                    <br>
                    <p><a href="/download/{}" class="download-btn">Download YAML File</a></p>
                    <script>
                        fetch('/raw/{}')
                            .then(response => response.text())
                            .then(data => {{
                                document.getElementById('code-content').textContent = data;
                                Prism.highlightAll();
                            }});
                    </script>
                </div>"#,
                file_info.id, file_info.id
            )
        },
        // TOML files
        "toml" => {
            format!(
                r#"<div class="code-viewer">
                    <div style="text-align: left; max-width: 100%; overflow: auto;">
                        <pre><code class="language-toml" id="code-content"></code></pre>
                    </div>
                    <br>
                    <p><a href="/download/{}" class="download-btn">Download TOML File</a></p>
                    <script>
                        fetch('/raw/{}')
                            .then(response => response.text())
                            .then(data => {{
                                document.getElementById('code-content').textContent = data;
                                Prism.highlightAll();
                            }});
                    </script>
                </div>"#,
                file_info.id, file_info.id
            )
        },
        // Other programming languages with basic highlighting
        "rb" | "swift" | "kt" => {
            let lang_name = match extension.as_str() {
                "rb" => "ruby",
                "swift" => "swift", 
                "kt" => "kotlin",
                _ => "markup"
            };
            format!(
                r#"<div class="code-viewer">
                    <div style="text-align: left; max-width: 100%; overflow: auto;">
                        <pre><code class="language-{}" id="code-content"></code></pre>
                    </div>
                    <br>
                    <p><a href="/download/{}" class="download-btn">Download {} File</a></p>
                    <script>
                        fetch('/raw/{}')
                            .then(response => response.text())
                            .then(data => {{
                                document.getElementById('code-content').textContent = data;
                                Prism.highlightAll();
                            }});
                    </script>
                </div>"#,
                lang_name, file_info.id, extension.to_uppercase(), file_info.id
            )
        },
        // Markdown files - server-side rendered HTML with styling
        "md" => {
            // Check file size first
            let file_path = Path::new(&file_info.path);
            if let Ok(metadata) = std::fs::metadata(file_path) {
                if metadata.len() > MAX_MARKDOWN_SIZE {
                    format!(
                        r#"<div class="file-info">
                            <h3>Markdown File: {}</h3>
                            <p>⚠️ File too large for preview ({:.1} MB)</p>
                            <p>Files larger than {:.1} MB cannot be rendered as markdown.</p>
                            <p><a href="/download/{}" class="download-btn">Download File</a></p>
                            <p><a href="/raw/{}" target="_blank">View Raw Content</a></p>
                        </div>"#,
                        file_info.name, 
                        metadata.len() as f64 / (1024.0 * 1024.0),
                        MAX_MARKDOWN_SIZE as f64 / (1024.0 * 1024.0),
                        file_info.id,
                        file_info.id
                    )
                } else {
                    // Read the markdown file content
                    let md_content = match std::fs::read_to_string(&Path::new(&file_info.path)) {
                        Ok(content) => simple_markdown_to_html(&content),
                        Err(_) => "<p>Error reading markdown file</p>".to_string(),
                    };
                    
                    format!(
                        r#"<div class="markdown-viewer">
                            <div class="markdown-body">
                                {}
                            </div>
                            <br>
                            <p><a href="/download/{}" class="download-btn">Download Markdown</a></p>
                        </div>"#,
                        md_content, file_info.id
                    )
                }
            } else {
                format!(
                    r#"<div class="file-info">
                        <h3>Error reading file: {}</h3>
                        <p><a href="/download/{}" class="download-btn">Download File</a></p>
                    </div>"#,
                    file_info.name, file_info.id
                )
            }
        },
        // Jupyter Notebook files - server-side rendered HTML
        "ipynb" => {
            // Check file size first
            let file_path = Path::new(&file_info.path);
            if let Ok(metadata) = std::fs::metadata(file_path) {
                if metadata.len() > MAX_NOTEBOOK_SIZE {
                    format!(
                        r#"<div class="file-info">
                            <h3>Jupyter Notebook: {}</h3>
                            <p>⚠️ Notebook too large for preview ({:.1} MB)</p>
                            <p>Notebooks larger than {:.1} MB cannot be rendered.</p>
                            <p><a href="/download/{}" class="download-btn">Download Notebook</a></p>
                            <p><a href="/raw/{}" target="_blank">View Raw JSON</a></p>
                        </div>"#,
                        file_info.name, 
                        metadata.len() as f64 / (1024.0 * 1024.0),
                        MAX_NOTEBOOK_SIZE as f64 / (1024.0 * 1024.0),
                        file_info.id,
                        file_info.id
                    )
                } else {
                    // Read and parse the notebook file
                    let notebook_content = match std::fs::read_to_string(&Path::new(&file_info.path)) {
                        Ok(content) => {
                            match serde_json::from_str::<serde_json::Value>(&content) {
                                Ok(notebook) => render_notebook_to_html(&notebook),
                                Err(e) => format!("<p>Error parsing notebook: {}</p><pre>{}</pre>", e, content),
                            }
                        },
                        Err(_) => "<p>Error reading notebook file</p>".to_string(),
                    };
                    
                    format!(
                        r#"<div class="notebook-viewer">
                            <div class="notebook-body">
                                {}
                            </div>
                            <br>
                            <p><a href="/download/{}" class="download-btn">Download Notebook</a></p>
                        </div>"#,
                        notebook_content, file_info.id
                    )
                }
            } else {
                format!(
                    r#"<div class="file-info">
                        <h3>Error reading notebook: {}</h3>
                        <p><a href="/download/{}" class="download-btn">Download File</a></p>
                    </div>"#,
                    file_info.name, file_info.id
                )
            }
        },
        // Other text files
        "txt" | "rst" | "log" | "ini" | "cfg" | "conf" => {
            format!(
                r#"<div class="text-viewer">
                    <iframe src="/raw/{}" style="width: 100%; height: 600px; border: 1px solid #ddd; border-radius: 5px;"></iframe>
                    <br><br>
                    <p><a href="/download/{}" class="download-btn">Download File</a></p>
                </div>"#,
                file_info.id, file_info.id
            )
        },
        // CSV files - display as table
        "csv" => {
            let file_path = Path::new(&file_info.path);
            if let Ok(metadata) = std::fs::metadata(file_path) {
                if metadata.len() > MAX_SPREADSHEET_SIZE {
                    format!(
                        r#"<div class="file-info">
                            <h3>Large CSV File: {}</h3>
                            <p>⚠️ CSV file too large for preview ({:.1} MB)</p>
                            <p>Files over {} MB are not displayed to prevent browser issues.</p>
                            <p><a href="/download/{}" class="download-btn">Download CSV</a></p>
                            <p><a href="/raw/{}" target="_blank">View Raw Content</a></p>
                        </div>"#,
                        file_info.name, 
                        metadata.len() as f64 / (1024.0 * 1024.0),
                        MAX_SPREADSHEET_SIZE / (1024 * 1024),
                        file_info.id,
                        file_info.id
                    )
                } else {
                    match parse_csv_to_html(file_path, MAX_CSV_ROWS) {
                        Ok(table_html) => format!(
                            r#"<div class="spreadsheet-viewer">
                                <h3>📊 CSV File: {}</h3>
                                {}
                                <br>
                                <p><a href="/download/{}" class="download-btn">Download CSV</a></p>
                            </div>"#,
                            file_info.name, table_html, file_info.id
                        ),
                        Err(_) => format!(
                            r#"<div class="file-info">
                                <h3>Error reading CSV file: {}</h3>
                                <p>Unable to parse CSV content. The file may be corrupted or use an unsupported format.</p>
                                <p><a href="/download/{}" class="download-btn">Download CSV</a></p>
                                <p><a href="/raw/{}" target="_blank">View Raw Content</a></p>
                            </div>"#,
                            file_info.name, file_info.id, file_info.id
                        )
                    }
                }
            } else {
                format!(
                    r#"<div class="file-info">
                        <h3>Error reading CSV file: {}</h3>
                        <p><a href="/download/{}" class="download-btn">Download File</a></p>
                    </div>"#,
                    file_info.name, file_info.id
                )
            }
        },
        // Excel files - display as table
        "xlsx" | "xls" => {
            let file_path = Path::new(&file_info.path);
            if let Ok(metadata) = std::fs::metadata(file_path) {
                if metadata.len() > MAX_SPREADSHEET_SIZE {
                    format!(
                        r#"<div class="file-info">
                            <h3>Large Excel File: {}</h3>
                            <p>⚠️ Excel file too large for preview ({:.1} MB)</p>
                            <p>Files over {} MB are not displayed to prevent browser issues.</p>
                            <p><a href="/download/{}" class="download-btn">Download Excel File</a></p>
                        </div>"#,
                        file_info.name, 
                        metadata.len() as f64 / (1024.0 * 1024.0),
                        MAX_SPREADSHEET_SIZE / (1024 * 1024),
                        file_info.id
                    )
                } else {
                    match parse_excel_to_html(file_path, MAX_EXCEL_ROWS) {
                        Ok(table_html) => format!(
                            r#"<div class="spreadsheet-viewer">
                                <h3>📊 Excel File: {}</h3>
                                {}
                                <br>
                                <p><a href="/download/{}" class="download-btn">Download Excel File</a></p>
                            </div>"#,
                            file_info.name, table_html, file_info.id
                        ),
                        Err(_) => format!(
                            r#"<div class="file-info">
                                <h3>Error reading Excel file: {}</h3>
                                <p>Unable to parse Excel content. The file may be corrupted or use an unsupported format.</p>
                                <p><a href="/download/{}" class="download-btn">Download Excel File</a></p>
                            </div>"#,
                            file_info.name, file_info.id
                        )
                    }
                }
            } else {
                format!(
                    r#"<div class="file-info">
                        <h3>Error reading Excel file: {}</h3>
                        <p><a href="/download/{}" class="download-btn">Download File</a></p>
                    </div>"#,
                    file_info.name, file_info.id
                )
            }
        },
        // PDF files
        "pdf" => {
            format!(
                r#"<div class="pdf-viewer">
                    <iframe src="/raw/{}" style="width: 100%; height: 800px; border: 1px solid #ddd; border-radius: 5px;" type="application/pdf">
                        <p>Your browser does not support PDF viewing. <a href="/download/{}">Download PDF</a></p>
                    </iframe>
                    <br>
                    <p><a href="/download/{}" class="download-btn">Download PDF</a></p>
                </div>"#,
                file_info.id, file_info.id, file_info.id
            )
        },
        // Default for other files
        _ => {
            format!(
                r#"<div class="file-info">
                    <h3>File: {}</h3>
                    <p>File type: {}</p>
                    <p>This file type cannot be previewed in the browser.</p>
                    <p><a href="/download/{}" class="download-btn">Download File</a></p>
                </div>"#,
                file_info.name, extension, file_info.id
            )
        }
    };

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <title>{}</title>
    <meta charset="UTF-8">
    <!-- Prism.js CSS for syntax highlighting -->
    <link href="https://cdnjs.cloudflare.com/ajax/libs/prism/1.29.0/themes/prism-dark.min.css" rel="stylesheet" />
    <style>
        body {{ 
            font-family: Arial, sans-serif; 
            margin: 20px; 
            background-color: #1a1a1a; 
            color: #e0e0e0;
        }}
        .container {{ 
            max-width: 1200px; 
            margin: 0 auto; 
            background-color: #2d2d2d; 
            padding: 20px; 
            border-radius: 10px; 
            box-shadow: 0 2px 10px rgba(0,0,0,0.3); 
        }}
        h1 {{ 
            color: #ffffff; 
            text-align: center; 
            margin-bottom: 20px; 
        }}
        .file-info {{ 
            text-align: center; 
            padding: 20px; 
            color: #e0e0e0;
        }}
        .download-btn {{ 
            display: inline-block;
            padding: 10px 20px;
            background-color: #0d7377;
            color: white;
            text-decoration: none;
            border-radius: 5px;
            margin: 10px;
        }}
        .download-btn:hover {{ 
            background-color: #14a085; 
        }}
        .text-viewer {{ 
            text-align: center; 
        }}
        .code-viewer {{
            text-align: center;
        }}
        .code-viewer pre {{
            text-align: left;
            margin: 0;
            border-radius: 8px;
            max-height: 600px;
            overflow: auto;
            background-color: #1e1e1e;
        }}
        .code-viewer code {{
            font-family: 'Monaco', 'Menlo', 'Consolas', 'Courier New', monospace;
            font-size: 14px;
            line-height: 1.5;
            color: #d4d4d4;
        }}
        .json-viewer {{
            text-align: center;
        }}
        .json-viewer pre {{
            text-align: left;
            margin: 0;
            border-radius: 8px;
            max-height: 600px;
            overflow: auto;
            background-color: #1e1e1e;
        }}
        .xml-viewer {{
            text-align: center;
        }}
        .xml-viewer pre {{
            text-align: left;
            margin: 0;
            border-radius: 8px;
            max-height: 600px;
            overflow: auto;
            background-color: #1e1e1e;
        }}
        .audio-viewer {{
            text-align: center;
        }}
        .video-container {{
            text-align: center;
        }}
        .pdf-viewer {{
            text-align: center;
        }}
        video, audio {{ 
            display: block; 
            margin: 20px auto; 
        }}
        img {{ 
            display: block; 
            margin: 20px auto; 
        }}
        iframe {{
            border: 1px solid #444;
            border-radius: 5px;
            background-color: #2d2d2d;
        }}
        /* Table Styling for Spreadsheets */
        .spreadsheet-viewer {{
            text-align: center;
        }}
        .table-container {{
            max-width: 100%;
            overflow: auto;
            margin: 20px 0;
            border-radius: 8px;
            border: 1px solid #444;
        }}
        .data-table {{
            width: 100%;
            border-collapse: collapse;
            background-color: #2d2d2d;
            font-size: 14px;
        }}
        .data-table th {{
            background-color: #0d7377;
            color: #ffffff;
            padding: 12px 8px;
            text-align: left;
            font-weight: bold;
            border-bottom: 2px solid #14a085;
            position: sticky;
            top: 0;
            z-index: 10;
        }}
        .data-table td {{
            padding: 8px;
            border-bottom: 1px solid #444;
            color: #e0e0e0;
            vertical-align: top;
        }}
        .data-table tr:nth-child(even) {{
            background-color: #3a3a3a;
        }}
        .data-table tr:hover {{
            background-color: #404040;
        }}
        .data-table td:empty::after {{
            content: "—";
            color: #8b949e;
            font-style: italic;
        }}
        /* Markdown Styling */
        .markdown-viewer {{
            text-align: center;
        }}
        .markdown-body {{
            text-align: left;
            max-width: 100%;
            margin: 0 auto;
            padding: 20px;
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            line-height: 1.6;
            color: #e0e0e0;
            background-color: #2d2d2d;
            border-radius: 8px;
        }}
        .markdown-body h1, .markdown-body h2, .markdown-body h3, .markdown-body h4, .markdown-body h5, .markdown-body h6 {{
            margin-top: 24px;
            margin-bottom: 16px;
            font-weight: 600;
            line-height: 1.25;
            color: #ffffff;
        }}
        .markdown-body h1 {{ font-size: 2em; border-bottom: 1px solid #58a6ff; padding-bottom: 10px; }}
        .markdown-body h2 {{ font-size: 1.5em; border-bottom: 1px solid #58a6ff; padding-bottom: 8px; }}
        .markdown-body h3 {{ font-size: 1.25em; }}
        .markdown-body h4 {{ font-size: 1em; }}
        .markdown-body h5 {{ font-size: 0.875em; }}
        .markdown-body h6 {{ font-size: 0.85em; color: #8b949e; }}
        .markdown-body p {{ margin-bottom: 16px; }}
        .markdown-body blockquote {{
            padding: 0 1em;
            color: #8b949e;
            border-left: 0.25em solid #58a6ff;
            margin: 0 0 16px 0;
            background-color: #0d1117;
            border-radius: 4px;
        }}
        .markdown-body ul, .markdown-body ol {{
            padding-left: 2em;
            margin-bottom: 16px;
        }}
        .markdown-body li {{ margin-bottom: 0.25em; }}
        .markdown-body code {{
            padding: 0.2em 0.4em;
            margin: 0;
            font-size: 85%;
            background-color: #343a46;
            border-radius: 6px;
            font-family: ui-monospace, SFMono-Regular, 'SF Mono', Consolas, 'Liberation Mono', Menlo, monospace;
            color: #f0f6fc;
        }}
        .markdown-body pre {{
            padding: 16px;
            overflow: auto;
            font-size: 85%;
            line-height: 1.45;
            background-color: #0d1117;
            border-radius: 6px;
            margin-bottom: 16px;
            border: 1px solid #30363d;
        }}
        .markdown-body pre code {{
            background-color: transparent;
            border: 0;
            display: inline;
            line-height: inherit;
            margin: 0;
            overflow: visible;
            padding: 0;
            word-wrap: normal;
            color: #e6edf3;
        }}
        .markdown-body table {{
            border-spacing: 0;
            border-collapse: collapse;
            margin-bottom: 16px;
            width: 100%;
            background-color: #0d1117;
            border: 1px solid #30363d;
            border-radius: 6px;
        }}
        .markdown-body table th, .markdown-body table td {{
            padding: 6px 13px;
            border: 1px solid #30363d;
        }}
        .markdown-body table th {{
            font-weight: 600;
            background-color: #161b22;
            color: #f0f6fc;
        }}
        .markdown-body table tr:nth-child(even) {{
            background-color: #161b22;
        }}
        .markdown-body a {{
            color: #58a6ff;
            text-decoration: none;
        }}
        .markdown-body a:hover {{
            text-decoration: underline;
        }}
        /* Jupyter Notebook Styling */
        .notebook-viewer {{
            text-align: center;
        }}
        .notebook-body {{
            text-align: left;
            max-width: 100%;
            margin: 0 auto;
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
        }}
        .notebook-header {{
            text-align: center;
            margin-bottom: 30px;
            padding: 20px;
            background-color: #161b22;
            border-radius: 8px;
            border: 1px solid #30363d;
        }}
        .notebook-header h2 {{
            margin: 0 0 10px 0;
            color: #f0f6fc;
        }}
        .notebook-cell {{
            margin-bottom: 20px;
            border: 1px solid #30363d;
            border-radius: 8px;
            overflow: hidden;
            background-color: #0d1117;
        }}
        .cell-header {{
            background-color: #161b22;
            padding: 8px 16px;
            border-bottom: 1px solid #30363d;
            display: flex;
            justify-content: space-between;
            align-items: center;
            font-size: 12px;
            font-weight: 600;
        }}
        .cell-type {{
            color: #8b949e;
        }}
        .cell-number {{
            color: #f0f6fc;
        }}
        .cell-content {{
            padding: 16px;
            background-color: #0d1117;
        }}
        .cell-markdown .cell-header {{
            background-color: #0d3a65;
        }}
        .cell-code .cell-header {{
            background-color: #4c1d95;
        }}
        .markdown-cell h1, .markdown-cell h2, .markdown-cell h3, .markdown-cell h4, .markdown-cell h5, .markdown-cell h6 {{
            margin-top: 16px;
            margin-bottom: 12px;
            font-weight: 600;
            line-height: 1.25;
            color: #f0f6fc;
        }}
        .markdown-cell h1 {{ font-size: 1.8em; }}
        .markdown-cell h2 {{ font-size: 1.5em; }}
        .markdown-cell h3 {{ font-size: 1.25em; }}
        .markdown-cell p {{ margin-bottom: 12px; color: #e6edf3; }}
        .markdown-cell code {{
            padding: 0.2em 0.4em;
            background-color: #343a46;
            border-radius: 4px;
            font-family: monospace;
            color: #f0f6fc;
        }}
        .code-cell pre {{
            background-color: #161b22;
            border: 1px solid #30363d;
            border-radius: 4px;
            padding: 12px;
            margin: 0;
            overflow-x: auto;
        }}
        .code-cell code {{
            background-color: transparent;
            font-family: 'SFMono-Regular', Consolas, 'Liberation Mono', Menlo, Courier, monospace;
            font-size: 13px;
            line-height: 1.4;
            color: #e6edf3;
        }}
        .cell-output {{
            margin-top: 16px;
            border-top: 1px solid #30363d;
            padding-top: 12px;
        }}
        .output-header {{
            font-weight: 600;
            color: #8b949e;
            font-size: 12px;
            margin-bottom: 8px;
            text-transform: uppercase;
        }}
        .output-stream {{
            background-color: #161b22;
            border: 1px solid #30363d;
            border-radius: 4px;
            padding: 8px 12px;
            margin: 0;
            font-family: monospace;
            font-size: 12px;
            white-space: pre-wrap;
            color: #e6edf3;
        }}
        .output-text {{
            background-color: #0d1117;
            border: 1px solid #30363d;
            border-radius: 4px;
            padding: 8px 12px;
            margin: 0;
            font-family: monospace;
            font-size: 12px;
            color: #e6edf3;
        }}
        .output-html {{
            border: 1px solid #30363d;
            border-radius: 4px;
            padding: 12px;
            background-color: #0d1117;
        }}
        .output-error {{
            background-color: #86181d;
            border: 1px solid #f85149;
            border-radius: 4px;
            padding: 8px 12px;
            margin: 0;
            font-family: monospace;
            font-size: 12px;
            color: #ffa198;
            white-space: pre-wrap;
        }}
        .raw-cell {{
            background-color: #161b22;
            font-family: monospace;
            font-size: 13px;
        }}
        .raw-cell pre {{
            margin: 0;
            white-space: pre-wrap;
            color: #e6edf3;
        }}
    </style>
</head>
<body>
    <div class="container">
        <h1>{}</h1>
        <div class="file-content">
            {}
        </div>
        <div style="text-align: center; margin-top: 20px;">
            <p style="color: #8b949e;"><strong>File Path:</strong> {}</p>
        </div>
    </div>
    <!-- Prism.js JavaScript for syntax highlighting -->
    <script src="https://cdnjs.cloudflare.com/ajax/libs/prism/1.29.0/components/prism-core.min.js"></script>
    <script src="https://cdnjs.cloudflare.com/ajax/libs/prism/1.29.0/plugins/autoloader/prism-autoloader.min.js"></script>
</body>
</html>"#,
        file_info.name, file_info.name, viewer_content, file_info.path
    )
}

// Simple markdown to HTML converter that works offline
fn simple_markdown_to_html(markdown: &str) -> String {
    let mut html = String::new();
    let lines: Vec<&str> = markdown.lines().collect();
    let mut i = 0;
    let mut in_code_block = false;
    let mut code_lang = String::new();

    while i < lines.len() {
        let line = lines[i].trim_end();
        
        // Handle code blocks
        if line.starts_with("```") {
            if in_code_block {
                html.push_str("</code></pre>\n");
                in_code_block = false;
                code_lang.clear();
            } else {
                in_code_block = true;
                code_lang = line[3..].trim().to_string();
                if code_lang.is_empty() {
                    html.push_str("<pre><code>");
                } else {
                    html.push_str(&format!("<pre><code class=\"language-{}\">", escape_html(&code_lang)));
                }
            }
            i += 1;
            continue;
        }
        
        if in_code_block {
            html.push_str(&escape_html(line));
            html.push('\n');
            i += 1;
            continue;
        }
        
        // Handle headers
        if line.starts_with("# ") {
            html.push_str(&format!("<h1>{}</h1>\n", escape_html(&line[2..])));
        } else if line.starts_with("## ") {
            html.push_str(&format!("<h2>{}</h2>\n", escape_html(&line[3..])));
        } else if line.starts_with("### ") {
            html.push_str(&format!("<h3>{}</h3>\n", escape_html(&line[4..])));
        } else if line.starts_with("#### ") {
            html.push_str(&format!("<h4>{}</h4>\n", escape_html(&line[5..])));
        } else if line.starts_with("##### ") {
            html.push_str(&format!("<h5>{}</h5>\n", escape_html(&line[6..])));
        } else if line.starts_with("###### ") {
            html.push_str(&format!("<h6>{}</h6>\n", escape_html(&line[7..])));
        }
        // Handle blockquotes
        else if line.starts_with("> ") {
            html.push_str(&format!("<blockquote><p>{}</p></blockquote>\n", process_inline_formatting(&line[2..])));
        }
        // Handle unordered lists
        else if line.starts_with("- ") || line.starts_with("* ") {
            html.push_str("<ul>\n");
            while i < lines.len() && (lines[i].trim_start().starts_with("- ") || lines[i].trim_start().starts_with("* ")) {
                let item = lines[i].trim_start();
                let content = if item.starts_with("- ") { &item[2..] } else { &item[2..] };
                html.push_str(&format!("<li>{}</li>\n", process_inline_formatting(content)));
                i += 1;
            }
            html.push_str("</ul>\n");
            continue;
        }
        // Handle ordered lists
        else if line.chars().next().map_or(false, |c| c.is_ascii_digit()) && line.contains(". ") {
            html.push_str("<ol>\n");
            while i < lines.len() && lines[i].chars().next().map_or(false, |c| c.is_ascii_digit()) && lines[i].contains(". ") {
                if let Some(dot_pos) = lines[i].find(". ") {
                    let content = &lines[i][dot_pos + 2..];
                    html.push_str(&format!("<li>{}</li>\n", process_inline_formatting(content)));
                }
                i += 1;
            }
            html.push_str("</ol>\n");
            continue;
        }
        // Handle horizontal rules
        else if line == "---" || line == "***" || line == "___" {
            html.push_str("<hr>\n");
        }
        // Handle empty lines
        else if line.is_empty() {
            // Skip empty lines, they'll be handled by paragraph spacing
        }
        // Handle regular paragraphs
        else {
            html.push_str(&format!("<p>{}</p>\n", process_inline_formatting(line)));
        }
        
        i += 1;
    }
    
    html
}

// Process inline markdown formatting (bold, italic, code, links)
fn process_inline_formatting(text: &str) -> String {
    let mut result = escape_html(text);
    
    // Handle inline code first (to avoid processing markdown inside code)
    result = regex::Regex::new(r"`([^`]+)`").unwrap()
        .replace_all(&result, "<code>$1</code>")
        .to_string();
    
    // Handle bold (**text**)
    result = regex::Regex::new(r"\*\*([^*]+)\*\*").unwrap()
        .replace_all(&result, "<strong>$1</strong>")
        .to_string();
    
    // Handle italic (*text*)
    result = regex::Regex::new(r"\*([^*]+)\*").unwrap()
        .replace_all(&result, "<em>$1</em>")
        .to_string();
    
    // Handle links [text](url)
    result = regex::Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap()
        .replace_all(&result, "<a href=\"$2\">$1</a>")
        .to_string();
    
    result
}

// Render Jupyter notebook to HTML
fn render_notebook_to_html(notebook: &serde_json::Value) -> String {
    let mut html = String::new();
    
    // Notebook header
    html.push_str("<div class=\"notebook-header\">");
    html.push_str("<h2>📓 Jupyter Notebook</h2>");
    
    if let Some(metadata) = notebook.get("metadata") {
        if let Some(kernelspec) = metadata.get("kernelspec") {
            if let Some(display_name) = kernelspec.get("display_name") {
                if let Some(name) = display_name.as_str() {
                    html.push_str(&format!("<p><strong>Kernel:</strong> {}</p>", escape_html(name)));
                }
            }
        }
    }
    html.push_str("</div>");
    
    // Process cells
    if let Some(cells) = notebook.get("cells") {
        if let Some(cells_array) = cells.as_array() {
            for (index, cell) in cells_array.iter().enumerate() {
                let cell_type = cell.get("cell_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                
                html.push_str(&format!("<div class=\"notebook-cell cell-{}\">", cell_type));
                html.push_str("<div class=\"cell-header\">");
                html.push_str(&format!("<span class=\"cell-type\">{}</span>", cell_type.to_uppercase()));
                html.push_str(&format!("<span class=\"cell-number\">[{}]</span>", index + 1));
                html.push_str("</div>");
                
                html.push_str("<div class=\"cell-content\">");
                
                // Get cell source
                let source = cell.get("source")
                    .map(|s| {
                        if let Some(array) = s.as_array() {
                            array.iter()
                                .filter_map(|v| v.as_str())
                                .collect::<Vec<_>>()
                                .join("")
                        } else if let Some(string) = s.as_str() {
                            string.to_string()
                        } else {
                            String::new()
                        }
                    })
                    .unwrap_or_default();
                
                match cell_type {
                    "markdown" => {
                        html.push_str("<div class=\"markdown-cell\">");
                        html.push_str(&simple_markdown_to_html(&source));
                        html.push_str("</div>");
                    },
                    "code" => {
                        html.push_str("<div class=\"code-cell\">");
                        html.push_str("<pre><code class=\"language-python\">");
                        html.push_str(&escape_html(&source));
                        html.push_str("</code></pre>");
                        
                        // Handle outputs
                        if let Some(outputs) = cell.get("outputs") {
                            if let Some(outputs_array) = outputs.as_array() {
                                if !outputs_array.is_empty() {
                                    html.push_str("<div class=\"cell-output\">");
                                    html.push_str("<div class=\"output-header\">Output:</div>");
                                    
                                    for output in outputs_array {
                                        let output_type = output.get("output_type")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("unknown");
                                        
                                        match output_type {
                                            "stream" => {
                                                if let Some(text) = output.get("text") {
                                                    let text_content = if let Some(array) = text.as_array() {
                                                        array.iter()
                                                            .filter_map(|v| v.as_str())
                                                            .collect::<Vec<_>>()
                                                            .join("")
                                                    } else if let Some(string) = text.as_str() {
                                                        string.to_string()
                                                    } else {
                                                        String::new()
                                                    };
                                                    html.push_str("<pre class=\"output-stream\">");
                                                    html.push_str(&escape_html(&text_content));
                                                    html.push_str("</pre>");
                                                }
                                            },
                                            "execute_result" | "display_data" => {
                                                if let Some(data) = output.get("data") {
                                                    if let Some(text_plain) = data.get("text/plain") {
                                                        let text_content = if let Some(array) = text_plain.as_array() {
                                                            array.iter()
                                                                .filter_map(|v| v.as_str())
                                                                .collect::<Vec<_>>()
                                                                .join("")
                                                        } else if let Some(string) = text_plain.as_str() {
                                                            string.to_string()
                                                        } else {
                                                            String::new()
                                                        };
                                                        html.push_str("<pre class=\"output-text\">");
                                                        html.push_str(&escape_html(&text_content));
                                                        html.push_str("</pre>");
                                                    }
                                                }
                                            },
                                            "error" => {
                                                if let Some(traceback) = output.get("traceback") {
                                                    let traceback_content = if let Some(array) = traceback.as_array() {
                                                        array.iter()
                                                            .filter_map(|v| v.as_str())
                                                            .collect::<Vec<_>>()
                                                            .join("\n")
                                                    } else if let Some(string) = traceback.as_str() {
                                                        string.to_string()
                                                    } else {
                                                        String::new()
                                                    };
                                                    html.push_str("<pre class=\"output-error\">");
                                                    html.push_str(&escape_html(&traceback_content));
                                                    html.push_str("</pre>");
                                                }
                                            },
                                            _ => {}
                                        }
                                    }
                                    
                                    html.push_str("</div>");
                                }
                            }
                        }
                        html.push_str("</div>");
                    },
                    _ => {
                        html.push_str("<div class=\"raw-cell\">");
                        html.push_str("<pre>");
                        html.push_str(&escape_html(&source));
                        html.push_str("</pre>");
                        html.push_str("</div>");
                    }
                }
                
                html.push_str("</div>");
                html.push_str("</div>");
            }
        }
    }
    
    html
}

// Helper function to escape HTML characters
fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

fn parse_range(range_header: &str, file_size: u64) -> Option<(u64, u64)> {
    // Parse Range header like "bytes=0-1023" or "bytes=1024-"
    if !range_header.starts_with("bytes=") {
        return None;
    }
    
    let range_part = &range_header[6..]; // Remove "bytes="
    let parts: Vec<&str> = range_part.split('-').collect();
    
    if parts.len() != 2 {
        return None;
    }
    
    let start = if parts[0].is_empty() {
        // Range like "bytes=-1024" (last 1024 bytes)
        if let Ok(suffix_length) = parts[1].parse::<u64>() {
            if suffix_length >= file_size {
                0
            } else {
                file_size - suffix_length
            }
        } else {
            return None;
        }
    } else if let Ok(start_pos) = parts[0].parse::<u64>() {
        start_pos
    } else {
        return None;
    };
    
    let end = if parts[1].is_empty() {
        // Range like "bytes=1024-" (from 1024 to end)
        file_size - 1
    } else if let Ok(end_pos) = parts[1].parse::<u64>() {
        std::cmp::min(end_pos, file_size - 1)
    } else {
        return None;
    };
    
    if start <= end && start < file_size {
        Some((start, end))
    } else {
        None
    }
}
