use std::collections::HashMap;
use std::path::PathBuf;

use shared::AppResult;

/// Transactional email (welcome, password reset, …).
///
/// SMTP (optional — without these vars, rendered HTML is logged at INFO for dev):
/// - `SMTP_HOST` — e.g. `smtp.sendgrid.net`
/// - `SMTP_PORT` — e.g. `587` (default `587`)
/// - `SMTP_USER` / `SMTP_PASS` — credentials (optional for local relay)
/// - `SMTP_FROM` — sender address, e.g. `VANTAGE-EARN <noreply@example.com>`
/// - `APP_URL` — link target in templates (default `https://vantage-earn.onrender.com`)
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
    user: Option<String>,
    pass: Option<String>,
    from: String,
}

impl EmailService {
    pub fn from_env() -> Self {
        let templates_dir = std::env::var("EMAIL_TEMPLATES_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("templates/email"));
        let app_url = std::env::var("APP_URL")
            .unwrap_or_else(|_| "https://vantage-earn.onrender.com".into());
        let smtp = std::env::var("SMTP_HOST").ok().map(|host| SmtpConfig {
            host,
            port: std::env::var("SMTP_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(587),
            user: std::env::var("SMTP_USER").ok(),
            pass: std::env::var("SMTP_PASS").ok(),
            from: std::env::var("SMTP_FROM")
                .unwrap_or_else(|_| "VANTAGE-EARN <noreply@vantage-earn.onrender.com>".into()),
        });
        Self {
            templates_dir,
            app_url,
            smtp,
        }
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
        vars.insert("app_url".into(), self.app_url.clone());
        vars.insert("bonus_block".into(), bonus_line);

        let subject = "Willkommen bei VANTAGE-EARN — dein Konto ist bereit";
        let html = self.render_template("registration.html", &vars)?;
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
        tracing::info!(to = %log_to, subject = %log_subject, "registration email sent");
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

    let mut builder = SmtpTransport::relay(&cfg.host)
        .map_err(|e| shared::AppError::InvalidInput(e.to_string()))?
        .port(cfg.port);

    if let (Some(user), Some(pass)) = (&cfg.user, &cfg.pass) {
        builder = builder.credentials(Credentials::new(user.clone(), pass.clone()));
    }

    builder
        .build()
        .send(&message)
        .map_err(|e| shared::AppError::InvalidInput(e.to_string()))?;
    Ok(())
}
