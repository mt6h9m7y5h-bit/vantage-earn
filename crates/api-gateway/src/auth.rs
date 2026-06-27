use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use shared::AppError;

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: usize,
    iat: usize,
}

#[derive(Clone)]
pub struct JwtService {
    encoding: EncodingKey,
    decoding: DecodingKey,
}

pub fn normalize_email(email: &str) -> Option<String> {
    let trimmed = email.trim().to_lowercase();
    if trimmed.is_empty() || !trimmed.contains('@') || trimmed.len() > 254 {
        return None;
    }
    Some(trimmed)
}

pub fn validate_password(password: &str) -> Result<(), AppError> {
    if password.len() < 8 {
        return Err(AppError::InvalidInput(
            "password must be at least 8 characters".into(),
        ));
    }
    if password.len() > 128 {
        return Err(AppError::InvalidInput("password too long".into()));
    }
    Ok(())
}

pub fn hash_password(password: &str) -> Result<String, AppError> {
    use argon2::password_hash::{PasswordHasher, SaltString};
    use argon2::Argon2;
    use rand::rngs::OsRng;

    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| AppError::InvalidInput(e.to_string()))?;
    Ok(hash.to_string())
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    use argon2::password_hash::{PasswordHash, PasswordVerifier};
    use argon2::Argon2;

    PasswordHash::new(hash)
        .ok()
        .is_some_and(|parsed| {
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        })
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn jwt_secret_from_env() -> String {
    if let Some(secret) = non_empty_env("JWT_SECRET") {
        return secret;
    }
    if let Some(secret) = non_empty_env("ADMIN_SECRET") {
        tracing::warn!(
            "JWT_SECRET not set — using ADMIN_SECRET for JWT signing; set JWT_SECRET explicitly in production"
        );
        return secret;
    }
    if std::env::var("RUST_ENV").as_deref() == Ok("production") {
        tracing::error!(
            "JWT_SECRET (or ADMIN_SECRET) must be set in production — check Render Dashboard → Environment"
        );
        std::process::exit(1);
    }
    tracing::warn!("JWT_SECRET not set — using insecure dev default");
    "dev-secret-change-in-production".into()
}

impl JwtService {
    pub fn from_env() -> Self {
        let secret = jwt_secret_from_env();
        Self {
            encoding: EncodingKey::from_secret(secret.as_bytes()),
            decoding: DecodingKey::from_secret(secret.as_bytes()),
        }
    }

    pub fn issue(&self, user_id: Uuid) -> Result<String, AppError> {
        let now = chrono::Utc::now().timestamp() as usize;
        let exp = now + 7 * 24 * 3600;
        encode(
            &Header::default(),
            &Claims {
                sub: user_id.to_string(),
                exp,
                iat: now,
            },
            &self.encoding,
        )
        .map_err(|e| AppError::InvalidInput(e.to_string()))
    }

    pub fn verify(&self, token: &str) -> Result<Uuid, AppError> {
        let data = decode::<Claims>(
            token,
            &self.decoding,
            &Validation::default(),
        )
        .map_err(|_| AppError::Unauthorized)?;

        Uuid::parse_str(&data.claims.sub).map_err(|_| AppError::Unauthorized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_falls_back_to_admin_secret_for_jwt() {
        let prior_jwt = std::env::var("JWT_SECRET").ok();
        let prior_admin = std::env::var("ADMIN_SECRET").ok();
        let prior_env = std::env::var("RUST_ENV").ok();

        std::env::remove_var("JWT_SECRET");
        std::env::set_var("ADMIN_SECRET", "render-generated-admin-secret");
        std::env::set_var("RUST_ENV", "production");

        let svc = JwtService::from_env();
        let user_id = Uuid::new_v4();
        let token = svc.issue(user_id).expect("issue with admin fallback");
        assert_eq!(svc.verify(&token).expect("verify with admin fallback"), user_id);

        match prior_jwt {
            Some(v) => std::env::set_var("JWT_SECRET", v),
            None => std::env::remove_var("JWT_SECRET"),
        }
        match prior_admin {
            Some(v) => std::env::set_var("ADMIN_SECRET", v),
            None => std::env::remove_var("ADMIN_SECRET"),
        }
        match prior_env {
            Some(v) => std::env::set_var("RUST_ENV", v),
            None => std::env::remove_var("RUST_ENV"),
        }
    }
}
