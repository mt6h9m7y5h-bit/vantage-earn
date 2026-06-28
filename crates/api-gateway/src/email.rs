use std::collections::HashMap;
use std::path::PathBuf;

use shared::AppResult;

/// Transactional email (welcome, password reset, …).
///
/// Production (Render): Resend HTTP API — no outbound SMTP ports required.
/// - `RESEND_API_KEY` or `SMTP_PASS` — Resend API key (`re_…`); `SMTP_PASS` is kept for
///   backward compatibility with existing Render secrets.
/// - `SMTP_FROM` — sender (no-reply), e.g. `VANTAGE-EARN <noreply@deine-domain.de>`
///
/// Local dev fallback: classic SMTP when `SMTP_HOST` + `SMTP_USER` + non-`re_` `SMTP_PASS` are set.
/// - `SMTP_HOST`, `SMTP_PORT` (default `587`), `SMTP_USER`, `SMTP_PASS`, `SMTP_FROM`
///
/// Other:
/// - `APP_URL` — origin for reset links (default `https://vantage-earn.onrender.com`)
/// - `EMAIL_TEMPLATES_DIR` — override template folder (default `templates/email`)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmailTransport {
    ResendApi,
    Smtp,
    Disabled,
}

#[derive(Clone)]
pub struct EmailService {
    templates_dir: PathBuf,
    app_url: String,
    resend: Option<ResendConfig>,
    smtp: Option<SmtpConfig>,
}

#[derive(Clone)]
struct ResendConfig {
    api_key: String,
    from: String,
}

#[derive(Clone)]
struct SmtpConfig {
    host: String,
    port: u16,
    user: String,
    pass: String,
    from: String,
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn normalize_secret(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2 {
        let bytes = trimmed.as_bytes();
        if (bytes[0] == b'"' && bytes[trimmed.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[trimmed.len() - 1] == b'\'')
        {
            return trimmed[1..trimmed.len() - 1].trim().to_string();
        }
    }
    trimmed.to_string()
}

fn extract_resend_api_key(value: &str) -> Option<String> {
    let normalized = normalize_secret(value);
    if normalized.starts_with("re_") {
        return Some(normalized);
    }
    let idx = normalized.find("re_")?;
    let rest = &normalized[idx..];
    let end = rest
        .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
        .unwrap_or(rest.len());
    let key = rest[..end].to_string();
    if key.len() > 3 {
        Some(key)
    } else {
        None
    }
}

fn looks_like_resend_api_key(value: &str) -> bool {
    extract_resend_api_key(value).is_some()
}

fn smtp_from_address() -> String {
    non_empty_env("SMTP_FROM")
        .unwrap_or_else(|| "VANTAGE-EARN <onboarding@resend.dev>".into())
}

/// Domains that Resend will reject or that indicate an unset production config.
fn is_unverified_sender_domain(from: &str) -> bool {
    let lower = from.to_ascii_lowercase();
    [
        "deine-domain.de",
        "vantage-earn.onrender.com",
        "example.com",
        "your-domain",
    ]
    .iter()
    .any(|d| lower.contains(d))
}

fn warn_if_suspicious_from_address(from: &str) {
    let lower = from.to_ascii_lowercase();
    if lower.contains("onboarding@resend.dev") {
        tracing::warn!(
            from = %from,
            "SMTP_FROM uses onboarding@resend.dev — Resend only delivers to your account email; verify a domain for real users"
        );
        return;
    }
    if is_unverified_sender_domain(from) {
        tracing::warn!(
            from = %from,
            "SMTP_FROM uses an unverified/placeholder domain — Resend will return 403; set a verified domain in Render env"
        );
    }
}

struct ResolvedResendKey {
    key: String,
    source: &'static str,
}

fn resolve_resend_api_key() -> Option<String> {
    resolve_resend_config().map(|resolved| resolved.key)
}

fn resolve_resend_config() -> Option<ResolvedResendKey> {
    if let Some(key) = non_empty_env("RESEND_API_KEY") {
        if let Some(normalized) = extract_resend_api_key(&key) {
            return Some(ResolvedResendKey {
                key: normalized,
                source: "RESEND_API_KEY",
            });
        }
        tracing::warn!("RESEND_API_KEY set but no re_ API key found — ignoring");
    }
    if let Some(pass) = non_empty_env("SMTP_PASS") {
        if let Some(normalized) = extract_resend_api_key(&pass) {
            return Some(ResolvedResendKey {
                key: normalized,
                source: "SMTP_PASS",
            });
        }
    }
    None
}

fn resend_from_env() -> Option<ResendConfig> {
    let resolved = resolve_resend_config()?;
    Some(ResendConfig {
        api_key: resolved.key,
        from: smtp_from_address(),
    })
}

fn smtp_from_env() -> Option<SmtpConfig> {
    if resolve_resend_api_key().is_some() {
        return None;
    }

    if crate::release_info::is_production() {
        if non_empty_env("SMTP_HOST").is_some() {
            tracing::warn!(
                "SMTP_HOST ignored in production — Render blocks outbound SMTP; use RESEND_API_KEY or SMTP_PASS (re_…)"
            );
        }
        return None;
    }

    let host = non_empty_env("SMTP_HOST")?;
    let port = non_empty_env("SMTP_PORT")
        .and_then(|p| p.parse().ok())
        .unwrap_or(587);
    let user = non_empty_env("SMTP_USER");
    let pass = non_empty_env("SMTP_PASS");
    let from = smtp_from_address();

    if user.is_none() || pass.is_none() {
        tracing::warn!(
            host = %host,
            "SMTP_HOST set but SMTP_USER/SMTP_PASS missing — SMTP relay disabled"
        );
        return None;
    }

    let pass = pass.unwrap();
    if looks_like_resend_api_key(&pass) {
        tracing::error!(
            "SMTP_PASS looks like a Resend API key (re_…) — SMTP relay disabled; Resend HTTP API is used instead"
        );
        return None;
    }

    Some(SmtpConfig {
        host,
        port,
        user: user.unwrap(),
        pass,
        from,
    })
}

impl EmailService {
    pub fn from_env() -> Self {
        let templates_dir = std::env::var("EMAIL_TEMPLATES_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("templates/email"));
        let app_url = std::env::var("APP_URL")
            .unwrap_or_else(|_| "https://vantage-earn.onrender.com".into());
        let resolved = resolve_resend_config();
        let resend_key_source = resolved.as_ref().map(|r| r.source);
        let resend = resend_from_env();
        let smtp = if resend.is_some() {
            None
        } else {
            smtp_from_env()
        };
        if let Some(resend) = &resend {
            warn_if_suspicious_from_address(&resend.from);
            tracing::info!(
                from = %resend.from,
                key_source = resend_key_source.unwrap_or("unknown"),
                "transactional email: Resend HTTP API configured"
            );
        } else if smtp.is_some() {
            tracing::info!("transactional email: SMTP relay configured (local dev fallback)");
        } else if crate::release_info::is_production() {
            tracing::error!(
                has_resend_api_key = non_empty_env("RESEND_API_KEY").is_some(),
                has_smtp_pass = non_empty_env("SMTP_PASS").is_some(),
                has_smtp_host = non_empty_env("SMTP_HOST").is_some(),
                smtp_pass_looks_like_re = non_empty_env("SMTP_PASS")
                    .is_some_and(|p| looks_like_resend_api_key(&p)),
                "transactional email NOT configured in production — set RESEND_API_KEY or SMTP_PASS (re_…); SMTP blocked on Render"
            );
        }
        Self {
            templates_dir,
            app_url,
            resend,
            smtp,
        }
    }

    pub fn transport_mode(&self) -> EmailTransport {
        if self.resend.is_some() {
            EmailTransport::ResendApi
        } else if self.smtp.is_some() {
            EmailTransport::Smtp
        } else {
            EmailTransport::Disabled
        }
    }

    pub fn health_status(&self) -> (&'static str, bool, Option<String>) {
        match self.transport_mode() {
            EmailTransport::ResendApi => ("resend_api", true, None),
            EmailTransport::Smtp => ("smtp", true, Some("local dev only".into())),
            EmailTransport::Disabled if crate::release_info::is_production() => (
                "none",
                false,
                Some("set RESEND_API_KEY or SMTP_PASS (re_…)".into()),
            ),
            EmailTransport::Disabled => ("none", false, None),
        }
    }

    /// Public app URL with `/demo` path (PWA entry).
    pub fn demo_url(&self) -> String {
        format!("{}/demo", self.app_url.trim_end_matches('/'))
    }

    /// Reset link consumed by `frontend/index.html` (`?reset=TOKEN` on `/demo`).
    pub fn password_reset_url(&self, reset_token: &str) -> String {
        format!(
            "{}/demo?reset={}",
            self.app_url.trim_end_matches('/'),
            reset_token
        )
    }

    pub async fn send_registration_welcome(&self, to: &str, bonus_usdt: Option<rust_decimal::Decimal>) -> AppResult<()> {
        let name = to.split('@').next().unwrap_or("Nutzer");
        let bonus_line = bonus_usdt
            .filter(|a| *a > rust_decimal::Decimal::ZERO)
            .map(|a| {
                format!(
                    "<p style=\"background:#ECFDF5;border-radius:8px;padding:12px 16px\">\
                     <strong>Willkommensbonus:</strong> Du erhältst <strong>{a} USDT</strong> \
                     als Early Adopter — direkt auf dein Wallet gutgeschrieben.</p>"
                )
            })
            .unwrap_or_default();

        let mut vars = HashMap::new();
        vars.insert("name".into(), name.into());
        vars.insert("app_url".into(), self.demo_url());
        vars.insert("bonus_block".into(), bonus_line);

        let subject = "Willkommen bei VANTAGE-EARN — dein Konto ist bereit";
        let html = self.render_template("registration.html", &vars)?;
        self.deliver(to, subject, &html).await
    }

    pub async fn send_password_reset(&self, to: &str, reset_token: &str) -> AppResult<()> {
        let name = to.split('@').next().unwrap_or("Nutzer");
        let reset_url = self.password_reset_url(reset_token);
        let mut vars = HashMap::new();
        vars.insert("name".into(), name.into());
        vars.insert("reset_url".into(), reset_url);

        let subject = "VANTAGE-EARN — Passwort zurücksetzen";
        let html = self.render_template("password_reset.html", &vars)?;
        self.deliver(to, subject, &html).await
    }

    fn render_template(&self, file: &str, vars: &HashMap<String, String>) -> AppResult<String> {
        let path = self.resolve_template(file)?;
        let raw = std::fs::read_to_string(&path).map_err(|e| {
            shared::AppError::InvalidInput(format!("email template {}: {e}", path.display()))
        })?;
        let mut html = raw;
        for (key, value) in vars {
            html = html.replace(&format!("{{{{{key}}}}}"), value);
        }
        Ok(html)
    }

    fn resolve_template(&self, file: &str) -> AppResult<PathBuf> {
        let candidates = [
            self.templates_dir.join(file),
            PathBuf::from("templates/email").join(file),
            PathBuf::from("../templates/email").join(file),
        ];
        for path in candidates {
            if path.is_file() {
                return Ok(path);
            }
        }
        Err(shared::AppError::InvalidInput(format!(
            "email template not found: {file}"
        )))
    }

    async fn deliver(&self, to: &str, subject: &str, html: &str) -> AppResult<()> {
        let log_to = to.to_string();
        let log_subject = subject.to_string();

        if let Some(resend) = &self.resend {
            send_via_resend_api(resend, to, subject, html).await?;
            tracing::info!(to = %log_to, subject = %log_subject, "transactional email sent (Resend API)");
            return Ok(());
        }

        if crate::release_info::is_production() && self.smtp.is_some() {
            tracing::error!(
                to = %to,
                subject = %subject,
                "email blocked — SMTP must not be used in production; configure RESEND_API_KEY or SMTP_PASS (re_…)"
            );
            return Err(shared::AppError::InvalidInput(
                "email not configured for production (Resend API key required)".into(),
            ));
        }

        let Some(smtp) = &self.smtp else {
            if crate::release_info::is_production() {
                tracing::warn!(
                    to = %to,
                    subject = %subject,
                    "email skipped (no Resend API key configured in production)"
                );
            } else {
                tracing::info!(
                    to = %to,
                    subject = %subject,
                    "email (not configured — dev log only)\n{html}"
                );
            }
            return Ok(());
        };

        let smtp = smtp.clone();
        let to = to.to_string();
        let subject = subject.to_string();
        let html = html.to_string();

        tokio::task::spawn_blocking(move || send_smtp(&smtp, &to, &subject, &html))
            .await
            .map_err(|e| shared::AppError::InvalidInput(e.to_string()))??;
        tracing::info!(to = %log_to, subject = %log_subject, "transactional email sent (SMTP)");
        Ok(())
    }
}

async fn send_via_resend_api(cfg: &ResendConfig, to: &str, subject: &str, html: &str) -> AppResult<()> {
    #[derive(serde::Serialize)]
    struct ResendEmail<'a> {
        from: &'a str,
        to: Vec<&'a str>,
        subject: &'a str,
        html: &'a str,
    }

    let body = ResendEmail {
        from: &cfg.from,
        to: vec![to],
        subject,
        html,
    };

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.resend.com/emails")
        .bearer_auth(&cfg.api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| shared::AppError::InvalidInput(format!("Resend API request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        tracing::error!(
            to = %to,
            from = %cfg.from,
            subject = %subject,
            status = %status,
            resend_response = %text,
            "Resend API rejected transactional email"
        );
        return Err(shared::AppError::InvalidInput(format!(
            "Resend API error {status}: {text}"
        )));
    }
    Ok(())
}

fn send_smtp(cfg: &SmtpConfig, to: &str, subject: &str, html: &str) -> AppResult<()> {
    use lettre::message::header::ContentType;
    use lettre::message::Mailbox;
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::{Message, SmtpTransport, Transport};

    let from: Mailbox = cfg.from.parse().map_err(|e| {
        shared::AppError::InvalidInput(format!("invalid SMTP_FROM: {e}"))
    })?;
    let to_mailbox: Mailbox = to.parse().map_err(|e| {
        shared::AppError::InvalidInput(format!("invalid recipient: {e}"))
    })?;

    let message = Message::builder()
        .from(from)
        .to(to_mailbox)
        .subject(subject)
        .header(ContentType::TEXT_HTML)
        .body(html.to_string())
        .map_err(|e| shared::AppError::InvalidInput(e.to_string()))?;

    if cfg.port == 465 {
        SmtpTransport::relay(&cfg.host)
    } else {
        SmtpTransport::starttls_relay(&cfg.host)
    }
    .map_err(|e| shared::AppError::InvalidInput(format!("SMTP relay {e}")))?
    .port(cfg.port)
    .credentials(Credentials::new(cfg.user.clone(), cfg.pass.clone()))
    .build()
    .send(&message)
    .map_err(|e| shared::AppError::InvalidInput(format!("SMTP send failed: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service_with_app_url(app_url: &str) -> EmailService {
        let templates_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../templates/email");
        EmailService {
            templates_dir,
            app_url: app_url.into(),
            resend: None,
            smtp: None,
        }
    }

    #[test]
    fn password_reset_url_matches_frontend_query_param() {
        let svc = service_with_app_url("https://vantage-earn.onrender.com");
        let url = svc.password_reset_url("abc123token");
        assert_eq!(
            url,
            "https://vantage-earn.onrender.com/demo?reset=abc123token"
        );
    }

    #[test]
    fn password_reset_url_strips_trailing_slash_from_app_url() {
        let svc = service_with_app_url("https://example.com/");
        assert_eq!(
            svc.password_reset_url("tok"),
            "https://example.com/demo?reset=tok"
        );
    }

    #[test]
    fn demo_url_appends_demo_path() {
        let svc = service_with_app_url("https://vantage-earn.onrender.com");
        assert_eq!(svc.demo_url(), "https://vantage-earn.onrender.com/demo");
    }

    #[test]
    fn registration_template_renders_with_bonus_block() {
        let svc = service_with_app_url("https://example.com");
        let mut vars = HashMap::new();
        vars.insert("name".into(), "test".into());
        vars.insert("app_url".into(), svc.demo_url());
        vars.insert("bonus_block".into(), "<p>bonus</p>".into());
        let html = svc.render_template("registration.html", &vars).unwrap();
        assert!(html.contains("Willkommen bei VANTAGE-EARN"));
        assert!(html.contains("https://example.com/demo"));
        assert!(html.contains("bonus"));
        assert!(html.contains("Diese E-Mail kann nicht beantwortet werden"));
    }

    #[test]
    fn password_reset_template_renders_reset_link() {
        let svc = service_with_app_url("https://example.com");
        let reset_url = svc.password_reset_url("secret-token");
        let mut vars = HashMap::new();
        vars.insert("name".into(), "user".into());
        vars.insert("reset_url".into(), reset_url.clone());
        let html = svc.render_template("password_reset.html", &vars).unwrap();
        assert!(html.contains(&reset_url));
        assert!(html.contains("Passwort zurücksetzen"));
        assert!(html.contains("Diese E-Mail kann nicht beantwortet werden"));
    }

    #[test]
    fn suspicious_from_detects_placeholder_domains() {
        assert!(is_unverified_sender_domain("VANTAGE-EARN <noreply@deine-domain.de>"));
        assert!(is_unverified_sender_domain("noreply@vantage-earn.onrender.com"));
        assert!(!is_unverified_sender_domain("VANTAGE-EARN <noreply@myapp.de>"));
        assert!(!is_unverified_sender_domain("VANTAGE-EARN <onboarding@resend.dev>"));
    }

    #[test]
    fn looks_like_resend_api_key_requires_re_prefix() {
        assert!(looks_like_resend_api_key("re_abc123"));
        assert!(looks_like_resend_api_key("re_live_xyz"));
        assert!(looks_like_resend_api_key("\"re_quoted_key\""));
        assert!(looks_like_resend_api_key("  re_trimmed  "));
        assert!(!looks_like_resend_api_key("password"));
        assert!(!looks_like_resend_api_key("smtp-secret"));
        assert!(!looks_like_resend_api_key(""));
    }

    #[test]
    fn extract_resend_key_from_quoted_value() {
        assert_eq!(
            extract_resend_api_key("\"re_abc123\"").as_deref(),
            Some("re_abc123")
        );
    }

    #[test]
    fn smtp_blocked_in_production_even_with_host_set() {
        let prev_env = std::env::var("RUST_ENV").ok();
        let prev_host = std::env::var("SMTP_HOST").ok();
        let prev_user = std::env::var("SMTP_USER").ok();
        let prev_pass = std::env::var("SMTP_PASS").ok();
        let prev_resend = std::env::var("RESEND_API_KEY").ok();
        std::env::set_var("RUST_ENV", "production");
        std::env::remove_var("RESEND_API_KEY");
        std::env::set_var("SMTP_HOST", "smtp.resend.com");
        std::env::set_var("SMTP_USER", "resend");
        std::env::set_var("SMTP_PASS", "plain-smtp-password");
        assert!(smtp_from_env().is_none());
        restore_env("RUST_ENV", prev_env);
        restore_env("SMTP_HOST", prev_host);
        restore_env("SMTP_USER", prev_user);
        restore_env("SMTP_PASS", prev_pass);
        restore_env("RESEND_API_KEY", prev_resend);
    }

    #[test]
    fn resend_key_from_smtp_pass_when_re_prefix() {
        let prev_resend = std::env::var("RESEND_API_KEY").ok();
        let prev_pass = std::env::var("SMTP_PASS").ok();
        std::env::remove_var("RESEND_API_KEY");
        std::env::set_var("SMTP_PASS", "re_test_key_from_smtp_pass");
        assert_eq!(
            resolve_resend_api_key().as_deref(),
            Some("re_test_key_from_smtp_pass")
        );
        restore_env("RESEND_API_KEY", prev_resend);
        restore_env("SMTP_PASS", prev_pass);
    }

    #[test]
    fn resend_key_prefers_resend_api_key_env() {
        let prev_resend = std::env::var("RESEND_API_KEY").ok();
        let prev_pass = std::env::var("SMTP_PASS").ok();
        std::env::set_var("RESEND_API_KEY", "re_from_dedicated_env");
        std::env::set_var("SMTP_PASS", "re_from_smtp_pass");
        assert_eq!(
            resolve_resend_api_key().as_deref(),
            Some("re_from_dedicated_env")
        );
        restore_env("RESEND_API_KEY", prev_resend);
        restore_env("SMTP_PASS", prev_pass);
    }

    #[test]
    fn smtp_disabled_when_only_host_set() {
        let prev_host = std::env::var("SMTP_HOST").ok();
        let prev_user = std::env::var("SMTP_USER").ok();
        let prev_pass = std::env::var("SMTP_PASS").ok();
        let prev_resend = std::env::var("RESEND_API_KEY").ok();
        std::env::remove_var("RESEND_API_KEY");
        std::env::set_var("SMTP_HOST", "smtp.resend.com");
        std::env::remove_var("SMTP_USER");
        std::env::remove_var("SMTP_PASS");
        assert!(smtp_from_env().is_none());
        restore_env("SMTP_HOST", prev_host);
        restore_env("SMTP_USER", prev_user);
        restore_env("SMTP_PASS", prev_pass);
        restore_env("RESEND_API_KEY", prev_resend);
    }

    #[test]
    fn smtp_skipped_when_pass_is_resend_api_key() {
        let prev_host = std::env::var("SMTP_HOST").ok();
        let prev_user = std::env::var("SMTP_USER").ok();
        let prev_pass = std::env::var("SMTP_PASS").ok();
        let prev_resend = std::env::var("RESEND_API_KEY").ok();
        std::env::remove_var("RESEND_API_KEY");
        std::env::set_var("SMTP_HOST", "smtp.resend.com");
        std::env::set_var("SMTP_USER", "resend");
        std::env::set_var("SMTP_PASS", "re_api_key_not_smtp");
        assert!(smtp_from_env().is_none());
        assert!(resend_from_env().is_some());
        restore_env("SMTP_HOST", prev_host);
        restore_env("SMTP_USER", prev_user);
        restore_env("SMTP_PASS", prev_pass);
        restore_env("RESEND_API_KEY", prev_resend);
    }

    fn restore_env(key: &str, value: Option<String>) {
        match value {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }
}
