//! Integration test: hm cloud login --paste against a wiremock API.
//!
//! Stubs the two endpoints the paste flow hits — `POST /cli/exchange`
//! (token redemption) and `GET /auth/me` (display name) — and uses
//! `HARMONT_LOGIN_CODE` to inject the "pasted" code without a TTY.
//! Token persistence is delegated to the host keyring; this test
//! asserts on the stderr message ("logged in as Test User"), which is
//! the user-facing signifier that both calls succeeded.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use assert_cmd::Command;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires hm-plugin-cloud installed in ~/.harmont/plugins/ — \
            the test sets HOME to a clean tempdir so no plugins are discovered"]
async fn cloud_login_paste_stores_token_and_prints_user() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/cli/exchange"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "token": "test-token"
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/auth/me"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000001",
            "email": "test@example.com",
            // Wire field is `name` (see api/types.rs); the plan's draft
            // used `display_name`, which serde silently dropped → the
            // login banner fell back to the email.
            "name": "Test User"
        })))
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().unwrap();
    let assert = Command::cargo_bin("hm")
        .unwrap()
        .args(["cloud", "login", "--paste"])
        .env("HARMONT_API_URL", server.uri())
        .env("HARMONT_LOGIN_CODE", "fake-pasted-code")
        .env("XDG_CONFIG_HOME", temp.path())
        .env("HOME", temp.path())
        .current_dir(temp.path())
        .assert();

    assert
        .success()
        .stderr(predicates::str::contains("logged in as Test User"));
}
