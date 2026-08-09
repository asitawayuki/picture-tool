use picture_tool_core as core;
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressPayload {
    pub current: usize,
    pub total: usize,
    pub file_name: String,
}

/// 変換できなかったファイルと理由
#[derive(Debug, Clone, Serialize)]
pub struct ProcessFailure {
    pub input_path: String,
    pub error: String,
}

/// `process_images` の戻り値
///
/// 以前は成功分だけを返し、失敗は件数だけを `processing-error` イベントで
/// 流していた。フロントエンドはそのイベントを購読しておらず、失敗の理由は
/// どこにも出ていなかった（死に配線）。イベントは購読漏れが起きうるので、
/// 戻り値に載せて必ず届くようにした（S6-M15）。
#[derive(Debug, Clone, Serialize)]
pub struct ProcessBatchResponse {
    pub results: Vec<core::ProcessResult>,
    pub failures: Vec<ProcessFailure>,
    /// バッチ全体に関わる警告（アセット読み込みの不備、削除のキャンセルなど）
    pub warnings: Vec<String>,
}

/// Exif フレームのプレビュー
#[derive(Debug, Clone, Serialize)]
pub struct PreviewImage {
    /// `<img src>` にそのまま渡せる data URI
    pub data_url: String,
    /// アセット読み込み時の警告（カスタム model_map の不備など）
    pub warnings: Vec<String>,
}
