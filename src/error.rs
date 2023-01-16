use thiserror::Error;
pub(crate) type Result<T = ()> = std::result::Result<T, MCloudError>;
#[derive(Error, Debug)]
pub(crate) enum MCloudError {
    #[error("Topic `{0}` already exists")]
    TopicAlreadyExists(String),
}
