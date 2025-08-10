use crate::file_system::FileInfo;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use ignore::WalkBuilder;
use rayon::prelude::*;
use regex::Regex;
use std::path::Path;
use std::time::Duration;
use tokio::task;
use tokio::time::timeout;

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub file_info: FileInfo,
    pub score: i64,
    pub match_type: MatchType,
}

#[derive(Debug, Clone)]
pub enum MatchType {
    FileName,
    FilePath,
}

pub struct SearchEngine {
    fuzzy_matcher: SkimMatcherV2,
}

impl SearchEngine {
    pub fn new() -> Self {
        SearchEngine {
            fuzzy_matcher: SkimMatcherV2::default(),
        }
    }

    pub async fn search(
        &self,
        root_path: &Path,
        pattern: &str,
    ) -> Result<Vec<SearchResult>, Box<dyn std::error::Error + Send + Sync>> {
        // Add timeout protection for search operations
        let search_future = self.search_internal(root_path, pattern);
        match timeout(Duration::from_secs(30), search_future).await {
            Ok(result) => result,
            Err(_) => Err("Search timed out after 30 seconds. Try a more specific search term or search from a smaller directory.".into()),
        }
    }

    async fn search_internal(
        &self,
        root_path: &Path,
        pattern: &str,
    ) -> Result<Vec<SearchResult>, Box<dyn std::error::Error + Send + Sync>> {
        let pattern = pattern.to_string();
        let root_path = root_path.to_path_buf();

        // Validate search path
        if !root_path.exists() {
            return Err(format!("Search path does not exist: {}", root_path.display()).into());
        }

        if !root_path.is_dir() {
            return Err(format!("Search path is not a directory: {}", root_path.display()).into());
        }

        task::spawn_blocking(move || {
            let fuzzy_matcher = SkimMatcherV2::default();
            let regex = Regex::new(&pattern).ok();
            let pattern_lower = pattern.to_lowercase();
            
            // Use ignore crate to respect .gitignore files with more conservative settings
            let walker = WalkBuilder::new(&root_path)
                .hidden(false)
                .ignore(true)
                .git_ignore(true)
                .max_depth(Some(8)) // Reduced depth for better performance
                .max_filesize(Some(100 * 1024 * 1024)) // Skip files larger than 100MB
                .build();

            // Stream processing with parallel search
            let results: Vec<SearchResult> = walker
                .par_bridge()
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| {
                    let path = entry.path();
                    
                    // Quick filename extraction without full FileInfo creation
                    let filename = path.file_name()?.to_str()?;
                    let filename_lower = filename.to_lowercase();
                    let path_str = path.to_string_lossy();
                    let path_str_lower = path_str.to_lowercase();
                    
                    // Fast pre-filtering: skip if no chance of match
                    let has_substring = filename_lower.contains(&pattern_lower) || 
                                      path_str_lower.contains(&pattern_lower);
                    
                    let has_regex_match = regex.as_ref()
                        .map(|r| r.is_match(&path_str))
                        .unwrap_or(false);
                    
                    if !has_substring && !has_regex_match {
                        // Quick fuzzy check on filename only
                        if fuzzy_matcher.fuzzy_match(filename, &pattern).is_none() {
                            return None; // Skip this file entirely
                        }
                    }
                    
                    // Only create FileInfo for potential matches
                    let file_info = FileInfo::from_path(path).ok()?;
                    
                    // Detailed scoring
                    if let Some(score) = fuzzy_matcher.fuzzy_match(&file_info.name, &pattern) {
                        return Some(SearchResult {
                            file_info,
                            score,
                            match_type: MatchType::FileName,
                        });
                    }
                    
                    // Regex match on full path
                    if let Some(ref regex) = regex {
                        if regex.is_match(&path_str) {
                            return Some(SearchResult {
                                file_info,
                                score: 50,
                                match_type: MatchType::FilePath,
                            });
                        }
                    }
                    
                    // Substring match on path
                    if path_str_lower.contains(&pattern_lower) {
                        // Higher score for filename matches vs path matches
                        let score = if filename_lower.contains(&pattern_lower) { 40 } else { 30 };
                        return Some(SearchResult {
                            file_info,
                            score,
                            match_type: MatchType::FilePath,
                        });
                    }
                    
                    None
                })
                .collect();

            // Sort by score (descending) and limit results
            let mut sorted_results = results;
            sorted_results.sort_by(|a, b| b.score.cmp(&a.score));
            sorted_results.truncate(1000); // Limit to top 1000 results
            
            Ok(sorted_results)
        }).await?
    }

    pub fn search_in_files(
        &self,
        files: &[FileInfo],
        pattern: &str,
    ) -> Vec<SearchResult> {
        let pattern_lower = pattern.to_lowercase();
        
        // Parallel search in provided files
        let results: Vec<SearchResult> = files
            .par_iter()
            .filter_map(|file_info| {
                let filename_lower = file_info.name.to_lowercase();
                
                // Quick substring check first
                if !filename_lower.contains(&pattern_lower) {
                    if let Some(score) = self.fuzzy_matcher.fuzzy_match(&file_info.name, pattern) {
                        return Some(SearchResult {
                            file_info: file_info.clone(),
                            score,
                            match_type: MatchType::FileName,
                        });
                    }
                    return None;
                }
                
                // Fuzzy match for substring matches to get better scoring
                let score = self.fuzzy_matcher
                    .fuzzy_match(&file_info.name, pattern)
                    .unwrap_or(25); // Default score for substring matches
                
                Some(SearchResult {
                    file_info: file_info.clone(),
                    score,
                    match_type: MatchType::FileName,
                })
            })
            .collect();

        // Sort by score (descending)
        let mut sorted_results = results;
        sorted_results.sort_by(|a, b| b.score.cmp(&a.score));
        sorted_results
    }

    /// Fast search optimized for interactive use (limits results and depth)
    pub async fn search_fast(
        &self,
        root_path: &Path,
        pattern: &str,
        max_results: usize,
    ) -> Result<Vec<SearchResult>, Box<dyn std::error::Error + Send + Sync>> {
        // Add timeout protection for fast search operations  
        let search_future = self.search_fast_internal(root_path, pattern, max_results);
        match timeout(Duration::from_secs(10), search_future).await {
            Ok(result) => result,
            Err(_) => Err("Fast search timed out after 10 seconds. Try a more specific search term.".into()),
        }
    }

    async fn search_fast_internal(
        &self,
        root_path: &Path,
        pattern: &str,
        max_results: usize,
    ) -> Result<Vec<SearchResult>, Box<dyn std::error::Error + Send + Sync>> {
        let pattern = pattern.to_string();
        let root_path = root_path.to_path_buf();

        // Validate search path
        if !root_path.exists() {
            return Err(format!("Search path does not exist: {}", root_path.display()).into());
        }

        if !root_path.is_dir() {
            return Err(format!("Search path is not a directory: {}", root_path.display()).into());
        }

        task::spawn_blocking(move || {
            let fuzzy_matcher = SkimMatcherV2::default();
            let pattern_lower = pattern.to_lowercase();
            
            let walker = WalkBuilder::new(&root_path)
                .hidden(false)
                .ignore(true)
                .git_ignore(true)
                .max_depth(Some(4)) // Very shallow search for speed
                .max_filesize(Some(50 * 1024 * 1024)) // Skip files larger than 50MB
                .build();

            let results: Vec<SearchResult> = walker
                .par_bridge()
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| {
                    let path = entry.path();
                    let filename = path.file_name()?.to_str()?;
                    let filename_lower = filename.to_lowercase();
                    
                    // Only process files that might match
                    if filename_lower.contains(&pattern_lower) {
                        let file_info = FileInfo::from_path(path).ok()?;
                        let score = fuzzy_matcher
                            .fuzzy_match(&file_info.name, &pattern)
                            .unwrap_or(25);
                        
                        Some(SearchResult {
                            file_info,
                            score,
                            match_type: MatchType::FileName,
                        })
                    } else {
                        // Try fuzzy match for non-substring matches
                        if let Some(score) = fuzzy_matcher.fuzzy_match(filename, &pattern) {
                            let file_info = FileInfo::from_path(path).ok()?;
                            Some(SearchResult {
                                file_info,
                                score,
                                match_type: MatchType::FileName,
                            })
                        } else {
                            None
                        }
                    }
                })
                .collect();

            let mut sorted_results = results;
            sorted_results.sort_by(|a, b| b.score.cmp(&a.score));
            sorted_results.truncate(max_results); // Limit results after collection
            
            Ok(sorted_results)
        }).await?
    }
}
