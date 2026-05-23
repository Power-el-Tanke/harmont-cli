//! `hm cloud billing balance|transactions|usage|topup|redeem`.

use std::collections::BTreeMap;

use hm_plugin_protocol::PluginError;
use hm_plugin_sdk::PluginContext;

use crate::api::types::{
    Balance, RedeemRequest, RedeemResponse, TopupRequest, TopupResponse, TransactionList,
    UsageWindow,
};
use crate::cli::BillingCommand;
use crate::config::Config;
use crate::creds;
use crate::http::Client;
use crate::state::CloudState;

pub(crate) async fn run(
    ctx: &PluginContext<'_>,
    env: &BTreeMap<String, String>,
    cmd: BillingCommand,
) -> Result<(), PluginError> {
    let cfg = Config::from_env(env);
    let token = creds::load_token(&cfg.api_base, env).ok_or_else(not_logged_in)?;
    let client = Client::new(&cfg, Some(token));
    let org = active_org(ctx)?;

    match cmd {
        BillingCommand::Balance => balance(ctx, &client, &org).await,
        BillingCommand::Transactions { limit } => transactions(ctx, &client, &org, limit).await,
        BillingCommand::Usage { from, to } => {
            usage(ctx, &client, &org, from.as_deref(), to.as_deref()).await
        }
        BillingCommand::Topup {
            amount_usd,
            no_browser,
        } => topup(ctx, &client, &org, amount_usd, no_browser).await,
        BillingCommand::Redeem { code } => redeem(ctx, &client, &org, &code).await,
    }
}

async fn balance(
    ctx: &PluginContext<'_>,
    client: &Client,
    org: &str,
) -> Result<(), PluginError> {
    let b: Balance = client
        .get(&format!("/organizations/{org}/billing/balance"))
        .await?;
    let dollars = b.credits_usd_cents as f64 / 100.0;
    ctx.write_stdout(format!("${dollars:.2}\n").as_bytes());
    Ok(())
}

async fn transactions(
    ctx: &PluginContext<'_>,
    client: &Client,
    org: &str,
    limit: u32,
) -> Result<(), PluginError> {
    let list: TransactionList = client
        .get(&format!(
            "/organizations/{org}/billing/transactions?limit={limit}"
        ))
        .await?;
    for t in &list.data {
        let line = format!(
            "{}  {:>10} {:<14} {}\n",
            t.at.format("%Y-%m-%d %H:%M:%S"),
            t.amount_cents,
            t.kind,
            t.memo.as_deref().unwrap_or("")
        );
        ctx.write_stdout(line.as_bytes());
    }
    Ok(())
}

async fn usage(
    ctx: &PluginContext<'_>,
    client: &Client,
    org: &str,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<(), PluginError> {
    let mut q = vec![];
    if let Some(f) = from {
        q.push(format!("from={f}"));
    }
    if let Some(t) = to {
        q.push(format!("to={t}"));
    }
    let qs = if q.is_empty() {
        String::new()
    } else {
        format!("?{}", q.join("&"))
    };
    let u: UsageWindow = client
        .get(&format!("/organizations/{org}/billing/usage{qs}"))
        .await?;
    let line = format!(
        "{} -> {}: {:.2} min, ${:.2}\n",
        u.from.format("%Y-%m-%d"),
        u.to.format("%Y-%m-%d"),
        u.minutes_used,
        u.cents_used as f64 / 100.0
    );
    ctx.write_stdout(line.as_bytes());
    Ok(())
}

async fn topup(
    ctx: &PluginContext<'_>,
    client: &Client,
    org: &str,
    amount_usd: u32,
    no_browser: bool,
) -> Result<(), PluginError> {
    let r: TopupResponse = client
        .post(
            &format!("/organizations/{org}/billing/topup"),
            &TopupRequest {
                org_slug: org.to_string(),
                amount_cents: i64::from(amount_usd) * 100,
            },
        )
        .await?;
    if no_browser {
        ctx.write_stdout(r.checkout_url.as_bytes());
        ctx.write_stdout(b"\n");
    } else if webbrowser::open(&r.checkout_url).is_err() {
        ctx.write_stderr(b"couldn't open browser; URL:\n");
        ctx.write_stderr(r.checkout_url.as_bytes());
        ctx.write_stderr(b"\n");
    }
    Ok(())
}

async fn redeem(
    ctx: &PluginContext<'_>,
    client: &Client,
    org: &str,
    code: &str,
) -> Result<(), PluginError> {
    let r: RedeemResponse = client
        .post(
            &format!("/organizations/{org}/billing/redeem"),
            &RedeemRequest {
                org_slug: org.to_string(),
                code: code.to_string(),
            },
        )
        .await?;
    let dollars = r.credited_cents as f64 / 100.0;
    ctx.write_stderr(format!("credited ${dollars:.2}\n").as_bytes());
    Ok(())
}

fn not_logged_in() -> PluginError {
    PluginError::new("cloud_not_logged_in", "not logged in; run `hm cloud login`")
}

fn active_org(ctx: &PluginContext<'_>) -> Result<String, PluginError> {
    CloudState::load(ctx).active_org.ok_or_else(|| {
        PluginError::new(
            "cloud_no_active_org",
            "no active organization; run `hm cloud org switch <slug>`",
        )
    })
}
