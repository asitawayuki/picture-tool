mod commands;
mod state;
mod types;

use state::ProcessingState;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(ProcessingState::new())
        .invoke_handler(tauri::generate_handler![
            commands::list_directory,
            commands::list_drives,
            commands::list_images,
            commands::get_thumbnail,
            commands::process_images,
            commands::cancel_processing,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
