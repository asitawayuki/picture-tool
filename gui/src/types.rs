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

/// 写真グリッドに★を出すための最小の応答。
/// `read_image_metadata` を 1 枚ずつ呼ぶと可視範囲だけで数十回の IPC になる
#[derive(Debug, Clone, Serialize)]
pub struct RatingEntry {
    pub path: String,
    pub rating: Option<u8>,
}

/// Exif フレームのプレビュー
#[derive(Debug, Clone, Serialize)]
pub struct PreviewImage {
    /// `<img src>` にそのまま渡せる data URI
    pub data_url: String,
    /// アセット読み込み時の警告（カスタム model_map の不備など）
    pub warnings: Vec<String>,
}

/// Rust ⇄ TypeScript の境界の形を固定する。
///
/// `gui-frontend/src/lib/types.ts` は、ここと core の DTO を**手で写したもの**である。
/// 写し間違いも、片側だけの改名も、コンパイラは何も言わない ── webview は JSON しか
/// 見ないので、`size_bytes` を `size` に変えても `make check` も e2e も全部通り、
/// 実機で初めて壊れる。
///
/// そこで、Rust が実際に serde で吐く形を TypeScript のリテラルとして書き出し、
/// `gui-frontend/src/lib/dto.contract.ts` にコミットしておく。
///
/// - Rust 側を変えると、このテストが「生成物と食い違う」で落ちる
/// - 生成し直すと、今度は TS 側の型注釈（手書きの `types.ts`）に合わなくなり
///   `bun run typecheck` が落ちる
///
/// **2 つを別々に書いているからこそ検査になる**（片方から他方を導くと同語反復になる）。
///
/// 検知できないもの: TS 側の型が Rust より**広い**場合（`string` に対して
/// `string | null` と書いてある等）と、`invoke` の引数名（`maxDimension` →
/// `max_dimension`）。前者は安全側、後者は未対応。
#[cfg(test)]
mod contract {
    use super::*;
    use picture_tool_core::exif_frame::{
        DisplayItems, ExifFrameConfig, ExifPosition, FontConfig, FontInfo,
    };
    use picture_tool_core::metadata::{EditableMetadata, MetadataRead};
    use picture_tool_core::{BackgroundColor, ConversionMode, ExifInfo, ProcessingConfig};
    use serde::Serialize;
    use serde_json::Value;

    /// 生成物の置き場。TS 側から `./dto.contract` として import される
    const GENERATED: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../gui-frontend/src/lib/dto.contract.ts"
    );

    /// 書き出す 1 件。`const <name>: <ts_type> = <value>;` になる
    struct Entry {
        name: &'static str,
        ts_type: &'static str,
        /// 何を代表させている値なのかを TS 側にも残す
        note: &'static str,
        value: Value,
    }

    fn entry<T: Serialize>(
        name: &'static str,
        ts_type: &'static str,
        note: &'static str,
        value: T,
    ) -> Entry {
        Entry {
            name,
            ts_type,
            note,
            value: serde_json::to_value(value).expect("DTO は必ず JSON になる"),
        }
    }

    /// serde が enum をどう綴るか（`rename_all` の効き目）を固定する
    fn variants<T: Serialize>(values: &[T]) -> Value {
        Value::Array(
            values
                .iter()
                .map(|v| serde_json::to_value(v).unwrap())
                .collect(),
        )
    }

    fn indent(level: usize) -> String {
        "  ".repeat(level)
    }

    /// JSON を TypeScript のリテラルとして書く。
    /// キーは snake_case の識別子しか来ないので引用符は付けない
    fn ts_literal(value: &Value, level: usize) -> String {
        match value {
            Value::Null => "null".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Number(n) => n.to_string(),
            Value::String(s) => serde_json::to_string(s).unwrap(),
            Value::Array(items) => {
                if items.is_empty() {
                    return "[]".to_string();
                }
                let inline = items.iter().all(|i| !matches!(i, Value::Object(_)));
                if inline {
                    let body: Vec<String> = items.iter().map(|i| ts_literal(i, level)).collect();
                    format!("[{}]", body.join(", "))
                } else {
                    let body: Vec<String> = items
                        .iter()
                        .map(|i| format!("{}{}", indent(level + 1), ts_literal(i, level + 1)))
                        .collect();
                    format!("[\n{},\n{}]", body.join(",\n"), indent(level))
                }
            }
            Value::Object(map) => {
                if map.is_empty() {
                    return "{}".to_string();
                }
                let body: Vec<String> = map
                    .iter()
                    .map(|(k, v)| {
                        format!("{}{}: {}", indent(level + 1), k, ts_literal(v, level + 1))
                    })
                    .collect();
                format!("{{\n{},\n{}}}", body.join(",\n"), indent(level))
            }
        }
    }

    fn render(entries: &[Entry], imports: &[&str]) -> String {
        let mut out = String::new();
        out.push_str(
            "// このファイルは自動生成される。**手で編集しない。**\n\
             //\n\
             // 生成元は `gui/src/types.rs` の `mod contract`。Rust が serde で実際に吐く\n\
             // JSON を、TypeScript のリテラルとして書き出したものである。手書きの\n\
             // `types.ts` に代入しているので、Rust 側と TS 側が食い違うと\n\
             // `bun run typecheck` が落ちる（境界の形を突き合わせる唯一の場所）。\n\
             //\n\
             // 再生成:\n\
             //   UPDATE_DTO_CONTRACT=1 cargo test -p picture-tool-gui contract\n\
             //\n\
             // 生成し直したら、`types.ts` の側も同じ変更を入れること。\n\n",
        );
        out.push_str("import type {\n");
        for name in imports {
            out.push_str(&format!("  {},\n", name));
        }
        out.push_str("} from \"./types\";\n");
        for e in entries {
            out.push_str(&format!(
                "\n/** {} */\nexport const {}: {} = {};\n",
                e.note,
                e.name,
                e.ts_type,
                ts_literal(&e.value, 0)
            ));
        }
        out
    }

    fn contract() -> String {
        let entries = vec![
            entry(
                "fileEntry",
                "FileEntry",
                "フォルダーツリーの 1 行",
                FileEntry {
                    name: "photos".to_string(),
                    path: "/home/user/photos".to_string(),
                    is_dir: true,
                    is_image: false,
                },
            ),
            entry(
                "imageEntry",
                "ImageEntry",
                "写真グリッドの 1 枚",
                ImageEntry {
                    name: "photo.jpg".to_string(),
                    path: "/home/user/photos/photo.jpg".to_string(),
                    width: 4000,
                    height: 3000,
                    size_bytes: 4_500_000,
                },
            ),
            entry(
                "ratingEntry",
                "RatingEntry",
                "グリッドの★1 件（読めなかった写真は rating: null）",
                RatingEntry {
                    path: "/home/user/photos/photo.jpg".to_string(),
                    rating: Some(3),
                },
            ),
            entry(
                "ratingEntryUnrated",
                "RatingEntry",
                "未評価（rating: None）の直列化を固定する。読めなかった場合と綴りが同じになる",
                RatingEntry {
                    path: "/home/user/photos/unrated.jpg".to_string(),
                    rating: None,
                },
            ),
            entry(
                "processingConfigWithMaxWidth",
                "ProcessingConfig",
                "出力幅の上限あり（4 の倍数で送る）",
                ProcessingConfig {
                    mode: ConversionMode::Crop,
                    bg_color: BackgroundColor::White,
                    quality: 90,
                    max_size_mb: 8,
                    delete_originals: false,
                    max_width: Some(1000),
                },
            ),
            entry(
                "processingConfigUnlimited",
                "ProcessingConfig",
                "出力幅の上限なし（max_width は null で送る）",
                ProcessingConfig {
                    mode: ConversionMode::Pad,
                    bg_color: BackgroundColor::Black,
                    quality: 75,
                    max_size_mb: 1,
                    delete_originals: true,
                    max_width: None,
                },
            ),
            entry(
                "processResultCompressed",
                "ProcessResult",
                "品質を下げて収めた 1 枚（警告つき）",
                picture_tool_core::ProcessResult {
                    input_path: "/in/photo.jpg".to_string(),
                    output_path: "/out/photo.jpg".to_string(),
                    final_size_mb: 1.5,
                    final_quality: Some(72),
                    size_limit_exceeded: false,
                    warnings: vec!["EXIF を読めませんでした".to_string()],
                },
            ),
            entry(
                "processResultOverLimit",
                "ProcessResult",
                "下限品質でも上限に収まらなかった 1 枚（final_quality は null になりうる）",
                picture_tool_core::ProcessResult {
                    input_path: "/in/huge.jpg".to_string(),
                    output_path: "/out/huge.jpg".to_string(),
                    final_size_mb: 12.0,
                    final_quality: None,
                    size_limit_exceeded: true,
                    warnings: Vec::new(),
                },
            ),
            entry(
                "processFailure",
                "ProcessFailure",
                "変換できなかった 1 件",
                ProcessFailure {
                    input_path: "/in/broken.jpg".to_string(),
                    error: "Failed to open image".to_string(),
                },
            ),
            entry(
                "processBatchResponse",
                "ProcessBatchResponse",
                "process_images の戻り値",
                ProcessBatchResponse {
                    results: vec![picture_tool_core::ProcessResult {
                        input_path: "/in/photo.jpg".to_string(),
                        output_path: "/out/photo.jpg".to_string(),
                        final_size_mb: 1.5,
                        final_quality: Some(90),
                        size_limit_exceeded: false,
                        warnings: Vec::new(),
                    }],
                    failures: vec![ProcessFailure {
                        input_path: "/in/broken.jpg".to_string(),
                        error: "Failed to open image".to_string(),
                    }],
                    warnings: vec!["元ファイルの削除は中止されました".to_string()],
                },
            ),
            entry(
                "processBatchResponseAllSucceeded",
                "ProcessBatchResponse",
                "失敗も警告も無い場合（空配列で来る。フィールドごと欠けることはない）",
                ProcessBatchResponse {
                    results: Vec::new(),
                    failures: Vec::new(),
                    warnings: Vec::new(),
                },
            ),
            entry(
                "previewImage",
                "PreviewImage",
                "Exif フレームのプレビュー",
                PreviewImage {
                    data_url: "data:image/jpeg;base64,/9j/4AAQ".to_string(),
                    warnings: vec!["model_map_custom.json を読めませんでした".to_string()],
                },
            ),
            entry(
                "progressPayload",
                "ProgressPayload",
                "processing-progress イベントの中身",
                ProgressPayload {
                    current: 3,
                    total: 40,
                    file_name: "photo.jpg".to_string(),
                },
            ),
            entry(
                "exifInfoFilled",
                "ExifInfo",
                "全項目が読めた場合",
                ExifInfo {
                    camera_make: Some("SONY".to_string()),
                    camera_model: Some("ILCE-7M4".to_string()),
                    lens_model: Some("FE 35mm F1.4 GM".to_string()),
                    focal_length: Some("35mm".to_string()),
                    f_number: Some("f/1.4".to_string()),
                    shutter_speed: Some("1/250s".to_string()),
                    iso: Some(400),
                    date_taken: Some("2026-08-19 10:00:00".to_string()),
                    orientation: Some(1),
                },
            ),
            entry(
                "exifInfoEmpty",
                "ExifInfo",
                "Exif が無い場合。全項目が null で来る（キーごと消えるのではない）",
                ExifInfo::default(),
            ),
            entry(
                "exifFrameConfigWithUserFont",
                "ExifFrameConfig",
                "利用者が足したフォントを指すプリセット",
                ExifFrameConfig {
                    name: "my-preset".to_string(),
                    position: ExifPosition::Bottom,
                    items: DisplayItems::default(),
                    font: FontConfig {
                        font_path: Some(
                            "/home/user/.config/picture-tool/assets/fonts/x.ttf".to_string(),
                        ),
                        primary_size: 0.5,
                        secondary_size: 0.25,
                    },
                    custom_text: "Shot on film".to_string(),
                },
            ),
            entry(
                "exifFrameConfigWithBundledFont",
                "ExifFrameConfig",
                "同梱フォントを使うプリセット（font_path は null）",
                ExifFrameConfig {
                    // 寸法は 2 進で丸め誤差の出ない値を使う。`FontConfig::default()` の
                    // 0.025 は f32 → f64 で 0.02500000037252903 に化け、
                    // 生成物に意味の無い差分を残す（形の検査には値は関係ない）
                    font: FontConfig {
                        font_path: None,
                        primary_size: 0.25,
                        secondary_size: 0.125,
                    },
                    ..ExifFrameConfig::default()
                },
            ),
            entry(
                "fontInfoUser",
                "FontInfo",
                "assets/fonts/ に置かれたフォント",
                FontInfo {
                    display_name: "User: MyFont".to_string(),
                    path: Some(
                        "/home/user/.config/picture-tool/assets/fonts/MyFont.ttf".to_string(),
                    ),
                    is_bundled: false,
                },
            ),
            entry(
                "fontInfoBundled",
                "FontInfo",
                "同梱フォント（path は null）",
                FontInfo {
                    display_name: "Noto Sans JP (bundled)".to_string(),
                    path: None,
                    is_bundled: true,
                },
            ),
            entry(
                "editableMetadataFilled",
                "EditableMetadata",
                "3 項目とも入っている",
                EditableMetadata {
                    title: Some("夕暮れの港".to_string()),
                    comment: Some("順光。もう少し寄れた".to_string()),
                    rating: Some(4),
                },
            ),
            entry(
                "editableMetadataEmpty",
                "EditableMetadata",
                "タグが 1 つも無い（null と空文字は別物）",
                EditableMetadata::default(),
            ),
            entry(
                "metadataRead",
                "MetadataRead",
                "他のアプリが書いた別の値を検出した場合",
                MetadataRead {
                    metadata: EditableMetadata {
                        title: Some("夕暮れの港".to_string()),
                        comment: None,
                        rating: Some(4),
                    },
                    warnings: vec![
                        "コメントに、他のアプリが書いた別の値があります（XMP dc:description を採用。ほかに Exif XPComment = 「古い注」）。保存すると揃います".to_string(),
                    ],
                },
            ),
            // enum の綴り。`#[serde(rename_all = ...)]` を外すと "Crop" になり、
            // TS の union に無いので typecheck が落ちる
            Entry {
                name: "conversionModes",
                ts_type: "ProcessingConfig[\"mode\"][]",
                note: "変換モードの綴り（serde の rename_all を固定する）",
                value: variants(&[
                    ConversionMode::Crop,
                    ConversionMode::Pad,
                    ConversionMode::Quality,
                ]),
            },
            Entry {
                name: "backgroundColors",
                ts_type: "ProcessingConfig[\"bg_color\"][]",
                note: "背景色の綴り",
                value: variants(&[BackgroundColor::White, BackgroundColor::Black]),
            },
            Entry {
                name: "exifPositions",
                ts_type: "ExifPosition[]",
                note: "Exif フレームの配置位置の綴り",
                value: variants(&[
                    ExifPosition::Auto,
                    ExifPosition::Bottom,
                    ExifPosition::Top,
                    ExifPosition::Right,
                    ExifPosition::Left,
                ]),
            },
            entry(
                "osColorSchemeChangedEvent",
                "OsColorSchemeChangedEvent",
                "OS のテーマ変更を流すイベント名（gui/src/theme.rs）",
                crate::theme::OS_SCHEME_CHANGED_EVENT,
            ),
            entry(
                "colorSchemes",
                "ColorSchemePayload[]",
                "そのイベントのペイロードと set_window_theme が受け付ける綴り",
                vec![crate::theme::LIGHT, crate::theme::DARK],
            ),
        ];

        render(
            &entries,
            &[
                "ColorSchemePayload",
                "EditableMetadata",
                "ExifFrameConfig",
                "ExifInfo",
                "ExifPosition",
                "FileEntry",
                "FontInfo",
                "ImageEntry",
                "MetadataRead",
                "OsColorSchemeChangedEvent",
                "PreviewImage",
                "ProcessBatchResponse",
                "ProcessFailure",
                "ProcessResult",
                "ProcessingConfig",
                "ProgressPayload",
                "RatingEntry",
            ],
        )
    }

    #[test]
    fn the_typescript_contract_matches_what_rust_serializes() {
        let generated = contract();

        if std::env::var_os("UPDATE_DTO_CONTRACT").is_some() {
            std::fs::write(GENERATED, &generated).expect("生成物を書けない");
            return;
        }

        let committed = std::fs::read_to_string(GENERATED).unwrap_or_default();
        assert_eq!(
            committed, generated,
            "\n境界の DTO が dto.contract.ts と食い違っている。\n\
             Rust 側を変えたなら:\n  \
             UPDATE_DTO_CONTRACT=1 cargo test -p picture-tool-gui contract\n\
             で生成し直し、gui-frontend/src/lib/types.ts にも同じ変更を入れること\n\
             （入れ忘れると bun run typecheck が落ちる）。\n"
        );
    }
}
