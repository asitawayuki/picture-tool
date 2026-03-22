use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub is_image: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageEntry {
    pub name: String,
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub size_bytes: u64,
    pub thumbnail_base64: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressPayload {
    pub current: usize,
    pub total: usize,
    pub file_name: String,
}
