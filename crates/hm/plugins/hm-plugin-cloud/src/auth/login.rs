//! `hm cloud login` — browser-loopback or paste-in flow.

use std::collections::BTreeMap;

use hm_plugin_protocol::PluginError;
use hm_plugin_sdk::PluginContext;

use crate::api::types::{CliExchangeRequest, CliExchangeResponse, User};
use crate::config::Config;
use crate::creds;
use crate::http::Client;

#[allow(
    dead_code,
    reason = "wired by `cli::dispatch` in the next cluster (Task 15)"
)]
pub(crate) async fn run(
    ctx: &PluginContext<'_>,
    env: &BTreeMap<String, String>,
    paste: bool,
) -> Result<(), PluginError> {
    let cfg = Config::from_env(env);
    let (verifier, challenge) = pkce_pair()?;

    if paste {
        login_paste(ctx, env, &cfg, &verifier, &challenge).await
    } else {
        login_loopback(ctx, &cfg, &verifier, &challenge).await
    }
}

async fn login_loopback(
    ctx: &PluginContext<'_>,
    cfg: &Config,
    verifier: &str,
    challenge: &str,
) -> Result<(), PluginError> {
    // Bind a one-shot axum server on localhost:0 to receive the OAuth callback.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| PluginError::new("cloud_loopback_spawn", format!("bind loopback: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| PluginError::new("cloud_loopback_spawn", format!("local_addr: {e}")))?
        .port();

    let redirect = format!("http://127.0.0.1:{port}/cb");
    let auth_url = format!(
        "{}/cli/login?challenge={}&redirect_uri={}",
        cfg.api_base,
        challenge,
        urlencoding(&redirect),
    );

    ctx.log(
        hm_plugin_protocol::Level::Info,
        &format!("opening browser to {auth_url}"),
    );
    if webbrowser::open(&auth_url).is_err() {
        ctx.write_stderr(
            format!("couldn't auto-open the browser. Open this URL manually:\n  {auth_url}\n")
                .as_bytes(),
        );
    }

    // Use a oneshot channel to receive the code from the callback handler.
    let (tx, rx) = tokio::sync::oneshot::channel::<String>();
    let tx = std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));

    let app = axum::Router::new().route(
        "/cb",
        axum::routing::get(
            move |axum::extract::Query(params): axum::extract::Query<BTreeMap<String, String>>| {
                let code = params.get("code").cloned().unwrap_or_default();
                if let Some(sender) = tx.lock().ok().and_then(|mut g| g.take()) {
                    let _ = sender.send(code);
                }
                async { "Login received. You can close this tab." }
            },
        ),
    );

    // Serve the axum app in the background; shut down after we get the code.
    let server = tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .ok();
    });

    let code = tokio::time::timeout(std::time::Duration::from_secs(180), rx)
        .await
        .map_err(|_| {
            PluginError::new(
                "cloud_login_timeout",
                "browser callback did not arrive within 3 minutes",
            )
        })?
        .map_err(|_| {
            PluginError::new(
                "cloud_login_timeout",
                "callback channel closed unexpectedly",
            )
        })?;

    server.abort();

    if code.is_empty() {
        return Err(PluginError::new(
            "cloud_login_missing_code",
            "callback had no 'code' query parameter",
        ));
    }

    finalize(ctx, cfg, &code, verifier).await
}

async fn login_paste(
    ctx: &PluginContext<'_>,
    env: &BTreeMap<String, String>,
    cfg: &Config,
    verifier: &str,
    challenge: &str,
) -> Result<(), PluginError> {
    let auth_url = format!(
        "{}/cli/login?challenge={}&redirect_uri=urn:ietf:wg:oauth:2.0:oob",
        cfg.api_base, challenge,
    );
    ctx.write_stderr(
        format!("Open this URL in your browser, then paste the code:\n  {auth_url}\n").as_bytes(),
    );
    let _ = webbrowser::open(&auth_url);

    // Tests inject the code via `HARMONT_LOGIN_CODE` to avoid TTY.
    let code = if let Some(c) = env.get("HARMONT_LOGIN_CODE") {
        c.clone()
    } else {
        dialoguer::Input::<String>::new()
            .with_prompt("code")
            .interact_text()
            .map_err(|e| PluginError::new("cloud_login_tty", format!("prompt failed: {e}")))?
    };
    let code = code.trim().to_string();
    if code.is_empty() {
        return Err(PluginError::new("cloud_login_empty_code", "no code pasted"));
    }
    finalize(ctx, cfg, &code, verifier).await
}

async fn finalize(
    ctx: &PluginContext<'_>,
    cfg: &Config,
    code: &str,
    verifier: &str,
) -> Result<(), PluginError> {
    let client = Client::anonymous(cfg);
    let resp: CliExchangeResponse = client
        .post(
            "/cli/exchange",
            &CliExchangeRequest {
                code: code.to_string(),
                verifier: verifier.to_string(),
            },
        )
        .await?;
    creds::save_token(&cfg.api_base, &resp.token);

    let auth_client = Client::new(cfg, Some(resp.token));
    let me: User = auth_client.get("/auth/me").await?;
    ctx.write_stderr(
        format!(
            "logged in as {} ({})\n",
            me.display_name.clone().unwrap_or_else(|| me.email.clone()),
            me.email,
        )
        .as_bytes(),
    );
    Ok(())
}

/// Generate a PKCE verifier + S256 challenge using real entropy.
fn pkce_pair() -> Result<(String, String), PluginError> {
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use rand::RngCore;
    use sha2::{Digest, Sha256};

    let mut seed = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut seed);
    let verifier = URL_SAFE_NO_PAD.encode(seed);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    Ok((verifier, challenge))
}

fn urlencoding(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}
