use thiserror::Error;
pub(crate) type Result<T = ()> = std::result::Result<T, MCloudError>;
#[derive(Error, Debug)]
pub enum MCloudError {
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
    #[error("No receivers found")]
    NoReceiversFound,
    #[error("IO error: `{0}`")]
    IOError(std::io::Error),
    #[error("Configuration Error: `{0}`")]
    ConfigurationError(config::ConfigError),
}

macro_rules! impl_from {
    ($(($source: ty, $target: expr)),*) => {
        $(
        impl From<$source> for MCloudError{
            fn from(value: $source) -> Self{
                $target(value)
            }
        })*
    }
}

impl_from!(
    (std::io::Error, MCloudError::IOError),
    (config::ConfigError, MCloudError::ConfigurationError)
);
