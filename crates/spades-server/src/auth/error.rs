//! AuthError type and HTTP response mapping.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("unauthenticated")]
    Unauthenticated,
    #[error("forbidden")]
    Forbidden,
    #[error("username_taken")]
    UsernameTaken,
    #[error("email_taken")]
    EmailTaken,
    #[error("invalid_credentials")]
    InvalidCredentials,
    #[error("locked")]
    Locked { retry_after_secs: u64 },
    #[error("rate_limited")]
    RateLimited { retry_after_secs: u64 },
    #[error("token_invalid")]
    TokenInvalid,
    #[error("oauth_failed: {0}")]
    OauthFailed(String),
    #[error("validation: {0}")]
    Validation(String),
    #[error("not_found")]
    NotFound,
    #[error("mailer_failed")]
    MailerFailed,
    #[error("storage: {0}")]
    Storage(String),
    #[error("internal: {0}")]
    Internal(String),
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    error: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    retry_after_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<String>,
}

impl AuthError {
    pub fn status(&self) -> StatusCode {
        match self {
            AuthError::Unauthenticated | AuthError::InvalidCredentials => StatusCode::UNAUTHORIZED,
            AuthError::Forbidden => StatusCode::FORBIDDEN,
            AuthError::UsernameTaken | AuthError::EmailTaken => StatusCode::CONFLICT,
            AuthError::NotFound => StatusCode::NOT_FOUND,
            AuthError::Locked { .. } => StatusCode::LOCKED,
            AuthError::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
            AuthError::TokenInvalid => StatusCode::GONE,
            AuthError::OauthFailed(_) => StatusCode::BAD_REQUEST,
            AuthError::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            AuthError::MailerFailed => StatusCode::BAD_GATEWAY,
            AuthError::Storage(_) | AuthError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn body(&self) -> ErrorBody<'_> {
        let retry_after_secs = match self {
            AuthError::Locked { retry_after_secs }
            | AuthError::RateLimited { retry_after_secs } => Some(*retry_after_secs),
            _ => None,
        };
        let details = match self {
            AuthError::OauthFailed(s) | AuthError::Validation(s) => Some(s.clone()),
            _ => None,
        };
        ErrorBody {
            error: error_code(self),
            retry_after_secs,
            details,
        }
    }
}

fn error_code(e: &AuthError) -> &'static str {
    match e {
        AuthError::Unauthenticated => "unauthenticated",
        AuthError::Forbidden => "forbidden",
        AuthError::UsernameTaken => "username_taken",
        AuthError::EmailTaken => "email_taken",
        AuthError::InvalidCredentials => "invalid_credentials",
        AuthError::Locked { .. } => "locked",
        AuthError::RateLimited { .. } => "rate_limited",
        AuthError::NotFound => "not_found",
        AuthError::TokenInvalid => "token_invalid",
        AuthError::OauthFailed(_) => "oauth_failed",
        AuthError::Validation(_) => "validation",
        AuthError::MailerFailed => "mailer_failed",
        AuthError::Storage(_) => "internal",
        AuthError::Internal(_) => "internal",
    }
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let status = self.status();
        let mut resp = (status, Json(self.body())).into_response();
        if let AuthError::RateLimited { retry_after_secs }
        | AuthError::Locked { retry_after_secs } = &self
        {
            if let Ok(hv) = retry_after_secs.to_string().parse() {
                resp.headers_mut()
                    .insert(axum::http::header::RETRY_AFTER, hv);
            }
        }
        resp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_codes_match_spec() {
        assert_eq!(
            AuthError::Unauthenticated.status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(AuthError::Forbidden.status(), StatusCode::FORBIDDEN);
        assert_eq!(AuthError::UsernameTaken.status(), StatusCode::CONFLICT);
        assert_eq!(AuthError::EmailTaken.status(), StatusCode::CONFLICT);
        assert_eq!(
            AuthError::InvalidCredentials.status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            AuthError::Locked {
                retry_after_secs: 30
            }
            .status(),
            StatusCode::LOCKED
        );
        assert_eq!(
            AuthError::RateLimited {
                retry_after_secs: 30
            }
            .status(),
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(AuthError::TokenInvalid.status(), StatusCode::GONE);
        assert_eq!(
            AuthError::OauthFailed("x".into()).status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            AuthError::Validation("x".into()).status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(AuthError::MailerFailed.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(
            AuthError::Storage("x".into()).status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn error_codes_stable() {
        assert_eq!(error_code(&AuthError::Unauthenticated), "unauthenticated");
        assert_eq!(error_code(&AuthError::UsernameTaken), "username_taken");
        assert_eq!(
            error_code(&AuthError::Locked {
                retry_after_secs: 0
            }),
            "locked"
        );
        assert_eq!(error_code(&AuthError::Storage("x".into())), "internal");
    }
}
