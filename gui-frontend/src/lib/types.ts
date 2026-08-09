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
}

export interface ProcessingConfig {
  mode: "crop" | "pad" | "quality";
  bg_color: "white" | "black";
  quality: number;
  max_size_mb: number;
  delete_originals: boolean;
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
