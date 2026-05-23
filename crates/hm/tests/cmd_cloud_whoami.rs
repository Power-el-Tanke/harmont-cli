//! Integration test: hm cloud whoami.
//!
//! Two cases:
//! 1. With `HARMONT_API_TOKEN` in env the plugin pulls the token from
//!    env (not the keyring), hits `GET /auth/me` with a `Bearer`
//!    header, and prints the email on stdout.
//! 2. Without any token, the plugin returns a structured error whose
//!    message contains "not logged in"; exit is non-zero.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use assert_cmd::Command;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires hm-plugin-cloud installed in ~/.harmont/plugins/ — \
            the test sets HOME to a clean tempdir so no plugins are discovered"]
async fn cloud_whoami_uses_token_from_env() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/auth/me"))
        .and(header("authorization", "Bearer env-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000001",
            "email": "alice@example.com",
            // Wire field is `name`, not `display_name` (api/types.rs
            // renames). With `null` here the whoami output falls back
            // to the email — exactly what we assert below.
            "name": null
        })))
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().unwrap();
    Command::cargo_bin("hm")
        .unwrap()
        .args(["cloud", "whoami"])
        .env("HARMONT_API_URL", server.uri())
        .env("HARMONT_API_TOKEN", "env-token")
        .env("XDG_CONFIG_HOME", temp.path())
        .env("HOME", temp.path())
        .current_dir(temp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("alice@example.com"));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires hm-plugin-cloud installed in ~/.harmont/plugins/ — \
            the test sets HOME to a clean tempdir so no plugins are discovered"]
async fn cloud_whoami_without_token_returns_helpful_error() {
    let temp = tempfile::tempdir().unwrap();
    Command::cargo_bin("hm")
        .unwrap()
        .args(["cloud", "whoami"])
        .env("HARMONT_API_URL", "https://example.invalid")
        .env_remove("HARMONT_API_TOKEN")
        .env("XDG_CONFIG_HOME", temp.path())
        .env("HOME", temp.path())
        .current_dir(temp.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("not logged in"));
}
