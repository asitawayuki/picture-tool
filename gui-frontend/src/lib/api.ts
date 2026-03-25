import { invoke } from "@tauri-apps/api/core";
import type {
  FileEntry,
  ImageEntry,
  ProcessingConfig,
  ProcessResult,
  ExifInfo,
  ExifFrameConfig,
  FontInfo,
  LogoInfo,
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

export async function processImages(
  files: string[],
  outputFolder: string,
  config: ProcessingConfig,
  exifFrameConfig?: ExifFrameConfig | null
): Promise<ProcessResult[]> {
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
  config: ExifFrameConfig
): Promise<string> {
  return invoke("render_exif_frame_preview", { path, config });
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

export async function listAvailableLogos(): Promise<LogoInfo[]> {
  return invoke("list_available_logos");
}
