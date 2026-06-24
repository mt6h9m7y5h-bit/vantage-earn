/// Runtime ad-provider configuration (env-driven, no secrets in public responses).
#[derive(Debug, Clone)]
pub struct AdConfig {
    pub provider: String,
    pub applixir_api_key: Option<String>,
    pub watch_duration_secs: u32,
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
        let watch_duration_secs = std::env::var("AD_WATCH_DURATION_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30);

        Self {
            provider,
            applixir_api_key,
            watch_duration_secs,
        }
    }
}

impl AdConfig {
    /// Effective provider exposed to clients: applixir only when key is present.
    pub fn effective_provider(&self) -> &str {
        if self.provider == "applixir" && self.applixir_api_key.is_some() {
            "applixir"
        } else {
            "mock"
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
            "watch_duration_secs": self.watch_duration_secs,
            "applixir_sdk_url": "https://cdn.applixir.com/applixir.app.v6.0.1.js",
        })
    }
}
