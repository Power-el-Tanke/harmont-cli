//! Wire types for the API endpoints the cloud plugin consumes.
//!
//! Hand-written from the API's OpenAPI spec for the specific paths
//! we hit. The legacy progenitor-generated client carried the whole
//! schema; we only need a subset.
//!
//! Field names match `cli/crates/hm/openapi.json` (the canonical
//! source for the live API) — not necessarily the plan's example
//! names. Where the plan and the API disagreed (e.g. plan said
//! `display_name`; API says `name`), the API wins because that's
//! what's on the wire.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─── Auth ──────────────────────────────────────────────────────────────

/// Body of `POST /cli/exchange` (the plan's PKCE-style exchange). The
/// API's closest analog is `POST /api/v0/auth/cli/redeem`, which only
/// carries a `code` — but the plan-4 flow includes a PKCE verifier.
/// Plan 5 reconciles these; for now, we send what the plan dictates.
#[derive(Debug, Serialize)]
#[allow(dead_code, reason = "consumed by auth::login in this cluster")]
pub(crate) struct CliExchangeRequest {
    pub code: String,
    pub verifier: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code, reason = "consumed by auth::login in this cluster")]
pub(crate) struct CliExchangeResponse {
    pub token: String,
}

/// Inner user record. Matches `UserResponse` in the OpenAPI spec.
#[derive(Debug, Deserialize)]
#[allow(dead_code, reason = "consumed by auth verbs")]
pub(crate) struct User {
    pub id: Uuid,
    pub email: String,
    /// The API field is `name`. The plan calls it `display_name` —
    /// rename so the plan's call sites still work.
    #[serde(rename = "name")]
    pub display_name: Option<String>,
}

// ─── Organizations ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[allow(dead_code, reason = "consumed by verbs in a later cluster")]
pub(crate) struct Organization {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code, reason = "consumed by verbs in a later cluster")]
pub(crate) struct OrganizationList {
    pub data: Vec<Organization>,
}

// ─── Pipelines ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[allow(dead_code, reason = "consumed by verbs in a later cluster")]
pub(crate) struct Pipeline {
    pub id: Uuid,
    pub slug: String,
    /// The API field is `name`. The plan calls it `label` — rename.
    #[serde(rename = "name")]
    pub label: Option<String>,
    pub default_branch: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code, reason = "consumed by verbs in a later cluster")]
pub(crate) struct PipelineList {
    pub data: Vec<Pipeline>,
}

// ─── Builds ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
#[allow(dead_code, reason = "consumed by verbs in a later cluster")]
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
#[allow(dead_code, reason = "consumed by verbs in a later cluster")]
pub(crate) struct BuildList {
    pub data: Vec<Build>,
}

/// Body of the create-build POST. The plan's shape carries
/// `pipeline_slug`, `env`, and `plan_json`; the API today takes
/// `branch`, `commit`, `message`, `source`, `source_archive_b64`,
/// and `source_archive_sha256`. We keep the plan's shape — plan-5
/// reconciles this with the live API; cluster 8 (`verbs/run.rs`)
/// adapts the call site.
#[derive(Debug, Serialize)]
#[allow(dead_code, reason = "consumed by verbs in a later cluster")]
pub(crate) struct CreateBuildRequest {
    pub pipeline_slug: String,
    pub branch: Option<String>,
    pub message: Option<String>,
    pub env: BTreeMap<String, String>,
    pub plan_json: serde_json::Value,
}

// ─── Jobs ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
#[allow(dead_code, reason = "consumed by verbs in a later cluster")]
pub(crate) struct Job {
    pub id: Uuid,
    pub state: String,
    /// The API field is `name`. The plan calls it `label` — rename.
    #[serde(rename = "name")]
    pub label: Option<String>,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code, reason = "consumed by verbs in a later cluster")]
pub(crate) struct JobList {
    pub data: Vec<Job>,
}

/// `GET /api/v0/.../jobs/{job_id}/log`. The API envelope is
/// `{chunks, next_idx}`. The plan's draft type called the inner field
/// `data`; rename so deserialisation works against the real server.
#[derive(Debug, Deserialize)]
#[allow(dead_code, reason = "consumed by verbs in a later cluster")]
pub(crate) struct JobLog {
    pub job_id: Uuid,
    #[serde(rename = "chunks")]
    pub data: Vec<JobLogChunk>,
}

/// One contiguous segment of a job's log. The API uses
/// `{idx, content, at}`; the plan's draft used `{stream, line, ts}`.
/// Rename to match the wire.
#[derive(Debug, Deserialize)]
#[allow(dead_code, reason = "consumed by verbs in a later cluster")]
pub(crate) struct JobLogChunk {
    /// API has no per-chunk stream field; the plan kept the name for
    /// future use. We default to "stdout" when absent.
    #[serde(default = "default_stream")]
    pub stream: String,
    #[serde(rename = "content")]
    pub line: String,
    #[serde(rename = "at")]
    pub ts: DateTime<Utc>,
}

fn default_stream() -> String {
    "stdout".to_string()
}

// ─── Billing ───────────────────────────────────────────────────────────

/// `GET /api/v0/organizations/{org_slug}/billing/balance`. The API
/// field is `balance_cents`; the plan called it `credits_usd_cents`.
#[derive(Debug, Deserialize)]
#[allow(dead_code, reason = "consumed by verbs in a later cluster")]
pub(crate) struct Balance {
    #[serde(rename = "balance_cents")]
    pub credits_usd_cents: i64,
}

/// `TransactionResponse`. The API uses `uuid`, `amount_cents`,
/// `source`, `description`, `created_at`. The plan's draft used
/// `id`, `kind`, `amount_cents`, `at`, `memo`. Rename to match.
#[derive(Debug, Deserialize)]
#[allow(dead_code, reason = "consumed by verbs in a later cluster")]
pub(crate) struct Transaction {
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
#[allow(dead_code, reason = "consumed by verbs in a later cluster")]
pub(crate) struct TransactionList {
    pub data: Vec<Transaction>,
}

/// `UsageResponse`. The API uses `window_start`, `window_end`, `items`,
/// `total_cost_cents`. The plan's draft used `from`, `to`,
/// `minutes_used`, `cents_used`. We keep the plan's field names and
/// rename to match the wire; `minutes_used` has no direct API analog,
/// so we make it optional and default to zero.
#[derive(Debug, Deserialize)]
#[allow(dead_code, reason = "consumed by verbs in a later cluster")]
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

/// `CreateCheckoutRequest`. The API takes `{org_slug, amount_cents}`;
/// the plan called the shape `TopupRequest { amount_usd }`. We send
/// the API's shape on the wire but keep the plan's type name; cluster
/// 8's `verbs/billing.rs` populates both fields.
#[derive(Debug, Serialize)]
#[allow(dead_code, reason = "consumed by verbs in a later cluster")]
pub(crate) struct TopupRequest {
    pub org_slug: String,
    pub amount_cents: i64,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code, reason = "consumed by verbs in a later cluster")]
pub(crate) struct TopupResponse {
    pub checkout_url: String,
}

/// `RedeemCouponRequest`. API takes `{org_slug, code}`.
#[derive(Debug, Serialize)]
#[allow(dead_code, reason = "consumed by verbs in a later cluster")]
pub(crate) struct RedeemRequest {
    pub org_slug: String,
    pub code: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code, reason = "consumed by verbs in a later cluster")]
pub(crate) struct RedeemResponse {
    pub credited_cents: i64,
}
