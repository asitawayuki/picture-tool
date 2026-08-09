import { invoke } from "@tauri-apps/api/core";
import type {
  FileEntry,
  ImageEntry,
  ProcessingConfig,
  ProcessBatchResponse,
  PreviewImage,
  ExifInfo,
  ExifFrameConfig,
  FontInfo,
} from "./types";

export async function listDirectory(path: string): Promise<FileEntry[]> {
  return invoke("list_directory", { path });
}

export async function listDrives(): Promise<string[]> {
  return invoke("list_drives");
}

export async function listImages(path: string): Promise<ImageEntry[]> {
  return invoke("list_images", { path });
}

export async function getThumbnail(path: string, maxDimension: number): Promise<string> {
  return invoke("get_thumbnail", { path, maxDimension });
}

/**
 * 出力先フォルダーを選ぶ。
 *
 * ダイアログは Rust 側が開く。`@tauri-apps/plugin-dialog` をフロントから
 * 呼ばないのは、選択結果が webview を経由すると「利用者がここを選んだ」を
 * 偽装できてしまい、書き込み許可の根拠にならないため（S6-H8）。
 * 戻り値は選択されたパス。キャンセル時は null。
 */
export async function pickOutputFolder(defaultPath?: string): Promise<string | null> {
  return invoke("pick_output_folder", { defaultPath: defaultPath ?? null });
}

/**
 * お気に入りフォルダーの読み書き。
 *
 * `@tauri-apps/plugin-store` を使わないのは、その JS API が**保存先のパスを
 * webview から受け取る**ため。パスは正規化されず、`load("../../../x.json")` の
 * 形でアプリのデータディレクトリの外へ出られる。保存先を Rust 側に固定した
 * これらのコマンドだけを開けている（S6-H8）。
 */
export async function loadFavorites(): Promise<string[]> {
  return invoke("load_favorites");
}

export async function saveFavorites(favorites: string[]): Promise<void> {
  return invoke("save_favorites", { favorites });
}

export async function processImages(
  files: string[],
  outputFolder: string,
  config: ProcessingConfig,
  exifFrameConfig?: ExifFrameConfig | null
): Promise<ProcessBatchResponse> {
  return invoke("process_images", {
    files,
    outputFolder,
    config,
    exifFrameConfig: exifFrameConfig ?? null,
  });
}

export async function cancelProcessing(): Promise<void> {
  return invoke("cancel_processing");
}

export async function getFullImage(
  path: string,
  maxWidth: number,
  maxHeight: number
): Promise<string> {
  return invoke("get_full_image", { path, maxWidth, maxHeight });
}

export async function getExifInfo(path: string): Promise<ExifInfo> {
  return invoke("get_exif_info", { path });
}

export async function renderExifFramePreview(
  path: string,
  config: ExifFrameConfig,
  bgColor: "white" | "black",
): Promise<PreviewImage> {
  return invoke("render_exif_frame_preview", { path, config, bgColor });
}

export async function listPresets(): Promise<ExifFrameConfig[]> {
  return invoke("list_presets");
}

export async function savePreset(config: ExifFrameConfig): Promise<void> {
  return invoke("save_preset", { config });
}

export async function deletePreset(name: string): Promise<void> {
  return invoke("delete_preset", { name });
}

export async function listAvailableFonts(): Promise<FontInfo[]> {
  return invoke("list_available_fonts");
}
