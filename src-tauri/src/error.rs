use std::fmt;

use serde::{Serialize, Serializer};

/// Error codes shared with the TypeScript `CommandError` contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    ValidationError,
    NotFound,
    Conflict,
    DatabaseError,
    InternalError,
    /// v1.1.2 E1: the on-disk database was written by a NEWER version of the
    /// app. The old program must refuse to open it (no writes at all) rather
    /// than risk corrupting a schema it does not understand.
    DatabaseTooNew,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::ValidationError => "VALIDATION_ERROR",
            ErrorCode::NotFound => "NOT_FOUND",
            ErrorCode::Conflict => "CONFLICT",
            ErrorCode::DatabaseError => "DATABASE_ERROR",
            ErrorCode::InternalError => "INTERNAL_ERROR",
            ErrorCode::DatabaseTooNew => "DATABASE_TOO_NEW",
        }
    }
}

impl Serialize for ErrorCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

/// Serialized to the frontend as `{ "code": "...", "message": "..." }`.
#[derive(Debug, Serialize)]
pub struct CommandError {
    pub code: ErrorCode,
    pub message: String,
}

impl CommandError {
    pub fn validation(message: impl Into<String>) -> Self {
        Self { code: ErrorCode::ValidationError, message: message.into() }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self { code: ErrorCode::NotFound, message: message.into() }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self { code: ErrorCode::Conflict, message: message.into() }
    }

    pub fn database(message: impl Into<String>) -> Self {
        Self { code: ErrorCode::DatabaseError, message: message.into() }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self { code: ErrorCode::InternalError, message: message.into() }
    }

    pub fn database_too_new(current: u32, supported: u32) -> Self {
        Self {
            code: ErrorCode::DatabaseTooNew,
            message: format!(
                "数据库由更新版本的 Abyssal Reverie 创建（库版本 {current}，本程序最高支持 {supported}）。\
                 请升级本应用后再启动；数据库文件未被修改。"
            ),
        }
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for CommandError {}

impl From<rusqlite::Error> for CommandError {
    fn from(err: rusqlite::Error) -> Self {
        CommandError::database(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_to_the_frontend_error_contract() {
        let err = CommandError::conflict("stale revision");
        let json = serde_json::to_value(&err).expect("serializable");

        assert_eq!(json["code"], "CONFLICT");
        assert_eq!(json["message"], "stale revision");
    }

    #[test]
    fn exposes_every_contract_code() {
        assert_eq!(ErrorCode::ValidationError.as_str(), "VALIDATION_ERROR");
        assert_eq!(ErrorCode::NotFound.as_str(), "NOT_FOUND");
        assert_eq!(ErrorCode::Conflict.as_str(), "CONFLICT");
        assert_eq!(ErrorCode::DatabaseError.as_str(), "DATABASE_ERROR");
        assert_eq!(ErrorCode::InternalError.as_str(), "INTERNAL_ERROR");
    }
}
