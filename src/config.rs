use crate::error::Result;
use config::Config;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct Ports {
    pub(crate) tcp: u16,
    pub(crate) ws: u16,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Tls {
    pub(crate) certfile: String,
    pub(crate) keyfile: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct General {
    pub(crate) websocket: bool,
    pub(crate) timeout: u16,
}

#[cfg(feature = "bq_logging")]
#[derive(Debug, Deserialize)]
pub(crate) struct BigQuery {
    pub(crate) project_id: String,
    pub(crate) dataset_id: String,
    pub(crate) table_id: String,
    pub(crate) credentials_path: String,
}

#[cfg(feature = "redis")]
#[derive(Debug, Deserialize)]
pub(crate) struct Redis {
    pub(crate) host: String,
    pub(crate) port: u16,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Configuration {
    pub(crate) ports: Ports,
    pub(crate) tls: Tls,
    pub(crate) general: General,
    #[cfg(feature = "bq_logging")]
    pub(crate) bigquery: BigQuery,
    #[cfg(feature = "redis")]
    pub(crate) redis: Redis,
}

impl Configuration {
    pub(crate) fn load() -> Result<Self> {
        Ok(Config::builder()
            .add_source(config::File::with_name("config.toml"))
            .add_source(config::Environment::with_prefix("mcloudtt"))
            .build()?
            .try_deserialize()?)
    }
}

#[cfg(test)]
mod tests {
    // TODO: Write tests
}
