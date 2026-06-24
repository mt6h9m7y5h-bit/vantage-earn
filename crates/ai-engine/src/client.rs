use std::sync::Arc;

use async_trait::async_trait;

use crate::error::AiError;

/// Abstraction over LLM providers — enables mocks in tests and provider swaps.
#[async_trait]
pub trait LlmBackend: Send + Sync {
    async fn complete(&self, system_prompt: &str, user_message: &str) -> Result<String, AiError>;
}

pub type SharedLlmBackend = Arc<dyn LlmBackend>;

/// Rule-based fallback when no API key is configured (dev / CI).
pub struct FallbackBackend;

#[async_trait]
impl LlmBackend for FallbackBackend {
    async fn complete(&self, system_prompt: &str, user_message: &str) -> Result<String, AiError> {
        let lang = system_prompt
            .lines()
            .find(|l| l.starts_with("LANGUAGE:"))
            .and_then(|l| l.split("in ").nth(1))
            .map(|s| s.trim().trim_end_matches('.'))
            .unwrap_or("en");

        Ok(format!(
            "🔥 ({lang}) Got it! \"{user_message}\" — keep grinding, every session counts toward your goal!"
        ))
    }
}

#[cfg(feature = "openai")]
pub mod openai {
    use std::time::Duration;

    use async_openai::{
        config::OpenAIConfig,
        types::chat::{
            ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
            ChatCompletionRequestSystemMessageContent, ChatCompletionRequestUserMessage,
            ChatCompletionRequestUserMessageContent, CreateChatCompletionRequestArgs,
        },
        Client,
    };
    use async_trait::async_trait;
    use tokio::time::timeout;
    use tracing::debug;

    use super::LlmBackend;
    use crate::config::AiConfig;
    use crate::error::AiError;

    pub struct OpenAiBackend {
        client: Client<OpenAIConfig>,
        model: String,
        max_tokens: u32,
        timeout: Duration,
    }

    impl OpenAiBackend {
        pub fn new(config: &AiConfig) -> Result<Self, AiError> {
            let api_key = config
                .api_key
                .as_deref()
                .ok_or_else(|| AiError::Provider("OPENAI_API_KEY not set".into()))?;

            let openai_config = OpenAIConfig::new().with_api_key(api_key);
            Ok(Self {
                client: Client::with_config(openai_config),
                model: config.model.clone(),
                max_tokens: config.max_tokens,
                timeout: Duration::from_secs(config.timeout_secs),
            })
        }
    }

    #[async_trait]
    impl LlmBackend for OpenAiBackend {
        async fn complete(
            &self,
            system_prompt: &str,
            user_message: &str,
        ) -> Result<String, AiError> {
            let request = CreateChatCompletionRequestArgs::default()
                .model(&self.model)
                .max_tokens(self.max_tokens)
                .messages(vec![
                    ChatCompletionRequestMessage::System(ChatCompletionRequestSystemMessage {
                        content: ChatCompletionRequestSystemMessageContent::Text(
                            system_prompt.to_string(),
                        ),
                        name: None,
                    }),
                    ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
                        content: ChatCompletionRequestUserMessageContent::Text(
                            user_message.to_string(),
                        ),
                        name: None,
                    }),
                ])
                .build()
                .map_err(|e| AiError::Provider(e.to_string()))?;

            debug!(model = %self.model, "openai chat request");

            let response = timeout(self.timeout, self.client.chat().create(request))
                .await
                .map_err(|_| AiError::Timeout(self.timeout.as_secs()))?
                .map_err(|e| AiError::Provider(e.to_string()))?;

            let content = response
                .choices
                .first()
                .and_then(|c| c.message.content.clone())
                .filter(|t| !t.trim().is_empty())
                .ok_or(AiError::EmptyResponse)?;

            Ok(content.trim().to_string())
        }
    }
}

/// Build the best available backend from config.
pub fn build_backend(config: &crate::config::AiConfig) -> SharedLlmBackend {
    #[cfg(feature = "openai")]
    if config.openai_enabled() {
        match openai::OpenAiBackend::new(config) {
            Ok(backend) => return Arc::new(backend),
            Err(e) => tracing::warn!(error = %e, "OpenAI backend unavailable, using fallback"),
        }
    }

    let _ = config;
    Arc::new(FallbackBackend)
}
