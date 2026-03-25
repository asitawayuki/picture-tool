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

// Exif Frame types
export type FrameLayout = "bottom_bar" | "side_bar" | "full_border";

export type FrameColor =
  | "white"
  | "black"
  | { custom: [number, number, number] };

export type OutputAspectRatio = { fixed: [number, number] } | "free";

export interface DisplayItems {
  maker_logo: boolean;
  brand_logo: boolean;
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
  layout: FrameLayout;
  color: FrameColor;
  aspect_ratio: OutputAspectRatio;
  items: DisplayItems;
  font: FontConfig;
  custom_text: string;
  frame_padding: number;
}

export interface FontInfo {
  display_name: string;
  path: string | null;
  is_bundled: boolean;
}

export interface LogoInfo {
  filename: string;
  matched_to: string | null;
  is_bundled: boolean;
}
