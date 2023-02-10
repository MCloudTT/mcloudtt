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
    #[cfg(feature = "bq_logging")]
    pub(crate) bigquery: BigQuery,
}

impl Configuration {
    pub(crate) fn load() -> Result<Self> {
        Ok(
            Config::builder()
                .add_source(config::File::with_name("config.toml"))
                .add_source(config::Environment::with_prefix("mcloudtt"))
                .build()?
                .try_deserialize()?,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_CONFIG: &'static str =
        r#"[general]
websocket = true

[tls]
certfile = "certs/broker/broker.crt"
keyfile = "certs/broker/broker.key"

[ports]
tcp = 1883
ws = 8080
"#;

    #[test]
    fn test_ports() {
        let config: Configuration =
            Config::builder()
                .add_source(config::File::from_str(TEST_CONFIG, config::FileFormat::Toml))
                .build()
                .unwrap()
                .try_deserialize()
                .unwrap();
        assert_eq!(config.ports.tcp, 1883);
        assert_eq!(config.ports.ws, 8080);
        assert_eq!(config.tls.certfile, "certs/broker/broker.crt");
        assert_eq!(config.tls.keyfile, "certs/broker/broker.key");
        assert!(config.general.websocket);
    }

    #[cfg(feature = "bq_logging")]
    #[test]
    fn test_bigquery() {
        let config =
            TEST_CONFIG.to_owned() +
                r#"
[bigquery]
project_id = "azubi-knowhow-building"
dataset_id = "mcloudttbq"
table_id = "topic-log"
credentials_path = "sa.key""#;
        let config: Configuration =
            Config::builder()
                .add_source(config::File::from_str(config.as_str(), config::FileFormat::Toml))
                .build()
                .unwrap()
                .try_deserialize()
                .unwrap();
        assert_eq!(config.bigquery.project_id, "azubi-knowhow-building");
        assert_eq!(config.bigquery.dataset_id, "mcloudttbq");
        assert_eq!(config.bigquery.table_id, "topic-log");
    }
}
