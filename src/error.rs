use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Config,
    Connection,
    Query,
    Internal,
}

impl ErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorKind::Config => "Config",
            ErrorKind::Connection => "Connection",
            ErrorKind::Query => "Query",
            ErrorKind::Internal => "Internal",
        }
    }
}

#[derive(Debug)]
pub struct AppError {
    pub kind: ErrorKind,
    pub message: String,
}

impl AppError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for AppError {}

pub fn classify_error(err: &anyhow::Error) -> ErrorKind {
    if let Some(app) = err.downcast_ref::<AppError>() {
        return app.kind;
    }
    ErrorKind::Internal
}
