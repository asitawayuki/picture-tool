//! webview から渡されたパスの検証。
//!
//! **なぜコマンド側で検証が要るのか**: Tauri v2 の capabilities/ACL は
//! プラグインコマンドのみを対象としており、`generate_handler!` で登録した
//! 自作コマンドには一切適用されない。つまり自作コマンド自身が唯一の境界であり、
//! webview が乗っ取られた場合に `~/.ssh/id_rsa` の読み出しや任意パスへの
//! 書き込み・削除を止めるものが他に無い（S6-H8）。
//!
//! 本アプリはフォルダーツリーでファイルシステム全体を閲覧する設計なので、
//! 「ユーザーが許可した1ルートだけ」に閉じることはできない。代わりに層を分ける:
//!
//! | 操作 | 境界 |
//! |---|---|
//! | ディレクトリ一覧（名前のみ） | 実在するディレクトリであること |
//! | ファイル内容の読み出し | 実体パスが対応画像の拡張子を持つこと |
//! | フォントの読み出し | 実体がユーザーフォントディレクトリ配下の ttf/otf であること |
//! | 書き込み | ネイティブダイアログで許可したルート配下 |
//! | 元ファイルの削除 | 実行ごとに OS ネイティブの確認ダイアログで承認されること |
//!
//! いずれも `canonicalize` した**実体**に対して判定する。シンボリックリンクや
//! `..` を解決してから拡張子を見るので、`photos/x.jpg -> /etc/passwd` は弾かれる。
//!
//! 削除だけルール系が違うのは、入力フォルダーはツリーから自由に選ぶ設計であり
//! 「許可ルート配下」に縛ると通常利用で削除が一切できなくなるため。代わりに
//! webview が偽装できない OS のダイアログを毎回挟み、対象の枚数と場所を提示する
//! （`commands::confirm_delete_originals`）。
//!
//! 判定と実際の I/O の間にリンクを差し替えられる TOCTOU までは防げない。
//! これはローカル権限を持つ攻撃者を想定したもので、本モジュールの脅威モデル
//! （乗っ取られた webview）の外側にある。

use picture_tool_core as core;
use picture_tool_core::exif_frame::AssetDirs;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// プリセット名の上限。ファイル名として扱うので、どの FS でも収まる長さに切る。
const MAX_PRESET_NAME_CHARS: usize = 64;

/// ユーザーがネイティブダイアログで明示的に選んだ書き込み先。
///
/// **webview からは追加できない。** 追加できるのは Rust 側でダイアログを開く
/// `pick_output_folder` コマンドだけで、登録されるのはダイアログの戻り値そのもの。
/// 「許可を与えるコマンド」を webview から呼べる形にすると、乗っ取られた
/// webview が自分に `/` を許可できてしまい境界の意味が無くなる。
#[derive(Default)]
pub struct WritableRoots {
    roots: Mutex<Vec<PathBuf>>,
}

impl WritableRoots {
    /// ネイティブダイアログの戻り値を許可ルートとして登録する
    pub fn grant(&self, root: PathBuf) {
        let mut roots = self.roots.lock().unwrap_or_else(|e| e.into_inner());
        if !roots.contains(&root) {
            roots.push(root);
        }
    }

    fn allows(&self, path: &Path) -> bool {
        let roots = self.roots.lock().unwrap_or_else(|e| e.into_inner());
        roots.iter().any(|root| path.starts_with(root))
    }
}

/// 実在するディレクトリとして検証し、実体のパスを返す
pub fn existing_dir(raw: &str) -> Result<PathBuf, String> {
    let resolved = canonicalize(raw)?;
    if !resolved.is_dir() {
        return Err(format!("Not a directory: {}", raw));
    }
    Ok(resolved)
}

/// 読み出してよい画像ファイルとして検証し、実体のパスを返す
///
/// 拡張子は**リンク解決後の実体**で判定する。これにより、対応画像以外の
/// ファイル（鍵・設定・認証情報など）はコマンド経由で一切読み出せない。
pub fn readable_image(raw: &str) -> Result<PathBuf, String> {
    let resolved = canonicalize(raw)?;
    if !resolved.is_file() {
        return Err(format!("Not a file: {}", raw));
    }
    if !core::is_supported_image(&resolved) {
        return Err(format!("Not a supported image file: {}", raw));
    }
    Ok(resolved)
}

/// 描画に使ってよいフォントとして検証し、実体のパスを返す
///
/// 正当な値の全体は `list_available_fonts` が返す集合、すなわち
/// ユーザーフォントディレクトリ配下の ttf/otf だけ（バンドルフォントは
/// `font_path: None` で表す）。ここを塞がないと、`ExifFrameConfig` は webview が
/// 丸ごと組み立てる構造体なので `font_path: "/dev/zero"` が `fs::read` まで届き、
/// メモリを食い潰せる。エラー文面の差分で任意パスの存在も列挙できる（S6-H8）。
pub fn readable_font(raw: &str) -> Result<PathBuf, String> {
    readable_font_in(AssetDirs::default().user_fonts_dir.as_deref(), raw)
}

fn readable_font_in(fonts_dir: Option<&Path>, raw: &str) -> Result<PathBuf, String> {
    let root = fonts_dir
        .and_then(|dir| fs_canonicalize(dir).ok())
        .ok_or_else(|| format!("No user font directory to read {} from", raw))?;

    let resolved = canonicalize(raw)?;
    if !resolved.starts_with(&root) {
        return Err(format!("Font is outside the user font folder: {}", raw));
    }
    if !resolved.is_file() {
        return Err(format!("Not a font file: {}", raw));
    }
    let is_font = resolved
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|ext| ext.eq_ignore_ascii_case("ttf") || ext.eq_ignore_ascii_case("otf"));
    if !is_font {
        return Err(format!("Not a supported font file: {}", raw));
    }
    Ok(resolved)
}

/// プリセット名を、ディレクトリを跨がない単純なファイル名として検証する
///
/// 実際に保存名を組み立てるのは core の `sanitize_filename` で、現状それだけでも
/// 外へは出られない。ここで重ねて検査するのは、**core が名前の許容文字を
/// 緩めた瞬間に GUI 側が無警告で traversal を獲得する**のを防ぐため。
/// 境界の契約は境界に書いておく。
pub fn preset_name(raw: &str) -> Result<String, String> {
    if raw.is_empty() {
        return Err("Preset name is empty".to_string());
    }
    if raw.chars().count() > MAX_PRESET_NAME_CHARS {
        return Err(format!(
            "Preset name must be {} characters or fewer",
            MAX_PRESET_NAME_CHARS
        ));
    }
    // OS ごとの区切り文字差を当てにしない（`a\b` は Unix では1成分になる）。
    if raw.contains(['/', '\\']) || raw.chars().any(char::is_control) {
        return Err(format!("Invalid preset name: {}", raw));
    }
    if Path::new(raw).file_name() != Some(OsStr::new(raw)) {
        return Err(format!("Invalid preset name: {}", raw));
    }
    Ok(raw.to_string())
}

/// 書き込み先ディレクトリとして検証し、実体のパスを返す
///
/// 未作成でもよい（呼び出し元が作る）が、許可ルート配下でなければ拒否する。
pub fn writable_dir(roots: &WritableRoots, raw: &str) -> Result<PathBuf, String> {
    let resolved = resolve_for_creation(raw)?;
    if !roots.allows(&resolved) {
        return Err(format!(
            "Output folder is outside the folders you selected: {}",
            raw
        ));
    }
    if resolved.exists() && !resolved.is_dir() {
        return Err(format!("Output path is not a directory: {}", raw));
    }
    Ok(resolved)
}

fn canonicalize(raw: &str) -> Result<PathBuf, String> {
    if raw.is_empty() {
        return Err("Empty path".to_string());
    }
    fs_canonicalize(Path::new(raw)).map_err(|e| format!("Cannot access {}: {}", raw, e))
}

/// まだ存在しないパスを、実在する祖先を基準に解決する
///
/// 祖先までは `canonicalize` で実体化し、未作成の部分は通常のファイル名成分だけを許す。
/// `Path::file_name` は `.` / `..` に対して `None` を返すため、未作成部分に
/// 相対参照を紛れ込ませる経路はここで閉じている。
fn resolve_for_creation(raw: &str) -> Result<PathBuf, String> {
    let requested = Path::new(raw);
    if raw.is_empty() {
        return Err("Empty path".to_string());
    }
    if !requested.is_absolute() {
        return Err(format!("Path must be absolute: {}", raw));
    }

    let mut tail: Vec<OsString> = Vec::new();
    let mut cursor = requested;

    loop {
        if let Ok(base) = fs_canonicalize(cursor) {
            let mut resolved = base;
            for name in tail.iter().rev() {
                resolved.push(name);
            }
            return Ok(resolved);
        }

        let name = cursor
            .file_name()
            .ok_or_else(|| format!("Invalid path: {}", raw))?;
        tail.push(name.to_os_string());
        cursor = cursor
            .parent()
            .ok_or_else(|| format!("Cannot resolve path: {}", raw))?;
    }
}

/// `dunce` 相当の正規化は行わない薄いラッパー。テストから差し替えやすくするためだけに分けている。
fn fs_canonicalize(path: &Path) -> std::io::Result<PathBuf> {
    std::fs::canonicalize(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn as_str(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }

    /// 中身も本物の JPEG を書く。拡張子だけのダミーにすると、実装を
    /// マジックナンバー判定に置き換えたときにこのテストが落ちてしまう
    /// （＝実装に依存したテストになる）。
    fn write_real_jpeg(path: &Path) {
        let img = image::RgbImage::from_pixel(4, 4, image::Rgb([120, 120, 120]));
        image::DynamicImage::ImageRgb8(img).save(path).unwrap();
    }

    // --- 読み出しの境界: 実体が対応画像形式のときだけ許可する ---

    #[test]
    fn readable_image_accepts_a_supported_image() {
        let dir = TempDir::new().unwrap();
        let jpeg = dir.path().join("photo.jpg");
        write_real_jpeg(&jpeg);

        let resolved = readable_image(&as_str(&jpeg)).expect("a real photo must be readable");
        assert_eq!(resolved, fs::canonicalize(&jpeg).unwrap());
    }

    #[test]
    fn readable_image_rejects_files_that_are_not_images() {
        let dir = TempDir::new().unwrap();
        let secret = dir.path().join("id_rsa");
        fs::write(&secret, b"-----BEGIN OPENSSH PRIVATE KEY-----").unwrap();

        // 前提: ファイル自体は実在する（「存在しないから拒否された」ではない）
        assert!(secret.is_file());
        assert!(readable_image(&as_str(&secret)).is_err());
    }

    #[test]
    #[cfg(unix)]
    fn readable_image_rejects_an_image_named_symlink_to_a_secret() {
        let dir = TempDir::new().unwrap();
        let secret = dir.path().join("id_rsa");
        fs::write(&secret, b"-----BEGIN OPENSSH PRIVATE KEY-----").unwrap();
        let disguised = dir.path().join("innocent.jpg");
        std::os::unix::fs::symlink(&secret, &disguised).unwrap();

        // 前提: 名前だけ見れば対応画像であり、リンクも辿れる状態にある
        assert!(core::is_supported_image(&disguised));
        assert!(disguised.is_file());

        assert!(readable_image(&as_str(&disguised)).is_err());
    }

    #[test]
    fn readable_image_rejects_a_directory_and_a_missing_path() {
        let dir = TempDir::new().unwrap();
        assert!(readable_image(&as_str(dir.path())).is_err());
        assert!(readable_image(&as_str(&dir.path().join("nope.jpg"))).is_err());
    }

    // --- 書き込みの境界: ネイティブダイアログで許可したルート配下だけ ---

    #[test]
    fn nothing_is_writable_before_the_user_picks_a_folder() {
        let dir = TempDir::new().unwrap();
        let roots = WritableRoots::default();
        assert!(writable_dir(&roots, &as_str(dir.path())).is_err());
    }

    #[test]
    fn writable_dir_rejects_paths_outside_every_granted_root() {
        let granted = TempDir::new().unwrap();
        let elsewhere = TempDir::new().unwrap();
        let roots = WritableRoots::default();
        roots.grant(fs::canonicalize(granted.path()).unwrap());

        // 前提: 許可したルート自身は通る（何を渡しても拒否される実装ではない）
        assert!(writable_dir(&roots, &as_str(granted.path())).is_ok());

        assert!(writable_dir(&roots, &as_str(elsewhere.path())).is_err());
    }

    #[test]
    fn writable_dir_allows_a_not_yet_created_subfolder_of_a_granted_root() {
        let granted = TempDir::new().unwrap();
        let roots = WritableRoots::default();
        let root = fs::canonicalize(granted.path()).unwrap();
        roots.grant(root.clone());

        let target = granted.path().join("out/2026");
        // 前提: まだ存在しない（既存ディレクトリを通しているだけではない）
        assert!(!target.exists());

        let resolved = writable_dir(&roots, &as_str(&target)).expect("subfolder must be allowed");
        assert_eq!(resolved, root.join("out").join("2026"));
    }

    #[test]
    fn writable_dir_rejects_traversal_out_of_a_granted_root() {
        let parent = TempDir::new().unwrap();
        let granted = parent.path().join("granted");
        fs::create_dir(&granted).unwrap();
        let roots = WritableRoots::default();
        roots.grant(fs::canonicalize(&granted).unwrap());

        // 前提: 許可ルート自身は通る
        assert!(writable_dir(&roots, &as_str(&granted)).is_ok());

        let escape = format!("{}/../escaped", granted.display());
        assert!(writable_dir(&roots, &escape).is_err());
    }

    /// 上のテストの `granted/..` は**実在する**ので `canonicalize` 側で解決されて終わる。
    /// 未作成の途中成分を挟むと解決できず、`resolve_for_creation` の
    /// 「`file_name()` が `None` を返す成分は拒否」という別の防御機構に入る。
    /// そちらを一度も発火させないままだと、外した時に気づけない。
    #[test]
    fn writable_dir_rejects_traversal_hidden_behind_a_not_yet_created_component() {
        let parent = TempDir::new().unwrap();
        let granted = parent.path().join("granted");
        fs::create_dir(&granted).unwrap();
        let roots = WritableRoots::default();
        roots.grant(fs::canonicalize(&granted).unwrap());

        let escape = format!("{}/not-created-yet/../../escaped", granted.display());
        // 前提: 途中成分が実在しない（実在すれば canonicalize 側の分岐に落ちる）
        assert!(!granted.join("not-created-yet").exists());

        assert!(writable_dir(&roots, &escape).is_err());
    }

    /// 許可ルートと同じ文字列で始まるだけの兄弟ディレクトリ。
    /// `Path::starts_with` は成分単位なので現状は正しく拒否されるが、
    /// 文字列比較に書き換えられた瞬間に通ってしまうためロックしておく。
    #[test]
    fn writable_dir_rejects_a_sibling_whose_name_merely_starts_with_a_granted_root() {
        let parent = TempDir::new().unwrap();
        let granted = parent.path().join("photos");
        let sibling = parent.path().join("photos-evil");
        fs::create_dir(&granted).unwrap();
        fs::create_dir(&sibling).unwrap();
        let roots = WritableRoots::default();
        roots.grant(fs::canonicalize(&granted).unwrap());

        // 前提: 文字列としては許可ルートを接頭辞に持つ
        assert!(as_str(&sibling).starts_with(&as_str(&granted)));

        assert!(writable_dir(&roots, &as_str(&sibling)).is_err());
    }

    // --- フォントの境界: ユーザーフォントディレクトリ配下の ttf/otf のみ ---

    #[test]
    fn readable_font_accepts_a_font_inside_the_user_font_folder() {
        let dir = TempDir::new().unwrap();
        let font = dir.path().join("MyFont.ttf");
        fs::write(
            &font,
            b"not really a font, but the extension is what we gate on",
        )
        .unwrap();

        let resolved = readable_font_in(Some(dir.path()), &as_str(&font))
            .expect("a ttf in the font folder must be readable");
        assert_eq!(resolved, fs::canonicalize(&font).unwrap());
    }

    #[test]
    fn readable_font_rejects_paths_outside_the_user_font_folder() {
        let fonts = TempDir::new().unwrap();
        let elsewhere = TempDir::new().unwrap();
        let outside = elsewhere.path().join("evil.ttf");
        fs::write(&outside, b"x").unwrap();

        // 前提: 拡張子は正当なので、拒否の理由は「場所」である
        assert!(outside.is_file());
        assert!(readable_font_in(Some(fonts.path()), &as_str(&outside)).is_err());
    }

    #[test]
    fn readable_font_rejects_non_font_files_and_devices() {
        let dir = TempDir::new().unwrap();
        let notes = dir.path().join("notes.txt");
        fs::write(&notes, b"x").unwrap();

        assert!(readable_font_in(Some(dir.path()), &as_str(&notes)).is_err());
        // 無制限読み込みの原因になっていた経路
        assert!(readable_font_in(Some(dir.path()), "/dev/zero").is_err());
    }

    #[test]
    #[cfg(unix)]
    fn readable_font_rejects_a_font_named_symlink_that_escapes_the_folder() {
        let fonts = TempDir::new().unwrap();
        let elsewhere = TempDir::new().unwrap();
        let secret = elsewhere.path().join("id_rsa");
        fs::write(&secret, b"-----BEGIN OPENSSH PRIVATE KEY-----").unwrap();
        let disguised = fonts.path().join("innocent.ttf");
        std::os::unix::fs::symlink(&secret, &disguised).unwrap();

        // 前提: パス文字列の上ではフォントフォルダー配下の ttf に見える
        assert!(disguised.starts_with(fonts.path()));

        assert!(readable_font_in(Some(fonts.path()), &as_str(&disguised)).is_err());
    }

    #[test]
    fn readable_font_rejects_everything_when_there_is_no_font_folder() {
        let dir = TempDir::new().unwrap();
        let font = dir.path().join("MyFont.ttf");
        fs::write(&font, b"x").unwrap();

        assert!(readable_font_in(None, &as_str(&font)).is_err());
    }

    // --- プリセット名の境界: ディレクトリを跨がない単純な名前のみ ---

    #[test]
    fn preset_name_accepts_ordinary_names() {
        assert_eq!(preset_name("my preset").unwrap(), "my preset");
        assert_eq!(preset_name("散歩用").unwrap(), "散歩用");
    }

    #[test]
    fn preset_name_rejects_traversal_and_separators() {
        for name in ["../evil", "..", ".", "a/b", "a\\b", "", "x\0y"] {
            assert!(
                preset_name(name).is_err(),
                "{:?} must be rejected as a preset name",
                name
            );
        }
    }

    #[test]
    fn preset_name_rejects_names_that_are_too_long_for_a_filename() {
        assert!(preset_name(&"a".repeat(MAX_PRESET_NAME_CHARS)).is_ok());
        assert!(preset_name(&"a".repeat(MAX_PRESET_NAME_CHARS + 1)).is_err());
    }

    #[test]
    #[cfg(unix)]
    fn writable_dir_rejects_a_symlink_that_points_out_of_a_granted_root() {
        let granted = TempDir::new().unwrap();
        let elsewhere = TempDir::new().unwrap();
        let roots = WritableRoots::default();
        roots.grant(fs::canonicalize(granted.path()).unwrap());

        let bridge = granted.path().join("bridge");
        std::os::unix::fs::symlink(elsewhere.path(), &bridge).unwrap();

        // 前提: パス文字列の上では許可ルート配下に見える
        assert!(bridge.starts_with(granted.path()));

        assert!(writable_dir(&roots, &as_str(&bridge)).is_err());
    }

    #[test]
    fn writable_dir_rejects_relative_paths() {
        let granted = TempDir::new().unwrap();
        let roots = WritableRoots::default();
        roots.grant(fs::canonicalize(granted.path()).unwrap());

        assert!(writable_dir(&roots, "out").is_err());
        assert!(writable_dir(&roots, "").is_err());
    }

    // --- 一覧の境界: 実在ディレクトリのみ ---

    #[test]
    fn existing_dir_accepts_only_directories_that_exist() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("photo.jpg");
        write_real_jpeg(&file);

        assert!(existing_dir(&as_str(dir.path())).is_ok());
        assert!(existing_dir(&as_str(&file)).is_err());
        assert!(existing_dir(&as_str(&dir.path().join("missing"))).is_err());
    }
}
