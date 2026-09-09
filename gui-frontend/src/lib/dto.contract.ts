// このファイルは自動生成される。**手で編集しない。**
//
// 生成元は `gui/src/types.rs` の `mod contract`。Rust が serde で実際に吐く
// JSON を、TypeScript のリテラルとして書き出したものである。手書きの
// `types.ts` に代入しているので、Rust 側と TS 側が食い違うと
// `bun run typecheck` が落ちる（境界の形を突き合わせる唯一の場所）。
//
// 再生成:
//   UPDATE_DTO_CONTRACT=1 cargo test -p picture-tool-gui contract
//
// 生成し直したら、`types.ts` の側も同じ変更を入れること。
//
// 載せる範囲は DTO の形だけではない。**画面に出ない綴り**（イベント名
// `os-color-scheme-changed` と、その "light" / "dark" ペイロード）も
// ここに置いてある。ずれても画面は普通に描画されるので、他に気づく
// 手立てが無い。
//
// `e2e/stub.ts` の応答も**この契約から形を取る**。スタブに形を書き写すと
// 写しが 3 つ目に増え、スタブだけが古い形のまま緑を出し続ける。
//
// 検知できないもの:
//   - TS 側の型が Rust より広い場合（代入は通る。安全側なので許容）
//   - `invoke` の引数名（`maxDimension` → `max_dimension`）。未対応

import type {
  ColorSchemePayload,
  EditableMetadata,
  ExifFrameConfig,
  ExifInfo,
  ExifPosition,
  FileEntry,
  FontInfo,
  ImageEntry,
  MetadataRead,
  OsColorSchemeChangedEvent,
  PreviewImage,
  ProcessBatchResponse,
  ProcessFailure,
  ProcessResult,
  ProcessingConfig,
  ProgressPayload,
} from "./types";

/** フォルダーツリーの 1 行 */
export const fileEntry: FileEntry = {
  is_dir: true,
  is_image: false,
  name: "photos",
  path: "/home/user/photos",
};

/** 写真グリッドの 1 枚 */
export const imageEntry: ImageEntry = {
  created_ms: null,
  height: 3000,
  modified_ms: 1755000000000,
  name: "photo.jpg",
  path: "/home/user/photos/photo.jpg",
  rating: 3,
  size_bytes: 4500000,
  width: 4000,
};

/** 出力幅の上限あり（4 の倍数で送る） */
export const processingConfigWithMaxWidth: ProcessingConfig = {
  bg_color: "white",
  delete_originals: false,
  max_size_mb: 8,
  max_width: 1000,
  mode: "crop",
  quality: 90,
};

/** 出力幅の上限なし（max_width は null で送る） */
export const processingConfigUnlimited: ProcessingConfig = {
  bg_color: "black",
  delete_originals: true,
  max_size_mb: 1,
  max_width: null,
  mode: "pad",
  quality: 75,
};

/** 品質を下げて収めた 1 枚（警告つき） */
export const processResultCompressed: ProcessResult = {
  final_quality: 72,
  final_size_mb: 1.5,
  input_path: "/in/photo.jpg",
  output_path: "/out/photo.jpg",
  size_limit_exceeded: false,
  warnings: ["EXIF を読めませんでした"],
};

/** 下限品質でも上限に収まらなかった 1 枚（final_quality は null になりうる） */
export const processResultOverLimit: ProcessResult = {
  final_quality: null,
  final_size_mb: 12.0,
  input_path: "/in/huge.jpg",
  output_path: "/out/huge.jpg",
  size_limit_exceeded: true,
  warnings: [],
};

/** 変換できなかった 1 件 */
export const processFailure: ProcessFailure = {
  error: "Failed to open image",
  input_path: "/in/broken.jpg",
};

/** process_images の戻り値 */
export const processBatchResponse: ProcessBatchResponse = {
  failures: [
    {
      error: "Failed to open image",
      input_path: "/in/broken.jpg",
    },
  ],
  results: [
    {
      final_quality: 90,
      final_size_mb: 1.5,
      input_path: "/in/photo.jpg",
      output_path: "/out/photo.jpg",
      size_limit_exceeded: false,
      warnings: [],
    },
  ],
  warnings: ["元ファイルの削除は中止されました"],
};

/** 失敗も警告も無い場合（空配列で来る。フィールドごと欠けることはない） */
export const processBatchResponseAllSucceeded: ProcessBatchResponse = {
  failures: [],
  results: [],
  warnings: [],
};

/** Exif フレームのプレビュー */
export const previewImage: PreviewImage = {
  data_url: "data:image/jpeg;base64,/9j/4AAQ",
  warnings: ["model_map_custom.json を読めませんでした"],
};

/** processing-progress イベントの中身 */
export const progressPayload: ProgressPayload = {
  current: 3,
  file_name: "photo.jpg",
  total: 40,
};

/** 全項目が読めた場合 */
export const exifInfoFilled: ExifInfo = {
  camera_make: "SONY",
  camera_model: "ILCE-7M4",
  date_taken: "2026-08-19 10:00:00",
  f_number: "f/1.4",
  focal_length: "35mm",
  iso: 400,
  lens_model: "FE 35mm F1.4 GM",
  orientation: 1,
  shutter_speed: "1/250s",
};

/** Exif が無い場合。全項目が null で来る（キーごと消えるのではない） */
export const exifInfoEmpty: ExifInfo = {
  camera_make: null,
  camera_model: null,
  date_taken: null,
  f_number: null,
  focal_length: null,
  iso: null,
  lens_model: null,
  orientation: null,
  shutter_speed: null,
};

/** 利用者が足したフォントを指すプリセット */
export const exifFrameConfigWithUserFont: ExifFrameConfig = {
  custom_text: "Shot on film",
  font: {
    font_path: "/home/user/.config/picture-tool/assets/fonts/x.ttf",
    primary_size: 0.5,
    secondary_size: 0.25,
  },
  items: {
    camera_model: true,
    custom_text: false,
    date_taken: false,
    f_number: true,
    focal_length: true,
    iso: true,
    lens_brand_logo: true,
    lens_model: true,
    maker_logo: true,
    shutter_speed: true,
  },
  name: "my-preset",
  position: "bottom",
};

/** 同梱フォントを使うプリセット（font_path は null） */
export const exifFrameConfigWithBundledFont: ExifFrameConfig = {
  custom_text: "",
  font: {
    font_path: null,
    primary_size: 0.25,
    secondary_size: 0.125,
  },
  items: {
    camera_model: true,
    custom_text: false,
    date_taken: false,
    f_number: true,
    focal_length: true,
    iso: true,
    lens_brand_logo: true,
    lens_model: true,
    maker_logo: true,
    shutter_speed: true,
  },
  name: "default",
  position: "auto",
};

/** assets/fonts/ に置かれたフォント */
export const fontInfoUser: FontInfo = {
  display_name: "User: MyFont",
  is_bundled: false,
  path: "/home/user/.config/picture-tool/assets/fonts/MyFont.ttf",
};

/** 同梱フォント（path は null） */
export const fontInfoBundled: FontInfo = {
  display_name: "Noto Sans JP (bundled)",
  is_bundled: true,
  path: null,
};

/** 3 項目とも入っている */
export const editableMetadataFilled: EditableMetadata = {
  comment: "順光。もう少し寄れた",
  rating: 4,
  title: "夕暮れの港",
};

/** タグが 1 つも無い（null と空文字は別物） */
export const editableMetadataEmpty: EditableMetadata = {
  comment: null,
  rating: null,
  title: null,
};

/** 他のアプリが書いた別の値を検出した場合 */
export const metadataRead: MetadataRead = {
  metadata: {
    comment: null,
    rating: 4,
    title: "夕暮れの港",
  },
  warnings: ["コメントに、他のアプリが書いた別の値があります（XMP dc:description を採用。ほかに Exif XPComment = 「古い注」）。保存すると揃います"],
};

/** 変換モードの綴り（serde の rename_all を固定する） */
export const conversionModes: ProcessingConfig["mode"][] = ["crop", "pad", "quality"];

/** 背景色の綴り */
export const backgroundColors: ProcessingConfig["bg_color"][] = ["white", "black"];

/** Exif フレームの配置位置の綴り */
export const exifPositions: ExifPosition[] = ["auto", "bottom", "top", "right", "left"];

/** OS のテーマ変更を流すイベント名（gui/src/theme.rs） */
export const osColorSchemeChangedEvent: OsColorSchemeChangedEvent = "os-color-scheme-changed";

/** そのイベントのペイロードと set_window_theme が受け付ける綴り */
export const colorSchemes: ColorSchemePayload[] = ["light", "dark"];
