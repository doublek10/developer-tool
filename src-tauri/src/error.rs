//! Structured errors. Spec §23: every operation returns a code, a human message,
//! optional detail and a recovery hint that the UI can render as actionable text.

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct AppError {
    pub status: &'static str,
    pub code: String,
    pub message: String,
    pub details: Option<String>,
    pub recovery: Option<String>,
}

impl AppError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            status: "error",
            code: code.to_string(),
            message: message.into(),
            details: None,
            recovery: None,
        }
    }

    pub fn detail(mut self, d: impl Into<String>) -> Self {
        self.details = Some(d.into());
        self
    }

    pub fn recover(mut self, r: impl Into<String>) -> Self {
        self.recovery = Some(r.into());
        self
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {}

pub type AppResult<T> = std::result::Result<T, AppError>;

// ---- conversions from the libraries we use, so `?` works everywhere ----

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::new("LOCAL_DATABASE_ERROR", "The local application database failed.")
            .detail(e.to_string())
            .recover("Restart DevWorkstation. If this persists, open Settings and run Repair local database.")
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::new("FILE_SYSTEM_ERROR", "A file operation failed.")
            .detail(e.to_string())
            .recover("Check that the path still exists and that you have permission to access it.")
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::new("DATA_FORMAT_ERROR", "Stored data could not be read.")
            .detail(e.to_string())
    }
}

impl From<keyring::Error> for AppError {
    fn from(e: keyring::Error) -> Self {
        AppError::new("VAULT_ERROR", "The secure credential store is unavailable.")
            .detail(e.to_string())
            .recover("Sign in to Windows with your normal account, then try again.")
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        AppError::new("NETWORK_ERROR", "The network request failed.")
            .detail(e.to_string())
            .recover("Check your internet connection and the address you entered.")
    }
}
