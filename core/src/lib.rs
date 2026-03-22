use serde::{Serialize, Deserialize};
use std::path::{Path, PathBuf};
use anyhow::Result;

// --- 型定義 ---

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ConversionMode {
    Crop,
    Pad,
    Quality,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum BackgroundColor {
    White,
    Black,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingConfig {
    pub mode: ConversionMode,
    pub bg_color: BackgroundColor,
    pub quality: u8,
    pub max_size_mb: usize,
    pub delete_originals: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessResult {
    pub input_path: String,
    pub output_path: String,
    pub final_size_mb: f64,
    pub final_quality: Option<u8>,
}

/// 進捗コールバック: (current, total) -> bool（falseでキャンセル）
pub type ProgressCallback = Box<dyn Fn(usize, usize) -> bool + Send + Sync>;

// --- 公開API（スタブ） ---

pub fn validate_config(config: &ProcessingConfig) -> Result<()> {
    todo!()
}

pub fn is_supported_image(path: &Path) -> bool {
    todo!()
}

pub fn collect_image_files(dir: &Path) -> Result<Vec<PathBuf>> {
    todo!()
}

pub fn process_image(
    input_path: &Path,
    output_folder: &Path,
    config: &ProcessingConfig,
) -> Result<ProcessResult> {
    todo!()
}

pub fn process_batch(
    files: &[PathBuf],
    output_folder: &Path,
    config: &ProcessingConfig,
    on_progress: Option<ProgressCallback>,
) -> Vec<Result<ProcessResult>> {
    todo!()
}

pub fn generate_thumbnail_base64(
    path: &Path,
    max_dimension: u32,
) -> Result<String> {
    todo!()
}
