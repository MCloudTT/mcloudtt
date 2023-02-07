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
}

#[derive(Debug, Deserialize)]
pub(crate) struct BigQuery {
    pub(crate) project_id: String,
    pub(crate) dataset_id: String,
    pub(crate) table_id: String,
    pub(crate) credentials_path: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Configuration {
    pub(crate) ports: Ports,
    pub(crate) tls: Tls,
    pub(crate) general: General,
    pub(crate) bigquery: BigQuery,
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
