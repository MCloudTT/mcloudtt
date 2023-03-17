use thiserror::Error;
use tokio::sync::mpsc::error::SendError;
use tracing_subscriber::filter::ParseError;

pub(crate) type Result<T = ()> = std::result::Result<T, MCloudError>;
type AnythingReally = Box<dyn std::error::Error + Send + Sync + 'static>;
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
    #[error("Decoding Packet Failed: `{0:?}`")]
    DecodePacketError(mqtt_v5_fork::types::DecodeError),
    #[error("Tracing initializer Error: `{0}`")]
    TracingInitializerError(ParseError),
    #[error("Error in mpsc channel: `{0}`")]
    MpscError(tokio::sync::mpsc::error::SendError<AnythingReally>),
    #[cfg(feature = "redis")]
    #[error("Error communicating with Redis: `{0}`")]
    RedisError(redis::RedisError),
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
    (config::ConfigError, MCloudError::ConfigurationError),
    (
        tracing_subscriber::filter::ParseError,
        MCloudError::TracingInitializerError
    )
);

impl From<tokio::sync::mpsc::error::SendError<String>> for MCloudError {
    fn from(value: SendError<String>) -> Self {
        MCloudError::MpscError(tokio::sync::mpsc::error::SendError(Box::new(value)))
    }
}
#[cfg(feature = "redis")]
impl_from!((redis::RedisError, MCloudError::RedisError));
