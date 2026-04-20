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
    pub state: AppState,
    pub waveform_data: Vec<u64>,
    pub interaction_mode: InteractionMode,
    pub active_trim_side: TrimSide,
    pub trim_warning: Option<String>,
    pub is_paused: bool,
}

#[derive(Clone)]
pub struct FileMetadata {
    pub tags: Vec<String>,
    pub location: Option<String>,
    pub trim_from: Option<Duration>,
    pub trim_to: Option<Duration>,
}

pub enum AppState {
    AskingForTags,
    AskingForLocation,
    Processing,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum InteractionMode {
    Normal,
    Trim,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TrimSide {
    From,
    To,
}