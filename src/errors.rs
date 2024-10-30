use crate::blocking::ToBlocking;
use actix_web::{http::StatusCode, ResponseError};
use derive_more::{Display, Error};

/// Errors our webpages can return
#[derive(Debug, Display, Error)]
pub enum PageError {
    #[display(fmt = "Couldn't find Page")]
    NotFound,
    #[display(fmt = "Internal server error")]
    InternalError,
    #[display(fmt = "Error handling multipart data")]
    MultipartError,
    #[display(fmt = "Resource doesn't exist")]
    ResourceNotFound,
    #[display(fmt = "File type is unsupported")]
    UnsupportedFileType,
    #[display(fmt = "Authentication error")]
    AuthError,
}

impl ResponseError for PageError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            PageError::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
            PageError::MultipartError => StatusCode::BAD_REQUEST,
            PageError::ResourceNotFound => StatusCode::BAD_REQUEST,
            PageError::UnsupportedFileType => StatusCode::BAD_REQUEST,
            PageError::AuthError => StatusCode::BAD_REQUEST,
        }
    }
}

impl From<tokio::sync::mpsc::error::SendError<ToBlocking>> for PageError {
    fn from(_: tokio::sync::mpsc::error::SendError<ToBlocking>) -> Self {
        PageError::InternalError
    }
}

impl From<actix_multipart::MultipartError> for PageError {
    fn from(_: actix_multipart::MultipartError) -> Self {
        PageError::MultipartError
    }
}
