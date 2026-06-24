/// Runtime configuration for the AI copilot.
/// Loaded in `api-gateway` from env — never stored inside `SafeAIContext`.
#[derive(Debug, Clone)]
pub struct AiConfig {
    pub api_key: Option<String>,
    pub model: String,
    pub max_tokens: u32,
    pub timeout_secs: u64,
    pub max_user_message_len: usize,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            api_key: std::env::var("OPENAI_API_KEY").ok().filter(|k| !k.is_empty()),
            model: std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into()),
            max_tokens: std::env::var("OPENAI_MAX_TOKENS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(256),
            timeout_secs: std::env::var("OPENAI_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(15),
            max_user_message_len: 500,
        }
    }
}

impl AiConfig {
    pub fn openai_enabled(&self) -> bool {
        self.api_key.is_some()
    }
}
