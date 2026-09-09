// Rust の DTO（`gui/src/types.rs`）を**手で写したもの**。webview は JSON しか
// 見ないので、片側だけ改名してもコンパイラは何も言わない。
//
// その食い違いを検知するのが `dto.contract.ts`（自動生成物）。Rust 側を変えると
// `cargo test` が落ち、契約を再生成すると今度はこのファイルを直すまで
// `bun run typecheck` が落ちる。手順は `dto.contract.ts` の冒頭を読むこと。

export interface FileEntry {
  name: string;
  path: string;
  is_dir: boolean;
  is_image: boolean;
}

export interface ImageEntry {
  name: string;
  path: string;
  width: number;
  height: number;
  size_bytes: number;
  /** 更新日時（UNIX epoch からのミリ秒）。null は取れなかった */
  modified_ms: number | null;
  /**
   * 作成日時（UNIX epoch からのミリ秒）。null は取れなかった。
   * **Linux では取れないファイルシステムがある**（並び順では末尾へ送る）
   */
  created_ms: number | null;
  /**
   * ★（1〜5）。null は未評価、または読めなかった。
   *
   * **`list_images` が唯一の出所。** 絞り込みにはフォルダー全件が揃っている
   * 必要があり、可視範囲を起点にする形では原理的に足りない
   */
  rating: number | null;
}

export interface RatingEntry {
  path: string;
  /** 1〜5。null は未評価、または読めなかった */
  rating: number | null;
}

export interface ProcessingConfig {
  mode: "crop" | "pad" | "quality";
  bg_color: "white" | "black";
  quality: number;
  max_size_mb: number;
  delete_originals: boolean;
  /** 出力4:5キャンバスの幅の上限 (px)。null は無制限。常に4の倍数を送る */
  max_width: number | null;
}

export interface ProcessResult {
  input_path: string;
  output_path: string;
  final_size_mb: number;
  final_quality: number | null;
  /** 品質を下限まで下げても max_size_mb を満たせなかった */
  size_limit_exceeded: boolean;
  /** 処理は成功したが利用者に伝えるべき事象（core は stderr へ出力しない） */
  warnings: string[];
}

/** 変換できなかったファイルと理由 */
export interface ProcessFailure {
  input_path: string;
  error: string;
}

export interface ProcessBatchResponse {
  results: ProcessResult[];
  failures: ProcessFailure[];
  /** バッチ全体に関わる警告（アセット読み込みの不備、削除のキャンセルなど） */
  warnings: string[];
}

/** Exif フレームのプレビュー */
export interface PreviewImage {
  /** <img src> にそのまま渡せる data URI */
  data_url: string;
  warnings: string[];
}

export interface ProgressPayload {
  current: number;
  total: number;
  file_name: string;
}

export interface ExifInfo {
  camera_make: string | null;
  camera_model: string | null;
  lens_model: string | null;
  focal_length: string | null;
  f_number: string | null;
  shutter_speed: string | null;
  iso: number | null;
  date_taken: string | null;
  /** EXIF Orientation (1-8)。null はタグ無し */
  orientation: number | null;
}

// Exif Frame types
export type ExifPosition = "auto" | "bottom" | "top" | "right" | "left";

export interface DisplayItems {
  maker_logo: boolean;
  lens_brand_logo: boolean;
  camera_model: boolean;
  lens_model: boolean;
  focal_length: boolean;
  f_number: boolean;
  shutter_speed: boolean;
  iso: boolean;
  date_taken: boolean;
  custom_text: boolean;
}

export interface FontConfig {
  font_path: string | null;
  primary_size: number;
  secondary_size: number;
}

export interface ExifFrameConfig {
  name: string;
  position: ExifPosition;
  items: DisplayItems;
  font: FontConfig;
  custom_text: string;
}

export interface FontInfo {
  display_name: string;
  path: string | null;
  is_bundled: boolean;
}

/**
 * 人が編集するメタデータ。
 *
 * `null` は「タグが無い」、`""` は「空文字のタグがある」を意味し、Rust 側は**区別する**。
 * 空欄で保存したら `null` を送る（＝タグを削除する）。
 */
export interface EditableMetadata {
  title: string | null;
  comment: string | null;
  /** 1〜5。null は未評価。**0 は存在しない**（RatingPercent に表現できないため） */
  rating: number | null;
}

/** 読み取り結果。warnings には箇所ごとの値の食い違いが載る */
export interface MetadataRead {
  metadata: EditableMetadata;
  warnings: string[];
}

/**
 * OS のテーマ変更を流すイベント名。値は Rust の `theme.rs` が持つ定数と同じ綴り。
 *
 * 型として書いておくと、Rust 側だけ変えたときに `dto.contract.ts` の再生成で
 * ここが食い違い、`bun run typecheck` が落ちる。**イベント名は画面に出ないので、
 * ずれても誰も気づかないまま OS 追従だけが死ぬ** ── それを防ぐための線である。
 */
export type OsColorSchemeChangedEvent = "os-color-scheme-changed";

/** そのイベントのペイロードと、`set_window_theme` が受け付ける綴り */
export type ColorSchemePayload = "light" | "dark";
