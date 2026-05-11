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

/// Wraps a backing `Mailer` with an unbounded mpsc queue. `send` enqueues
/// and returns `Ok` immediately so the HTTP handler doesn't await SMTP
/// latency; a background task drains the queue and forwards each `Email`
/// to the backing mailer, logging failures.
///
/// This is "best-effort" delivery — if the backing mailer fails or the
/// process crashes, in-flight emails are lost. Both verify-email and
/// password-reset flows are recoverable by re-requesting on the user side.
pub struct MailerQueue {
    sender: tokio::sync::mpsc::UnboundedSender<Email>,
}

impl MailerQueue {
    /// Wrap `backing` with a queue. Spawns a tokio task on the current
    /// runtime that runs until the last `MailerQueue` clone is dropped.
    pub fn new(backing: Arc<dyn Mailer>) -> Self {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<Email>();
        tokio::spawn(async move {
            while let Some(email) = receiver.recv().await {
                let to = email.to.clone();
                let subject = email.subject.clone();
                if let Err(e) = backing.send(email).await {
                    error!(to = %to, subject = %subject, error = %e, "mailer queue: send failed");
                }
            }
        });
        Self { sender }
    }
}

#[async_trait::async_trait]
impl Mailer for MailerQueue {
    async fn send(&self, email: Email) -> Result<(), AuthError> {
        self.sender.send(email).map_err(|_| {
            AuthError::Internal("mailer queue closed".into())
        })?;
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

    #[tokio::test]
    async fn mailer_queue_forwards_enqueued_emails_to_backing() {
        // LogMailer wraps an Arc<Mutex<Vec<Email>>>; clones share state, so
        // we can hold one handle to check what the queue forwarded.
        let backing = Arc::new(LogMailer::new());
        let queue = MailerQueue::new(backing.clone() as Arc<dyn Mailer>);

        for i in 0..3 {
            queue
                .send(Email {
                    to: format!("u{i}@example.com"),
                    subject: "subj".into(),
                    body: "body".into(),
                })
                .await
                .expect("enqueue succeeds");
        }

        // The drain runs on a background task; poll briefly for it to catch
        // up rather than picking an arbitrary sleep.
        for _ in 0..100 {
            if backing.sent().len() >= 3 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }

        let sent = backing.sent();
        assert_eq!(sent.len(), 3, "all enqueued emails reach the backing mailer");
        assert_eq!(sent[0].to, "u0@example.com");
        assert_eq!(sent[2].to, "u2@example.com");
    }

    #[tokio::test]
    async fn mailer_queue_send_returns_ok_without_awaiting_backing() {
        // Build a queue whose backing mailer would deadlock if awaited
        // synchronously — the queue must return Ok immediately rather than
        // await the underlying send.
        struct SlowMailer;
        #[async_trait::async_trait]
        impl Mailer for SlowMailer {
            async fn send(&self, _email: Email) -> Result<(), AuthError> {
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                Ok(())
            }
        }
        let queue = MailerQueue::new(Arc::new(SlowMailer) as Arc<dyn Mailer>);
        let start = std::time::Instant::now();
        queue
            .send(Email {
                to: "slow@example.com".into(),
                subject: "s".into(),
                body: "b".into(),
            })
            .await
            .expect("enqueue succeeds");
        assert!(
            start.elapsed() < std::time::Duration::from_millis(100),
            "send must not await the backing mailer (took {:?})",
            start.elapsed(),
        );
    }
}
