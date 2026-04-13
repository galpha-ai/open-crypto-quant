use clickhouse::Row;
use serde::Serialize;

#[derive(Debug, Clone, Row, Serialize)]
pub struct ChRow {
    #[serde(with = "clickhouse::serde::time::datetime")]
    pub ts: time::OffsetDateTime,
    pub queue: String,
    pub json: String,
}