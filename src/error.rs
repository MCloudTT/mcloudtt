use std::ops::FromResidual;

use thiserror::Error;
pub(crate) type Result<T = ()> = std::result::Result<T, MCloudError>;
#[derive(Error, Debug)]
pub(crate) enum MCloudError {
    #[error("Topic `{0}` already exists")]
    TopicAlreadyExists(String),
    #[error("Client `{0}` disconncted")]
    ClientDisconnected(String),
    #[error("Client `{0}` disconncted unexpectedly")]
    UnexpectedClientDisconnected(String),
    #[error("Client error: `{0}`")]
    ClientError(String),
    #[error("Unknown packet type")]
    UnknownPacketType,
    #[error("Could not write to stream because of `{0}`")]
    CouldNotWriteToStream(String),
}

impl FromResidual<std::result::Result<Infallible, std::io::Error>> for MCloudError {
    fn from_residual(residual: std::result::Result<Infallible, std::io::Error>) -> Self {
        
    }
}