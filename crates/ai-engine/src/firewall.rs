pub struct AiFirewall;

impl AiFirewall {
    pub fn detect_prompt_injection(input: &str) -> bool {
        const BLOCKED: &[&str] = &[
            "ignore instructions",
            "reveal prompt",
            "show admin",
            "execute sql",
            "api keys",
            "developer message",
            "internal config",
            "wallet table",
            "system prompt",
            "jailbreak",
            "dan mode",
        ];
        let lower = input.to_lowercase();
        BLOCKED.iter().any(|x| lower.contains(x))
    }

    pub fn sanitize_user_message(input: &str, max_len: usize) -> Option<String> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return None;
        }
        if trimmed.len() > max_len {
            return None;
        }
        Some(trimmed.to_string())
    }
}
