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

impl JwtService {
    pub fn from_env() -> Self {
        let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| {
            if std::env::var("RUST_ENV").as_deref() == Ok("production") {
                panic!("JWT_SECRET must be set in production");
            }
            tracing::warn!("JWT_SECRET not set — using insecure dev default");
            "dev-secret-change-in-production".into()
        });
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
