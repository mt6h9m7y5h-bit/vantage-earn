/// Runtime ad-provider configuration (env-driven, no secrets in public responses).
#[derive(Debug, Clone)]
pub struct AdConfig {
    pub provider: String,
    pub applixir_api_key: Option<String>,
    pub adinplay_tag_url: Option<String>,
    pub watch_duration_secs: u32,
}

fn adinplay_tag_url_from_env() -> Option<String> {
    if let Ok(url) = std::env::var("ADINPLAY_TAG_URL") {
        let url = url.trim().to_string();
        if !url.is_empty() {
            return Some(url);
        }
    }
    let publisher_id = std::env::var("ADINPLAY_PUBLISHER_ID")
        .ok()
        .filter(|k| !k.is_empty());
    let site_id = std::env::var("ADINPLAY_SITE_ID")
        .ok()
        .filter(|k| !k.is_empty());
    match (publisher_id, site_id) {
        (Some(pub_id), Some(site_id)) => Some(format!(
            "https://api.adinplay.com/libs/aiptag/pub/{pub_id}/{site_id}/tag.min.js"
        )),
        _ => None,
    }
}

impl Default for AdConfig {
    fn default() -> Self {
        let provider = std::env::var("AD_PROVIDER")
            .unwrap_or_else(|_| "mock".into())
            .to_lowercase();
        let applixir_api_key = std::env::var("APPLIXIR_API_KEY")
            .or_else(|_| std::env::var("APPLIXIR_APP_ID"))
            .ok()
            .filter(|k| !k.is_empty());
        let adinplay_tag_url = adinplay_tag_url_from_env();
        let watch_duration_secs = std::env::var("AD_WATCH_DURATION_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30);

        Self {
            provider,
            applixir_api_key,
            adinplay_tag_url,
            watch_duration_secs,
        }
    }
}

impl AdConfig {
    /// Effective provider exposed to clients; falls back to mock when credentials are missing.
    pub fn effective_provider(&self) -> &str {
        match self.provider.as_str() {
            "adinplay" if self.adinplay_tag_url.is_some() => "adinplay",
            "applixir" if self.applixir_api_key.is_some() => "applixir",
            _ => "mock",
        }
    }

    pub fn public_json(&self) -> serde_json::Value {
        let provider = self.effective_provider();
        serde_json::json!({
            "ad_provider": provider,
            "applixir_app_id": if provider == "applixir" {
                self.applixir_api_key.clone()
            } else {
                None
            },
            "adinplay_tag_url": if provider == "adinplay" {
                self.adinplay_tag_url.clone()
            } else {
                None
            },
            "watch_duration_secs": self.watch_duration_secs,
            "applixir_sdk_url": "https://cdn.applixir.com/applixir.app.v6.0.1.js",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config(
        provider: &str,
        applixir: Option<&str>,
        adinplay_url: Option<&str>,
    ) -> AdConfig {
        AdConfig {
            provider: provider.into(),
            applixir_api_key: applixir.map(String::from),
            adinplay_tag_url: adinplay_url.map(String::from),
            watch_duration_secs: 30,
        }
    }

    #[test]
    fn effective_provider_defaults_to_mock() {
        let cfg = sample_config("mock", None, None);
        assert_eq!(cfg.effective_provider(), "mock");
    }

    #[test]
    fn effective_provider_uses_adinplay_when_configured() {
        let cfg = sample_config(
            "adinplay",
            None,
            Some("https://api.adinplay.com/libs/aiptag/pub/ABC/example.com/tag.min.js"),
        );
        assert_eq!(cfg.effective_provider(), "adinplay");
        let json = cfg.public_json();
        assert_eq!(json["ad_provider"], "adinplay");
        assert!(json["adinplay_tag_url"].as_str().unwrap().contains("pub/ABC/example.com"));
    }

    #[test]
    fn adinplay_falls_back_without_credentials() {
        let cfg = sample_config("adinplay", None, None);
        assert_eq!(cfg.effective_provider(), "mock");
    }

    #[test]
    fn adinplay_tag_url_built_from_publisher_and_site() {
        let saved_pub = std::env::var("ADINPLAY_PUBLISHER_ID").ok();
        let saved_site = std::env::var("ADINPLAY_SITE_ID").ok();
        let saved_tag = std::env::var("ADINPLAY_TAG_URL").ok();
        std::env::set_var("ADINPLAY_PUBLISHER_ID", "XYZ");
        std::env::set_var("ADINPLAY_SITE_ID", "game.test");
        std::env::remove_var("ADINPLAY_TAG_URL");
        let url = adinplay_tag_url_from_env().unwrap();
        assert!(url.contains("pub/XYZ/game.test"));
        match saved_pub {
            Some(v) => std::env::set_var("ADINPLAY_PUBLISHER_ID", v),
            None => std::env::remove_var("ADINPLAY_PUBLISHER_ID"),
        }
        match saved_site {
            Some(v) => std::env::set_var("ADINPLAY_SITE_ID", v),
            None => std::env::remove_var("ADINPLAY_SITE_ID"),
        }
        match saved_tag {
            Some(v) => std::env::set_var("ADINPLAY_TAG_URL", v),
            None => std::env::remove_var("ADINPLAY_TAG_URL"),
        }
    }
}
