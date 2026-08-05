use anyhow::{anyhow, Context, Result};
use rust_embed::Embed;
use serde::Deserialize;
use std::collections::HashMap;

/// model_map.json のみを埋め込む
#[derive(Embed)]
#[folder = "assets/"]
#[include = "model_map.json"]
struct ModelMapAssets;

#[derive(Debug, Default, Deserialize)]
struct ModelMapJson {
    /// カスタムマップで片方だけ上書きできるよう、両フィールドとも省略可能にする
    #[serde(default)]
    logo_match: HashMap<String, LogoMatchEntry>,
    #[serde(default)]
    lens_brand_match: Vec<LensBrandRule>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LogoMatchEntry {
    pub maker: String,
}

/// マッチ方式。未知の値は serde がエラーにするので黙って無視されない。
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum MatchType {
    Contains,
}

#[derive(Debug, Deserialize)]
struct LensBrandRule {
    pattern: String,
    match_type: MatchType,
    logo: String,
}

pub struct ModelMap {
    /// キーは正規化済み（trim + 大文字化）。表記揺れを人力で列挙しないため。
    logo_match: HashMap<String, LogoMatchEntry>,
    lens_brand_match: Vec<LensBrandRule>,
}

/// Exif の Make / メーカー名を比較用のキーに正規化する。
/// `"  sony "` も `"SONY"` も同じキーになる。
fn normalize_maker_key(make: &str) -> String {
    make.trim().to_uppercase()
}

impl ModelMap {
    pub fn load_bundled() -> Result<Self> {
        let data = ModelMapAssets::get("model_map.json")
            .context("bundled model_map.json not found in assets/")?;
        let json: ModelMapJson =
            serde_json::from_slice(&data.data).context("invalid bundled model_map.json")?;
        let mut map = Self {
            logo_match: HashMap::new(),
            lens_brand_match: Vec::new(),
        };
        map.merge(json).context("invalid bundled model_map.json")?;
        Ok(map)
    }

    pub fn merge_custom(&mut self, json_str: &str) -> Result<()> {
        let custom: ModelMapJson =
            serde_json::from_str(json_str).context("failed to parse custom model map")?;
        self.merge(custom)
    }

    fn merge(&mut self, json: ModelMapJson) -> Result<()> {
        for (make, entry) in json.logo_match {
            validate_asset_filename(&entry.maker).with_context(|| {
                format!("logo_match[{:?}].maker is not a usable filename", make)
            })?;
            self.logo_match.insert(normalize_maker_key(&make), entry);
        }
        for rule in &json.lens_brand_match {
            // 空パターンは `contains` で全レンズにマッチしてしまう
            if rule.pattern.trim().is_empty() {
                return Err(anyhow!(
                    "lens_brand_match has an empty pattern (it would match every lens)"
                ));
            }
            validate_asset_filename(&rule.logo).with_context(|| {
                format!(
                    "lens_brand_match[{:?}].logo is not a usable filename",
                    rule.pattern
                )
            })?;
        }
        // 後から与えたルールを先に評価する（カスタムがバンドルを上書きできる）
        let mut merged = json.lens_brand_match;
        merged.append(&mut self.lens_brand_match);
        self.lens_brand_match = merged;
        Ok(())
    }

    /// Exif の Make からメーカーロゴを引く。
    /// 完全一致（正規化後）→ 先頭トークンの順に試すので、
    /// `"Sony Corporation"` や `"NIKON CORPORATION"` を個別に列挙しなくてよい。
    pub fn maker_logo(&self, make: &str) -> Option<&LogoMatchEntry> {
        let key = normalize_maker_key(make);
        if let Some(entry) = self.logo_match.get(&key) {
            return Some(entry);
        }
        let first_token = key.split_whitespace().next()?;
        if first_token == key {
            return None;
        }
        self.logo_match.get(first_token)
    }

    pub fn lens_brand_logo(&self, lens_model: &str) -> Option<&str> {
        for rule in &self.lens_brand_match {
            match rule.match_type {
                MatchType::Contains => {
                    if lens_model.contains(&rule.pattern) {
                        return Some(&rule.logo);
                    }
                }
            }
        }
        None
    }
}

/// アセット参照名がディレクトリを脱出しないことを保証する（S4-M8）。
/// `model_map.json` はユーザーが差し替えられるため、そのまま `dir.join()` に
/// 渡すと `"../../secret"` のようなパスでディレクトリの外を読める。
pub fn validate_asset_filename(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(anyhow!("asset filename is empty"));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(anyhow!(
            "asset filename {:?} contains characters outside [A-Za-z0-9._-]",
            name
        ));
    }
    if name.contains("..") || name.starts_with('.') {
        return Err(anyhow!("asset filename {:?} is not a plain filename", name));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundled() -> ModelMap {
        ModelMap::load_bundled().expect("bundled model_map.json must be valid")
    }

    #[test]
    fn maker_logo_lookup() {
        let logo = bundled().maker_logo("SONY").cloned();
        assert_eq!(logo.expect("SONY should resolve").maker, "sony.svg");
    }

    /// Exif の Make はカメラごとに表記が揺れる（大小文字・前後空白・社名の接尾辞）。
    /// 仕様: 同じメーカーである限り同じロゴが返る。
    #[test]
    fn maker_logo_tolerates_make_string_variations() {
        let map = bundled();
        for make in [
            "SONY",
            "Sony",
            "sony",
            "  SONY  ",
            "Sony Corporation",
            "SONY CORPORATION",
        ] {
            let logo = map.maker_logo(make);
            assert!(logo.is_some(), "no maker logo for Exif Make {:?}", make);
            assert_eq!(logo.unwrap().maker, "sony.svg", "Make {:?}", make);
        }
    }

    #[test]
    fn maker_logo_fujifilm_variants() {
        let map = bundled();
        for make in ["FUJIFILM", "Fujifilm", "FUJIFILM Corporation"] {
            let logo = map.maker_logo(make);
            assert!(logo.is_some(), "no maker logo for Exif Make {:?}", make);
            assert_eq!(logo.unwrap().maker, "fujifilm.svg");
        }
    }

    #[test]
    fn maker_logo_unknown() {
        assert!(bundled().maker_logo("UNKNOWN_MAKER").is_none());
    }

    /// 正規化は「同じメーカー」を束ねるためのもので、別メーカーを混同してはならない。
    #[test]
    fn maker_logo_does_not_match_a_different_maker() {
        let map = bundled();
        assert!(map.maker_logo("Sonyx").is_none());
        assert!(map.maker_logo("Panasonic Corporation").is_none());
        assert!(map.maker_logo("").is_none());
    }

    #[test]
    fn lens_brand_match_gm() {
        assert_eq!(
            bundled().lens_brand_logo("FE 24-70mm f/2.8 GM II"),
            Some("gmaster.png")
        );
    }

    #[test]
    fn lens_brand_match_non_gm() {
        assert!(bundled()
            .lens_brand_logo("FE 70-200mm f/4 OSS II")
            .is_none());
    }

    #[test]
    fn custom_map_merge() {
        let mut map = bundled();
        map.merge_custom(
            r#"{
            "logo_match": { "CustomMaker": { "maker": "custom.svg" } },
            "lens_brand_match": []
        }"#,
        )
        .unwrap();
        assert!(map.maker_logo("CustomMaker").is_some());
        assert!(map.maker_logo("SONY").is_some());
    }

    /// 仕様: カスタムマップは片方のフィールドだけを書きたい場合がある（S4-M7）。
    #[test]
    fn custom_map_may_omit_either_field() {
        let mut map = bundled();
        map.merge_custom(r#"{ "logo_match": { "Foo": { "maker": "sony.svg" } } }"#)
            .expect("logo_match only should be accepted");
        map.merge_custom(r#"{ "lens_brand_match": [] }"#)
            .expect("lens_brand_match only should be accepted");
        assert!(map.maker_logo("Foo").is_some());
    }

    /// 仕様: 未知の match_type は黙って無視せずエラーにする（S4-M6）。
    /// 無視するとユーザーのルールが効いていないことに気づけない。
    #[test]
    fn unknown_match_type_is_rejected() {
        let mut map = bundled();
        let err = map.merge_custom(
            r#"{ "lens_brand_match": [
                { "pattern": "GM", "match_type": "regex", "logo": "gmaster.png" }
            ] }"#,
        );
        assert!(err.is_err(), "unknown match_type must be rejected");
    }

    /// 仕様: 空パターンの contains は全レンズにマッチしてしまうので受け付けない（S4-M5）。
    #[test]
    fn empty_lens_pattern_is_rejected() {
        let mut map = bundled();
        let err = map.merge_custom(
            r#"{ "lens_brand_match": [
                { "pattern": "", "match_type": "contains", "logo": "gmaster.png" }
            ] }"#,
        );
        assert!(err.is_err(), "empty pattern must be rejected");
        assert!(
            map.lens_brand_logo("FE 70-200mm f/4 OSS II").is_none(),
            "a rejected rule must not take effect"
        );
    }

    /// 仕様: ロゴ参照はアセットディレクトリ内の単純なファイル名に限る（S4-M8）。
    #[test]
    fn logo_filenames_escaping_the_asset_dir_are_rejected() {
        for bad in [
            "../../etc/passwd",
            "/etc/passwd",
            "sub/dir/logo.svg",
            "..",
            "",
        ] {
            assert!(
                validate_asset_filename(bad).is_err(),
                "{:?} must be rejected as an asset filename",
                bad
            );
            let mut map = bundled();
            let err = map.merge_custom(&format!(
                r#"{{ "logo_match": {{ "Evil": {{ "maker": "{}" }} }} }}"#,
                bad.replace('\\', "\\\\")
            ));
            assert!(err.is_err(), "maker {:?} must be rejected", bad);
        }
        assert!(validate_asset_filename("sony.svg").is_ok());
        assert!(validate_asset_filename("gmaster_light.png").is_ok());
    }
}
