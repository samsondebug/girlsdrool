//! The M5 forecast exactly as fixtures/forecast.json states it, for the acceptance tests.

use std::collections::HashMap;

#[derive(serde::Deserialize)]
pub struct ForecastFile {
    pub as_of: String,
    pub horizon_days: i64,
    pub bucket_days: i64,
    pub pay_shift_days: i64,
    pub timing_buffer_cents: i64,
    pub model: Vec<ModelSpec>,
    pub model_total_cents: i64,
    pub scenarios: HashMap<String, ScenarioSpec>,
}

#[derive(serde::Deserialize)]
pub struct ModelSpec {
    pub category: String,
    pub buckets: Vec<BucketSpec>,
    pub median_cents: i64,
}

#[derive(serde::Deserialize)]
pub struct BucketSpec {
    pub start: String,
    pub end: String,
    pub net_outflow_cents: i64,
}

#[derive(serde::Deserialize)]
pub struct ScenarioSpec {
    pub downside: bool,
    pub surprise: Option<PointSpec>,
    pub opening_cents: i64,
    pub inflows_cents: i64,
    pub outflows_cents: i64,
    pub closing_cents: i64,
    pub lowest: PointSpec,
    pub first_shortfall: Option<PointSpec>,
    pub first_buffer_breach: Option<PointSpec>,
    pub pay_dates: HashMap<String, Vec<String>>,
    pub days: Vec<DaySpec>,
    pub weeks: Vec<WeekSpec>,
}

#[derive(serde::Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct PointSpec {
    pub date: String,
    pub cents: i64,
}

#[derive(serde::Deserialize)]
pub struct DaySpec {
    pub day: i64,
    pub date: String,
    pub inflows_cents: i64,
    pub outflows_cents: i64,
    pub closing_cents: i64,
    pub committed_cents: i64,
    pub headroom_cents: i64,
    pub events: Vec<EventSpec>,
}

#[derive(serde::Deserialize)]
pub struct EventSpec {
    pub kind: String,
    pub name: String,
    pub cents: i64,
}

#[derive(serde::Deserialize)]
pub struct WeekSpec {
    pub week: i64,
    pub start: String,
    pub end: String,
    pub inflows_cents: i64,
    pub outflows_cents: i64,
    pub closing_cents: i64,
    pub lowest_cents: i64,
}
