use anyhow::Result;
use rust_embed::Embed;
use serde::Deserialize;
use std::collections::HashMap;

/// model_map.json のみを埋め込む
#[derive(Embed)]
#[folder = "assets/"]
#[include = "model_map.json"]
struct ModelMapAssets;

#[derive(Debug, Deserialize)]
struct ModelMapJson {
    camera: HashMap<String, String>,
    logo_match: HashMap<String, LogoMatchEntry>,
    lens_brand_match: Vec<LensBrandRule>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LogoMatchEntry {
    pub maker: String,
    pub brand: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LensBrandRule {
    pattern: String,
    match_type: String,
    logo: String,
}

pub struct ModelMap {
    camera: HashMap<String, String>,
    logo_match: HashMap<String, LogoMatchEntry>,
    lens_brand_match: Vec<LensBrandRule>,
}

impl ModelMap {
    pub fn load_bundled() -> Self {
        let data = ModelMapAssets::get("model_map.json")
            .expect("bundled model_map.json not found");
        let json: ModelMapJson = serde_json::from_slice(&data.data)
            .expect("invalid bundled model_map.json");
        Self {
            camera: json.camera,
            logo_match: json.logo_match,
            lens_brand_match: json.lens_brand_match,
        }
    }

    pub fn merge_custom(&mut self, json_str: &str) -> Result<()> {
        let custom: ModelMapJson = serde_json::from_str(json_str)?;
        for (k, v) in custom.camera {
            self.camera.insert(k, v);
        }
        for (k, v) in custom.logo_match {
            self.logo_match.insert(k, v);
        }
        let mut merged = custom.lens_brand_match;
        merged.extend(self.lens_brand_match.drain(..));
        self.lens_brand_match = merged;
        Ok(())
    }

    pub fn camera_display_name<'a>(&'a self, model: &'a str) -> &'a str {
        self.camera.get(model).map(|s| s.as_str()).unwrap_or(model)
    }

    pub fn maker_logo(&self, make: &str) -> Option<&LogoMatchEntry> {
        self.logo_match.get(make)
    }

    pub fn lens_brand_logo(&self, lens_model: &str) -> Option<&str> {
        for rule in &self.lens_brand_match {
            match rule.match_type.as_str() {
                "contains" => {
                    if lens_model.contains(&rule.pattern) {
                        return Some(&rule.logo);
                    }
                }
                _ => {}
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_model_lookup() {
        let map = ModelMap::load_bundled();
        assert_eq!(map.camera_display_name("ILCE-7M4"), "α7IV");
        assert_eq!(map.camera_display_name("UNKNOWN-123"), "UNKNOWN-123");
    }

    #[test]
    fn maker_logo_lookup() {
        let map = ModelMap::load_bundled();
        let logo = map.maker_logo("SONY");
        assert!(logo.is_some());
        let logo = logo.unwrap();
        assert_eq!(logo.maker, "sony.svg");
        assert_eq!(logo.brand.as_deref(), Some("alpha.svg"));
    }

    #[test]
    fn maker_logo_unknown() {
        let map = ModelMap::load_bundled();
        assert!(map.maker_logo("UNKNOWN_MAKER").is_none());
    }

    #[test]
    fn lens_brand_match_priority() {
        let map = ModelMap::load_bundled();
        let logo = map.lens_brand_logo("FE 24-70mm f/2.8 GM II");
        assert_eq!(logo, Some("gmaster.svg"));
    }

    #[test]
    fn lens_brand_match_g_lens() {
        let map = ModelMap::load_bundled();
        let logo = map.lens_brand_logo("FE 70-200mm f/4 G OSS II");
        assert_eq!(logo, Some("sony_g.svg"));
    }

    #[test]
    fn custom_map_merge() {
        let mut map = ModelMap::load_bundled();
        let custom_json = r#"{
            "camera": { "CUSTOM-1": "Custom Camera" },
            "logo_match": {},
            "lens_brand_match": []
        }"#;
        map.merge_custom(custom_json).unwrap();
        assert_eq!(map.camera_display_name("CUSTOM-1"), "Custom Camera");
        assert_eq!(map.camera_display_name("ILCE-7M4"), "α7IV");
    }
}
