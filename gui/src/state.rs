use crate::security::WritableRoots;
use lru::LruCache;
use picture_tool_core::ExifFramePreviewSource;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// 写真の同一性。更新日時が取れない環境ではサイズだけで見分ける
pub type PreviewSourceKey = (PathBuf, Option<SystemTime>, u64);

/// 直近にプレビューした 1 枚の下地（`render_exif_frame_preview`）。
/// フレームの設定を触るたびに写真をデコードし直さないためのもの。
/// `spawn_blocking` へ持ち出すので `Arc` で包む
pub type PreviewSourceCache = Arc<Mutex<Option<(PreviewSourceKey, Arc<ExifFramePreviewSource>)>>>;

pub struct ProcessingState {
    pub cancel_flag: Arc<AtomicBool>,
    pub thumbnail_cache: Mutex<LruCache<String, String>>,
    pub preview_source: PreviewSourceCache,
    /// **変換の出力先**として、ネイティブダイアログで選ばれたフォルダー
    /// （`pick_output_folder`）。起動時は空＝どこにも書けない状態から始まる。
    pub writable_roots: WritableRoots,
    /// **メタデータ編集**として、ネイティブダイアログで承認されたフォルダー
    /// （`grant_metadata_editing`）。
    ///
    /// `writable_roots` と**分けてある**。出力先の選択ダイアログは
    /// 「出力先フォルダーを選択」としか言っておらず、既存写真の書き換えについては
    /// 一言も触れていない。集合を共有すると、出力先に自分の写真フォルダーを選んだ
    /// だけで、そのフォルダー配下の全 JPEG のメタデータが追加の同意なしに
    /// 書き換え可能になってしまう（それぞれのダイアログが承認したことだけを承認する）。
    pub metadata_roots: WritableRoots,
    /// メタデータの下書きに未保存の変更があるか。
    /// フロントが `set_unsaved_state` で更新し、終了時の確認だけがこれを読む
    pub unsaved: AtomicBool,
    /// 終了確認のダイアログを表示中か。
    /// **表示中にもう一度 `CloseRequested` が来うる**ので抑止に使う（刷新 spec §3-4）
    pub close_prompt_open: AtomicBool,
}

impl ProcessingState {
    pub fn new() -> Self {
        Self {
            cancel_flag: Arc::new(AtomicBool::new(false)),
            thumbnail_cache: Mutex::new(LruCache::new(NonZeroUsize::new(500).unwrap())),
            preview_source: PreviewSourceCache::default(),
            writable_roots: WritableRoots::default(),
            metadata_roots: WritableRoots::default(),
            unsaved: AtomicBool::new(false),
            close_prompt_open: AtomicBool::new(false),
        }
    }
}

impl Default for ProcessingState {
    fn default() -> Self {
        Self::new()
    }
}
