mod commands;
mod state;
mod types;

use state::ProcessingState;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .manage(ProcessingState::new())
        .invoke_handler(tauri::generate_handler![
            commands::list_directory,
            commands::list_drives,
            commands::list_images,
            commands::get_thumbnail,
            commands::get_full_image,
            commands::process_images,
            commands::cancel_processing,
            commands::get_exif_info,
            commands::render_exif_frame_preview,
            commands::list_presets,
            commands::save_preset,
            commands::delete_preset,
            commands::list_available_fonts,
            commands::list_available_logos,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
