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
  thumbnail_base64: string | null;
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
}
