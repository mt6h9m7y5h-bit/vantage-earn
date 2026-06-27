use std::collections::HashMap;
use std::path::PathBuf;

use shared::AppResult;

/// Transactional email (welcome, password reset, …).
///
/// SMTP (optional — all of host, user, pass required; port defaults to 587):
/// - `SMTP_HOST` — e.g. `smtp.resend.com`
/// - `SMTP_PORT` — e.g. `587` (default `587`; use `465` for implicit TLS)
/// - `SMTP_USER` / `SMTP_PASS` — credentials (`resend` + API key for Resend)
/// - `SMTP_FROM` — sender, e.g. `VANTAGE-EARN <onboarding@deine-domain.de>`
/// - `APP_URL` — origin for reset links (default `https://vantage-earn.onrender.com`)
/// - `EMAIL_TEMPLATES_DIR` — override template folder (default `templates/email`)
#[derive(Clone)]
pub struct EmailService {
    templates_dir: PathBuf,
    app_url: String,
    smtp: Option<SmtpConfig>,
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

fn smtp_from_env() -> Option<SmtpConfig> {
    let host = non_empty_env("SMTP_HOST")?;
    let port = non_empty_env("SMTP_PORT")
        .and_then(|p| p.parse().ok())
        .unwrap_or(587);
    let user = non_empty_env("SMTP_USER");
    let pass = non_empty_env("SMTP_PASS");
    let from = non_empty_env("SMTP_FROM")
        .unwrap_or_else(|| "VANTAGE-EARN <noreply@vantage-earn.onrender.com>".into());

    if user.is_none() || pass.is_none() {
        tracing::warn!(
            host = %host,
            "SMTP_HOST set but SMTP_USER/SMTP_PASS missing — transactional email disabled"
        );
        return None;
    }

    Some(SmtpConfig {
        host,
        port,
        user: user.unwrap(),
        pass: pass.unwrap(),
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
        let smtp = smtp_from_env();
        if smtp.is_some() {
            tracing::info!("transactional email: SMTP relay configured");
        }
        Self {
            templates_dir,
            app_url,
            smtp,
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
        let Some(smtp) = &self.smtp else {
            if crate::release_info::is_production() {
                tracing::info!(
                    to = %to,
                    subject = %subject,
                    "email skipped (SMTP not configured in production)"
                );
            } else {
                tracing::info!(
                    to = %to,
                    subject = %subject,
                    "email (SMTP not configured — dev log only)\n{html}"
                );
            }
            return Ok(());
        };

        let smtp = smtp.clone();
        let to = to.to_string();
        let subject = subject.to_string();
        let html = html.to_string();
        let log_to = to.clone();
        let log_subject = subject.clone();

        tokio::task::spawn_blocking(move || send_smtp(&smtp, &to, &subject, &html))
            .await
            .map_err(|e| shared::AppError::InvalidInput(e.to_string()))??;
        tracing::info!(to = %log_to, subject = %log_subject, "transactional email sent");
        Ok(())
    }
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
    }

    #[test]
    fn smtp_disabled_when_only_host_set() {
        let prev_host = std::env::var("SMTP_HOST").ok();
        let prev_user = std::env::var("SMTP_USER").ok();
        let prev_pass = std::env::var("SMTP_PASS").ok();
        std::env::set_var("SMTP_HOST", "smtp.resend.com");
        std::env::remove_var("SMTP_USER");
        std::env::remove_var("SMTP_PASS");
        assert!(smtp_from_env().is_none());
        restore_env("SMTP_HOST", prev_host);
        restore_env("SMTP_USER", prev_user);
        restore_env("SMTP_PASS", prev_pass);
    }

    fn restore_env(key: &str, value: Option<String>) {
        match value {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }
}
