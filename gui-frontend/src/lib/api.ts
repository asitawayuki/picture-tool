import { invoke } from "@tauri-apps/api/core";
import type {
  FileEntry,
  ImageEntry,
  ProcessingConfig,
  ProcessResult,
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

export async function getThumbnail(path: string): Promise<string> {
  return invoke("get_thumbnail", { path });
}

export async function processImages(
  files: string[],
  outputFolder: string,
  config: ProcessingConfig
): Promise<ProcessResult[]> {
  return invoke("process_images", {
    files,
    outputFolder,
    config,
  });
}

export async function cancelProcessing(): Promise<void> {
  return invoke("cancel_processing");
}
