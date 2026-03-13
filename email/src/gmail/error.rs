use std::{any::Any, result};

use thiserror::Error;

use crate::{AnyBoxedError, AnyError};

pub type Result<T> = result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("cannot send Gmail API request to {1}")]
    ApiRequestError(#[source] http::Error, String),
    #[error("Gmail API request failed with status {status}: {message}")]
    ApiResponseError { status: u16, message: String },
    #[error("cannot parse Gmail API response")]
    ParseResponseError(#[source] serde_json::Error),
    #[error("cannot read Gmail API response body")]
    ReadResponseError(#[source] http::Error),
    #[error("cannot decode Gmail raw message")]
    Base64DecodeError(#[source] base64::DecodeError),
    #[error("cannot refresh Gmail access token")]
    RefreshAccessTokenError(#[source] crate::account::Error),
    #[error("cannot get Gmail access token")]
    GetAccessTokenError(#[source] crate::account::Error),
    #[error("cannot reset Gmail OAuth secrets")]
    ResetOAuthSecretsError(#[source] crate::account::Error),
    #[error("cannot configure Gmail OAuth")]
    ConfiguringOAuthError(#[source] crate::account::Error),
    #[error("cannot replace Gmail secret with keyring entry")]
    ReplacingKeyringFailed(#[source] secret::Error),
    #[error("cannot resolve Gmail label {0}")]
    ResolveLabelError(String),
    #[error("cannot find Gmail message {0}")]
    MessageNotFoundError(String),
    #[error("cannot list Gmail envelopes: page {0} out of bounds")]
    PageOutOfBoundsError(usize),
    #[error("cannot parse Gmail batch response: {0}")]
    ParseBatchResponseError(String),
    #[error("missing Gmail OAuth client id")]
    MissingClientIdError,
}

impl AnyError for Error {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl From<Error> for AnyBoxedError {
    fn from(err: Error) -> Self {
        Box::new(err)
    }
}
