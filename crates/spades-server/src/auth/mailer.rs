//! Pluggable Mailer trait. LogMailer (dev/CI) + SmtpMailer (lettre).

use crate::auth::AuthError;
use crate::lock_util::MutexExt;
use std::sync::{Arc, Mutex};
use tracing::{debug, error};

/// A single email message to send.
#[derive(Debug, Clone)]
pub struct Email {
    pub to: String,
    pub subject: String,
    pub body: String,
}

#[async_trait::async_trait]
pub trait Mailer: Send + Sync {
    async fn send(&self, email: Email) -> Result<(), AuthError>;
}

/// Mailer that records messages in memory instead of sending. For dev/CI.
#[derive(Clone, Default)]
pub struct LogMailer {
    sent: Arc<Mutex<Vec<Email>>>,
}

impl LogMailer {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn sent(&self) -> Vec<Email> {
        self.sent.lock_or_recover().clone()
    }
}

#[async_trait::async_trait]
impl Mailer for LogMailer {
    async fn send(&self, email: Email) -> Result<(), AuthError> {
        debug!(to = %email.to, subject = %email.subject, "LogMailer: would send email");
        self.sent.lock_or_recover().push(email);
        Ok(())
    }
}

use lettre::{
    message::{header::ContentType, Mailbox},
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};

#[derive(Clone)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub from: String,
    pub starttls: bool,
}

impl SmtpConfig {
    /// Build from env vars. Returns None if any required var is missing.
    pub fn from_env() -> Option<Self> {
        let host = std::env::var("SMTP_HOST").ok()?;
        let username = std::env::var("SMTP_USER").ok()?;
        let password = std::env::var("SMTP_PASS").ok()?;
        let from = std::env::var("SMTP_FROM").ok()?;
        let port: u16 = std::env::var("SMTP_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(587);
        let starttls = std::env::var("SMTP_STARTTLS")
            .map(|s| s != "false")
            .unwrap_or(true);
        Some(SmtpConfig {
            host,
            port,
            username,
            password,
            from,
            starttls,
        })
    }
}

pub struct SmtpMailer {
    cfg: SmtpConfig,
    transport: AsyncSmtpTransport<Tokio1Executor>,
}

impl SmtpMailer {
    pub fn new(cfg: SmtpConfig) -> Result<Self, AuthError> {
        let creds = Credentials::new(cfg.username.clone(), cfg.password.clone());
        let builder = if cfg.starttls {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&cfg.host)
                .map_err(|e| AuthError::Internal(format!("smtp init: {e}")))?
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::relay(&cfg.host)
                .map_err(|e| AuthError::Internal(format!("smtp init: {e}")))?
        };
        let transport = builder.credentials(creds).port(cfg.port).build();
        Ok(SmtpMailer { cfg, transport })
    }
}

#[async_trait::async_trait]
impl Mailer for SmtpMailer {
    async fn send(&self, email: Email) -> Result<(), AuthError> {
        let from: Mailbox = self
            .cfg
            .from
            .parse()
            .map_err(|e| AuthError::Internal(format!("invalid SMTP_FROM: {e}")))?;
        let to: Mailbox = email
            .to
            .parse()
            .map_err(|e| AuthError::Validation(format!("invalid email: {e}")))?;
        let msg = Message::builder()
            .from(from)
            .to(to)
            .subject(email.subject)
            .header(ContentType::TEXT_PLAIN)
            .body(email.body)
            .map_err(|e| AuthError::Internal(format!("message build: {e}")))?;
        self.transport.send(msg).await.map_err(|e| {
            error!(error = %e, "SmtpMailer send failed");
            AuthError::MailerFailed
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn log_mailer_records_messages() {
        let m = LogMailer::new();
        m.send(Email {
            to: "a@example.com".into(),
            subject: "Hello".into(),
            body: "Body".into(),
        })
        .await
        .unwrap();
        let sent = m.sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].to, "a@example.com");
        assert_eq!(sent[0].subject, "Hello");
    }

    #[test]
    fn smtp_config_from_env_missing_vars_returns_none() {
        for v in ["SMTP_HOST", "SMTP_USER", "SMTP_PASS", "SMTP_FROM"] {
            unsafe {
                std::env::remove_var(v);
            }
        }
        assert!(SmtpConfig::from_env().is_none());
    }
}
