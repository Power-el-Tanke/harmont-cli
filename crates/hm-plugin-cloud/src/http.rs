//! Async HTTP wrapper around reqwest. Bearer-token injection, JSON
//! ser/de, status-code to stable error code mapping.

use hm_plugin_protocol::PluginError;
use reqwest::Client as ReqwestClient;
use serde::{Serialize, de::DeserializeOwned};

use crate::config::Config;

pub(crate) struct Client {
    inner: ReqwestClient,
    base: String,
    token: Option<String>,
}

impl Client {
    #[allow(
        dead_code,
        reason = "consumed by authenticated verbs in a later cluster"
    )]
    pub(crate) fn new(config: &Config, token: Option<String>) -> Self {
        Self {
            inner: ReqwestClient::new(),
            base: config.api_base.clone(),
            token,
        }
    }

    #[allow(dead_code, reason = "consumed by the `login` verb in a later cluster")]
    pub(crate) fn anonymous(config: &Config) -> Self {
        Self::new(config, None)
    }

    /// Issue a GET. Body deserialised as `O`.
    #[allow(dead_code, reason = "consumed by verbs in a later cluster")]
    pub(crate) async fn get<O: DeserializeOwned>(&self, path: &str) -> Result<O, PluginError> {
        self.send::<(), O>("GET", path, None).await
    }

    #[allow(dead_code, reason = "consumed by verbs in a later cluster")]
    pub(crate) async fn post<I: Serialize, O: DeserializeOwned>(
        &self,
        path: &str,
        body: &I,
    ) -> Result<O, PluginError> {
        self.send::<I, O>("POST", path, Some(body)).await
    }

    #[allow(dead_code, reason = "consumed by verbs in a later cluster")]
    pub(crate) async fn delete<O: DeserializeOwned>(&self, path: &str) -> Result<O, PluginError> {
        self.send::<(), O>("DELETE", path, None).await
    }

    async fn send<I, O>(&self, method: &str, path: &str, body: Option<&I>) -> Result<O, PluginError>
    where
        I: Serialize,
        O: DeserializeOwned,
    {
        let url = format!("{}{path}", self.base);
        let mut req = self.inner.request(
            method
                .parse()
                .map_err(|e| PluginError::new("cloud_http", format!("{e}")))?,
            &url,
        );
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        req = req.header("Accept", "application/json");
        if let Some(b) = body {
            req = req.json(b);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| PluginError::new("cloud_http_request", format!("{method} {url}: {e}")))?;
        let status = resp.status().as_u16();
        if !(200..300).contains(&status) {
            let snippet = resp.text().await.unwrap_or_default();
            let snippet: String = snippet.chars().take(500).collect();
            return Err(PluginError::new(
                map_status_code(status),
                format!("{method} {url} \u{2192} HTTP {status}: {snippet}"),
            ));
        }
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| PluginError::new("cloud_http_decode", e.to_string()))?;
        if bytes.is_empty() {
            // Treat as unit type if `O` accepts `null` (e.g., `()`).
            return serde_json::from_slice(b"null")
                .map_err(|e| PluginError::new("cloud_http_decode", e.to_string()));
        }
        serde_json::from_slice(&bytes)
            .map_err(|e| PluginError::new("cloud_http_decode", e.to_string()))
    }
}

fn map_status_code(status: u16) -> &'static str {
    match status {
        401 | 403 => "cloud_auth",
        404 => "cloud_not_found",
        429 => "cloud_rate_limited",
        500..=599 => "cloud_server",
        _ => "cloud_http",
    }
}
