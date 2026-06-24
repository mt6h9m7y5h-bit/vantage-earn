use thiserror::Error;

#[derive(Debug, Error)]
pub enum AiError {
    #[error("prompt injection detected")]
    PromptInjection,

    #[error("user message too long (max {max} chars)")]
    MessageTooLong { max: usize },

    #[error("response validation failed")]
    ResponseRejected,

    #[error("empty response from model")]
    EmptyResponse,

    #[error("openai client error: {0}")]
    Provider(String),

    #[error("request timed out after {0}s")]
    Timeout(u64),
}
