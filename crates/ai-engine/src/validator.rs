pub struct AiResponseValidator;

impl AiResponseValidator {
    pub fn validate(response: &str) -> bool {
        const FORBIDDEN: &[&str] = &[
            "postgres://",
            "sk-",
            "jwt_secret",
            "admin_secret",
            "select * from",
            "api key",
            "bearer ",
            "openai_api",
        ];
        let lower = response.to_lowercase();
        !FORBIDDEN.iter().any(|x| lower.contains(x))
    }
}
