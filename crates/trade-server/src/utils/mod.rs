use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;

pub mod token_decimal_cache;

#[cfg(test)]
mod token_decimal_cache_test;

pub fn serialize_duration_as_secs<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let seconds_str = format!("{}s", duration.num_seconds());
    serializer.serialize_str(&seconds_str)
}

pub fn deserialize_timestamp<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let timestamp: i64 = Deserialize::deserialize(deserializer)?;
    let secs = timestamp / 1000;
    let nsecs = ((timestamp % 1000) * 1_000_000) as u32;
    DateTime::from_timestamp(secs, nsecs).ok_or(Error::custom("invalid timestamp"))
}
