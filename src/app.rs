use std::time::Duration;

pub struct App {
    pub input: String,
    pub total_duration: Duration,
    pub current_duration: Duration,
    pub progress: f64,
    pub current_file_index: usize,
    pub should_quit: bool,
    pub available_files: Vec<String>,
    pub metadata: Vec<FileMetadata>,
}

pub struct FileMetadata {
    pub file_name: String,
    pub tags: Vec<String>,
}