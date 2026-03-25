use super::ExifFrameConfig;
use anyhow::Result;
use rust_embed::Embed;
use std::fs;
use std::path::Path;

#[derive(Embed)]
#[folder = "assets/presets/"]
struct PresetAssets;

/// バンドルプリセットを読み込み
pub fn load_bundled_presets() -> Vec<ExifFrameConfig> {
    let mut presets = Vec::new();
    for file in PresetAssets::iter() {
        if file.ends_with(".json") {
            if let Some(data) = PresetAssets::get(&file) {
                if let Ok(config) = serde_json::from_slice::<ExifFrameConfig>(&data.data) {
                    presets.push(config);
                }
            }
        }
    }
    presets
}

/// ユーザープリセットディレクトリから読み込み
pub fn load_user_presets(presets_dir: &Path) -> Vec<ExifFrameConfig> {
    let mut presets = Vec::new();
    if let Ok(entries) = fs::read_dir(presets_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "json") {
                if let Ok(data) = fs::read_to_string(&path) {
                    if let Ok(config) = serde_json::from_str::<ExifFrameConfig>(&data) {
                        presets.push(config);
                    }
                }
            }
        }
    }
    presets
}

/// 全プリセット一覧（バンドル + ユーザー、ユーザー側が優先）
pub fn list_all_presets(user_presets_dir: Option<&Path>) -> Vec<ExifFrameConfig> {
    let bundled = load_bundled_presets();
    let user = user_presets_dir
        .map(|d| load_user_presets(d))
        .unwrap_or_default();

    let mut result = bundled;
    for u in user {
        if let Some(pos) = result.iter().position(|b| b.name == u.name) {
            result[pos] = u;
        } else {
            result.push(u);
        }
    }
    result
}

/// プリセットを保存（同名は上書き）
pub fn save_preset(presets_dir: &Path, config: &ExifFrameConfig) -> Result<()> {
    fs::create_dir_all(presets_dir)?;
    let filename = sanitize_filename(&config.name);
    let path = presets_dir.join(format!("{}.json", filename));
    let json = serde_json::to_string_pretty(config)?;
    fs::write(path, json)?;
    Ok(())
}

/// プリセットを削除
pub fn delete_preset(presets_dir: &Path, name: &str) -> Result<()> {
    let filename = sanitize_filename(name);
    let path = presets_dir.join(format!("{}.json", filename));
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn load_bundled_presets_test() {
        let presets = load_bundled_presets();
        assert!(!presets.is_empty());
        assert!(presets.iter().any(|p| p.name == "default"));
    }

    #[test]
    fn save_and_load_user_preset() {
        let dir = TempDir::new().unwrap();
        let config = ExifFrameConfig::default();
        save_preset(dir.path(), &config).unwrap();
        let loaded = load_user_presets(dir.path());
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "default");
    }

    #[test]
    fn save_preset_overwrites_existing() {
        let dir = TempDir::new().unwrap();
        let mut config = ExifFrameConfig::default();
        save_preset(dir.path(), &config).unwrap();
        config.custom_text = "updated".to_string();
        save_preset(dir.path(), &config).unwrap();
        let loaded = load_user_presets(dir.path());
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].custom_text, "updated");
    }

    #[test]
    fn delete_user_preset_test() {
        let dir = TempDir::new().unwrap();
        let config = ExifFrameConfig::default();
        save_preset(dir.path(), &config).unwrap();
        delete_preset(dir.path(), "default").unwrap();
        let loaded = load_user_presets(dir.path());
        assert!(loaded.is_empty());
    }

    #[test]
    fn list_all_presets_merges_bundled_and_user() {
        let dir = TempDir::new().unwrap();
        let mut user_preset = ExifFrameConfig::default();
        user_preset.name = "my_custom".to_string();
        save_preset(dir.path(), &user_preset).unwrap();
        let all = list_all_presets(Some(dir.path()));
        assert!(all.iter().any(|p| p.name == "default"));
        assert!(all.iter().any(|p| p.name == "my_custom"));
    }
}
