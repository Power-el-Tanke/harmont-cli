//! Integration test: hm cloud run.
//!
//! Wiremocks the build-create endpoint, seeds `.harmont/plan.json` and
//! the plugin's KV state file (for the active org slug), and runs with
//! `--no-watch` so we don't hit the watch loop. Asserts on the stderr
//! "submitted build #42" signifier.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use assert_cmd::Command;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires hm-plugin-cloud installed in ~/.harmont/plugins/ — \
            the test sets HOME to a clean tempdir so no plugins are discovered"]
async fn cloud_run_submits_and_prints_build_url() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_regex(r"^/organizations/[^/]+/pipelines/[^/]+/builds$"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000001",
            "number": 42,
            "state": "scheduled",
            "branch": null,
            "message": null,
            "started_at": null,
            "finished_at": null
        })))
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join(".harmont")).unwrap();
    std::fs::write(
        temp.path().join(".harmont/plan.json"),
        r#"{"version":"0","steps":[]}"#,
    )
    .unwrap();

    // Pre-populate active_org via the cloud plugin's KV file. The host
    // stores `KvScope::Plugin` state at
    // `$XDG_CONFIG_HOME/harmont/state/<plugin-name>.kv`. The on-disk
    // shape is a JSON map `{<key>: <bytes>}`; the cloud plugin reads a
    // single key `"state"` whose bytes are the JSON of `CloudState`.
    let kv_dir = temp.path().join(".config/harmont/state");
    std::fs::create_dir_all(&kv_dir).unwrap();
    let state_json = serde_json::json!({ "active_org": "test" });
    let inner_bytes = serde_json::to_vec(&state_json).unwrap();
    let outer = serde_json::json!({ "state": inner_bytes });
    std::fs::write(
        kv_dir.join("harmont-cloud.kv"),
        serde_json::to_vec(&outer).unwrap(),
    )
    .unwrap();

    Command::cargo_bin("hm")
        .unwrap()
        .args([
            "cloud",
            "run",
            "p",
            "--no-watch",
            "--plan-file",
            "plan.json",
        ])
        .env("HARMONT_API_URL", server.uri())
        .env("HARMONT_API_TOKEN", "test-token")
        .env("XDG_CONFIG_HOME", temp.path().join(".config"))
        .env("HOME", temp.path())
        .current_dir(temp.path())
        .assert()
        .success()
        .stderr(predicates::str::contains("submitted build #42"));
}
