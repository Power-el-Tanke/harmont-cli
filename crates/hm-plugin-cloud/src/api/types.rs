//! Wire types for the API endpoints the cloud plugin consumes.
//!
//! Hand-written from the API's OpenAPI spec for the specific paths
//! we hit. Field names match the canonical source for the live API.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─── Auth ──────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub(crate) struct CliExchangeRequest {
    pub code: String,
    pub verifier: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CliExchangeResponse {
    pub token: String,
}

/// Inner user record. Matches `UserResponse` in the OpenAPI spec.
#[derive(Debug, Deserialize)]
pub(crate) struct User {
    pub id: Uuid,
    pub email: String,
    #[serde(rename = "name")]
    pub display_name: Option<String>,
}

// ─── Organizations ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub(crate) struct Organization {
    #[allow(dead_code)]
    pub id: Uuid,
    pub slug: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OrganizationList {
    pub data: Vec<Organization>,
}

// ─── Pipelines ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub(crate) struct Pipeline {
    pub id: Uuid,
    pub slug: String,
    #[serde(rename = "name")]
    pub label: Option<String>,
    pub default_branch: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PipelineList {
    pub data: Vec<Pipeline>,
}

// ─── Builds ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Build {
    pub id: Uuid,
    pub number: i64,
    pub state: String,
    pub branch: Option<String>,
    pub message: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BuildList {
    pub data: Vec<Build>,
}

#[derive(Debug, Serialize)]
pub(crate) struct CreateBuildRequest {
    pub pipeline_slug: String,
    pub branch: Option<String>,
    pub message: Option<String>,
    pub env: BTreeMap<String, String>,
    pub plan_json: serde_json::Value,
}

// ─── Jobs ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Job {
    pub id: Uuid,
    pub state: String,
    #[serde(rename = "name")]
    pub label: Option<String>,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct JobList {
    pub data: Vec<Job>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct JobLog {
    #[allow(dead_code)]
    pub job_id: Uuid,
    #[serde(rename = "chunks")]
    pub data: Vec<JobLogChunk>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct JobLogChunk {
    #[allow(dead_code)]
    #[serde(default = "default_stream")]
    pub stream: String,
    #[serde(rename = "content")]
    pub line: String,
    #[allow(dead_code)]
    #[serde(rename = "at")]
    pub ts: DateTime<Utc>,
}

fn default_stream() -> String {
    "stdout".to_string()
}

// ─── Billing ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub(crate) struct Balance {
    #[serde(rename = "balance_cents")]
    pub credits_usd_cents: i64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Transaction {
    #[allow(dead_code)]
    #[serde(rename = "uuid")]
    pub id: Uuid,
    #[serde(rename = "source")]
    pub kind: String,
    pub amount_cents: i64,
    #[serde(rename = "created_at")]
    pub at: DateTime<Utc>,
    #[serde(rename = "description")]
    pub memo: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TransactionList {
    pub data: Vec<Transaction>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UsageWindow {
    #[serde(rename = "window_start")]
    pub from: DateTime<Utc>,
    #[serde(rename = "window_end")]
    pub to: DateTime<Utc>,
    #[serde(default)]
    pub minutes_used: f64,
    #[serde(rename = "total_cost_cents")]
    pub cents_used: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct TopupRequest {
    pub org_slug: String,
    pub amount_cents: i64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TopupResponse {
    pub checkout_url: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct RedeemRequest {
    pub org_slug: String,
    pub code: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RedeemResponse {
    pub credited_cents: i64,
}
