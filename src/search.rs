use crate::file_system::FileInfo;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use ignore::WalkBuilder;
use rayon::prelude::*;
use regex::Regex;
use std::path::Path;
use tokio::task;

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
        let pattern = pattern.to_string();
        let root_path = root_path.to_path_buf();

        task::spawn_blocking(move || {
            let fuzzy_matcher = SkimMatcherV2::default();
            let regex = Regex::new(&pattern).ok();
            let pattern_lower = pattern.to_lowercase();
            
            // Use ignore crate to respect .gitignore files
            let walker = WalkBuilder::new(&root_path)
                .hidden(false)
                .ignore(true)
                .git_ignore(true)
                .max_depth(Some(10)) // Limit depth for performance
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
        let pattern = pattern.to_string();
        let root_path = root_path.to_path_buf();

        task::spawn_blocking(move || {
            let fuzzy_matcher = SkimMatcherV2::default();
            let pattern_lower = pattern.to_lowercase();
            
            let walker = WalkBuilder::new(&root_path)
                .hidden(false)
                .ignore(true)
                .git_ignore(true)
                .max_depth(Some(5)) // Shallow search for speed
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
