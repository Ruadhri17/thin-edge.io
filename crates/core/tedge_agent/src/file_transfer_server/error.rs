use axum::extract::rejection::PathRejection;
use axum::response::IntoResponse;
use hyper::StatusCode;
use tedge_actors::RuntimeError;

use super::request_files::RequestPath;

#[derive(Debug, thiserror::Error)]
pub enum FileTransferError {
    #[error(transparent)]
    FromIo(#[from] std::io::Error),

    #[error(transparent)]
    FromHyperError(#[from] hyper::Error),

    #[error(transparent)]
    FromAddressParseError(#[from] std::net::AddrParseError),

    #[error(transparent)]
    FromUtf8Error(#[from] std::string::FromUtf8Error),

    #[error("Could not bind to address: {address}. Address already in use.")]
    BindingAddressInUse { address: std::net::SocketAddr },
}

#[derive(Debug, thiserror::Error)]
pub enum FileTransferRequestError {
    #[error(transparent)]
    FromIo(#[from] std::io::Error),

    #[error("Request to delete {path:?} failed: {err}")]
    DeleteIoError {
        #[source]
        err: std::io::Error,
        path: RequestPath,
    },

    #[error("Request to upload to {path:?} failed: {err:?}")]
    Upload {
        #[source]
        err: anyhow::Error,
        path: RequestPath,
    },

    #[error("Invalid file path: {path:?}")]
    InvalidPath { path: RequestPath },

    #[error("File not found: {0:?}")]
    FileNotFound(RequestPath),

    #[error("Path rejection: {0:?}")]
    PathRejection(#[from] PathRejection),
}

impl From<FileTransferError> for RuntimeError {
    fn from(error: FileTransferError) -> Self {
        RuntimeError::ActorError(Box::new(error))
    }
}

impl IntoResponse for FileTransferError {
    fn into_response(self) -> axum::response::Response {
        use FileTransferError::*;
        let status_code = match self {
            // TODO split out errors into startup and runtime errors
            FromIo(_)
            | FromHyperError(_)
            | FromAddressParseError(_)
            | FromUtf8Error(_)
            | BindingAddressInUse { .. } => {
                tracing::error!("{self}");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };
        status_code.into_response()
    }
}

impl IntoResponse for FileTransferRequestError {
    fn into_response(self) -> axum::response::Response {
        use FileTransferRequestError::*;
        match &self {
            FromIo(_) | PathRejection(_) => {
                tracing::error!("{self}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal error".to_owned(),
                )
            }
            DeleteIoError { path, .. } => {
                tracing::error!("{self}");
                (
                    // TODO do we really want to respond with forbidden for these errors?
                    StatusCode::FORBIDDEN,
                    format!("Cannot delete path {path:?}"),
                )
            }
            Upload { path, .. } => {
                tracing::error!("{self}");
                (
                    // TODO do we really want to respond with forbidden for these errors?
                    StatusCode::FORBIDDEN,
                    format!("Cannot upload to path {path:?}"),
                )
            }
            InvalidPath { .. } | FileNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
        }
        .into_response()
    }
}
