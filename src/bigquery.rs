use gcp_bigquery_client::{
    model::table_data_insert_all_request::TableDataInsertAllRequest, Client,
};
use serde::Serialize;
use tracing::{error, info};

#[derive(Debug, Serialize)]
struct LogEntry {
    topic: String,
    message: String,
    datetime: String,
}

pub async fn log_in_bq(topic: String, message: String) {
    info!("Loggin in BQ: {0} in {1}", &message, &topic);

    let log_entry = LogEntry {
        topic,
        message,
        datetime: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
    };

    let client = Client::from_service_account_key_file("sa.key")
        .await
        .unwrap();
    let mut request = TableDataInsertAllRequest::new();
    let _ = request.add_row(None, log_entry);

    match client
        .tabledata()
        .insert_all("azubi-knowhow-building", "mcloudttbq", "topic-log", request)
        .await
    {
        Ok(_) => {}
        Err(e) => error!("Error logging in BQ: {0}", e),
    }
}
