//! Pluggable Mailer trait. LogMailer (dev/CI) + SmtpMailer (lettre).

use crate::auth::AuthError;
use std::sync::{Arc, Mutex};

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
        self.sent.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl Mailer for LogMailer {
    async fn send(&self, email: Email) -> Result<(), AuthError> {
        eprintln!("LogMailer: to={} subject={}", email.to, email.subject);
        self.sent.lock().unwrap().push(email);
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
}
