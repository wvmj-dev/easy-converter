use serde::{Serialize, Serializer};
use ts_rs::TS;

/// Error type returned by every command and operation.
///
/// It is serialized to the frontend as an [`ErrorPayload`], so the UI can pick a
/// translated message from `code` and show `message` as technical detail.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("invalid path: {0}")]
    InvalidPath(String),
    #[error("file not found: {0}")]
    NotFound(String),
    #[error("invalid parameter: {0}")]
    InvalidParameter(String),
    /// The file is damaged or its content is not in a supported format.
    #[error("invalid or unsupported file: {0}")]
    InvalidFile(String),
    /// The input or the requested output exceeds the size limits that protect memory.
    #[error("too large: {0}")]
    TooLarge(String),
    /// The PDF is protected and the password is missing or wrong.
    #[error("password required or incorrect: {0}")]
    Password(String),
    #[error("tool not available: {0}")]
    ToolMissing(String),
    #[error("external process failed: {0}")]
    ProcessFailed(String),
    #[error("operation timed out")]
    Timeout,
    #[error("operation cancelled")]
    Cancelled,
    #[error("job not found: {0}")]
    JobNotFound(u64),
    #[error("action not allowed: {0}")]
    NotAllowed(String),
    #[error("settings error: {0}")]
    Settings(String),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ErrorCode {
    InvalidPath,
    NotFound,
    InvalidParameter,
    InvalidFile,
    TooLarge,
    Password,
    ToolMissing,
    ProcessFailed,
    Timeout,
    Cancelled,
    JobNotFound,
    NotAllowed,
    Settings,
    Io,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ErrorPayload {
    pub code: ErrorCode,
    /// Full English message, for logs.
    pub message: String,
    /// What went wrong, without the generic part the interface already translates: a
    /// path, a tool name or the tool's own explanation.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub detail: Option<String>,
}

impl AppError {
    pub fn code(&self) -> ErrorCode {
        match self {
            AppError::InvalidPath(_) => ErrorCode::InvalidPath,
            AppError::NotFound(_) => ErrorCode::NotFound,
            AppError::InvalidParameter(_) => ErrorCode::InvalidParameter,
            AppError::InvalidFile(_) => ErrorCode::InvalidFile,
            AppError::TooLarge(_) => ErrorCode::TooLarge,
            AppError::Password(_) => ErrorCode::Password,
            AppError::ToolMissing(_) => ErrorCode::ToolMissing,
            AppError::ProcessFailed(_) => ErrorCode::ProcessFailed,
            AppError::Timeout => ErrorCode::Timeout,
            AppError::Cancelled => ErrorCode::Cancelled,
            AppError::JobNotFound(_) => ErrorCode::JobNotFound,
            AppError::NotAllowed(_) => ErrorCode::NotAllowed,
            AppError::Settings(_) => ErrorCode::Settings,
            AppError::Io(_) => ErrorCode::Io,
            AppError::Internal(_) => ErrorCode::Internal,
        }
    }

    /// The specific part of the message, when there is one.
    pub fn detail(&self) -> Option<String> {
        let detail = match self {
            AppError::InvalidPath(d)
            | AppError::NotFound(d)
            | AppError::InvalidParameter(d)
            | AppError::InvalidFile(d)
            | AppError::TooLarge(d)
            | AppError::Password(d)
            | AppError::ToolMissing(d)
            | AppError::ProcessFailed(d)
            | AppError::NotAllowed(d)
            | AppError::Settings(d)
            | AppError::Internal(d) => d.clone(),
            AppError::Io(err) => err.to_string(),
            AppError::JobNotFound(id) => id.to_string(),
            AppError::Timeout | AppError::Cancelled => return None,
        };
        let detail = detail.trim();
        (!detail.is_empty()).then(|| detail.to_owned())
    }

    pub fn payload(&self) -> ErrorPayload {
        ErrorPayload {
            code: self.code(),
            message: self.to_string(),
            detail: self.detail(),
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.payload().serialize(serializer)
    }
}

impl From<tauri::Error> for AppError {
    fn from(err: tauri::Error) -> Self {
        AppError::Internal(err.to_string())
    }
}

impl From<tauri_plugin_store::Error> for AppError {
    fn from(err: tauri_plugin_store::Error) -> Self {
        AppError::Settings(err.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_as_code_and_message() {
        let json = serde_json::to_value(AppError::InvalidParameter("quality".into())).unwrap();
        assert_eq!(json["code"], "invalidParameter");
        assert_eq!(json["message"], "invalid parameter: quality");
        assert_eq!(json["detail"], "quality");

        let json = serde_json::to_value(AppError::Timeout).unwrap();
        assert_eq!(json["code"], "timeout");
        assert!(json.get("detail").is_none());
    }
}
