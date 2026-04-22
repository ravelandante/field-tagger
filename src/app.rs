use std::time::{Duration, Instant};

pub struct App {
    pub location_input: String,
    pub tags_input: String,
    pub total_duration: Duration,
    pub current_duration: Duration,
    pub progress: f64,
    pub current_file_index: usize,
    pub should_quit: bool,
    pub available_files: Vec<String>,
    pub metadata: Vec<FileMetadata>,
    pub state: AppState,
    pub active_input_field: InputField,
    pub waveform_data: Vec<u64>,
    pub interaction_mode: InteractionMode,
    pub active_trim_side: TrimSide,
    pub trim_warning: Option<String>,
    pub is_paused: bool,
    pub seek_repeat_count: u32,
    pub last_seek_direction: i8,
    pub last_seek_at: Option<Instant>,
    pub auto_compute_filename: bool,
    pub keep_original_files: bool,
    pub review_selected_option: ReviewOption,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum InputField {
    Location,
    Tags,
}

#[derive(Clone)]
pub struct FileMetadata {
    pub tags: Vec<String>,
    pub location: Option<String>,
    pub trim_from: Option<Duration>,
    pub trim_to: Option<Duration>,
    pub marked_for_deletion: bool,
}

pub enum AppState {
    EditingMetadata,
    ReviewOutputNaming,
    Processing,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ReviewOption {
    AutoComputeFilename,
    KeepOriginalFiles,
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