mod commands;
mod security;
mod state;
mod theme;
mod types;

use state::ProcessingState;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Manager;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use theme::OsColorScheme;

/// `close_prompt_open` を必ず降ろす RAII ガード（Task 9 の `TempFile` と同じ
/// 「panic 経路も塞ぐ」型）。`blocking_show()` を含む一連の処理が途中で panic しても、
/// スタック巻き戻しの過程で `Drop` が走ってフラグを戻す。**戻していないと、
/// 以後の `CloseRequested` は毎回「表示中」扱いで早期 return し、
/// ウィンドウを UI から二度と閉じられなくなる。**
///
/// **降ろすのは他のフラグ操作より必ず後。** `Drop` はスコープの終わりで走るので、
/// このガードは他の処理より先に（かつ `state` より後に）宣言すること。
struct PromptGuard<'a>(&'a AtomicBool);

impl Drop for PromptGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .manage(ProcessingState::new())
        .manage(OsColorScheme::new())
        // OS のテーマは Linux では自前で追う必要がある（theme.rs 冒頭の実測メモ）
        .setup(|app| {
            theme::init(app.handle());
            Ok(())
        })
        // 終了だけは webview に任せない。不可逆な操作は OS ネイティブで確認する
        // （元ファイル削除と同じ扱い）。刷新 spec §3-4
        .on_window_event(|window, event| {
            // Linux ではこのイベントが発火しない（tao に実装が無い）。
            // 追従は theme.rs が portal から直接読む
            #[cfg(not(target_os = "linux"))]
            if let tauri::WindowEvent::ThemeChanged(t) = event {
                theme::on_theme_changed(window.app_handle(), *t);
                return;
            }

            let tauri::WindowEvent::CloseRequested { api, .. } = event else {
                return;
            };
            let state = window.state::<ProcessingState>();
            if !state.unsaved.load(Ordering::SeqCst) {
                return; // そのまま閉じてよい
            }

            // 未保存があるときは**常に**止めてから確認する
            api.prevent_close();

            // ダイアログ表示中に再度 CloseRequested が来ても二重に出さない
            if state.close_prompt_open.swap(true, Ordering::SeqCst) {
                return;
            }

            // **メインスレッドで blocking_show() を呼ばない。**
            // イベントループを塞いでダイアログが出ないまま固まる
            let handle = window.app_handle().clone();
            std::thread::spawn(move || {
                let state = handle.state::<ProcessingState>();
                // panic しても close_prompt_open を必ず降ろす。ガードは
                // **他の処理より先に**（`state` の取得より後に）宣言する ──
                // Drop は宣言と逆順（＝ここで宣言すれば最後）に走るので、
                // これで「unsaved を先に落としてから close_prompt_open を降ろす」
                // 順序が、途中の return や panic があっても崩れない
                let _prompt_guard = PromptGuard(&state.close_prompt_open);

                let mut builder = handle
                    .dialog()
                    .message("保存していない変更があります。\n閉じると失われます。")
                    .title("保存せずに終了しますか？")
                    .kind(MessageDialogKind::Warning)
                    .buttons(MessageDialogButtons::OkCancelCustom(
                        "保存せずに終了".to_string(),
                        "戻る".to_string(),
                    ));
                let main = handle.get_webview_window("main");
                if let Some(ref w) = main {
                    builder = builder.parent(w);
                }
                let approved = builder.blocking_show();

                if approved {
                    // **close_prompt_open より先に unsaved を落とす。** 逆順だと、
                    // この2命令の間に来た CloseRequested が
                    // 「unsaved=true / close_prompt_open=false」を見て
                    // 二重にダイアログスレッドを立ててしまう
                    state.unsaved.store(false, Ordering::SeqCst);
                    if let Some(w) = main {
                        let _ = w.destroy();
                    }
                }
                // _prompt_guard はここで drop され、close_prompt_open を最後に false へ戻す
            });
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_directory,
            commands::list_drives,
            commands::list_images,
            commands::get_thumbnail,
            commands::get_full_image,
            commands::pick_output_folder,
            commands::process_images,
            commands::cancel_processing,
            commands::load_favorites,
            commands::save_favorites,
            commands::get_exif_info,
            commands::render_exif_frame_preview,
            commands::list_presets,
            commands::save_preset,
            commands::delete_preset,
            commands::list_available_fonts,
            commands::read_image_metadata,
            commands::write_image_metadata,
            commands::grant_metadata_editing,
            commands::set_unsaved_state,
            theme::os_color_scheme,
            theme::set_window_theme,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
