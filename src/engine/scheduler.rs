use chrono::Timelike;
use std::sync::Arc;
use std::time::Duration;
use teloxide::prelude::*;

use crate::bot::{messages, notifier};
use crate::data::{fetcher, weather};
use crate::db::Db;
use crate::engine::analyzer;

const REGIONS: &[&str] = &["NSW1", "VIC1", "QLD1", "SA1", "TAS1"];

pub async fn run(db: Arc<Db>, bot: Bot, admin_chat_id: Option<i64>) {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("Failed to build HTTP client");

    tracing::info!("Scheduler started, fetching initial data...");

    // Fetch immediately on startup (no timestamp validation)
    price_fetch_unchecked(&client, &db, &bot, admin_chat_id).await;
    forecast_fetch(&client, &db, &bot, admin_chat_id).await;

    // Spawn aligned price fetcher
    {
        let c = client.clone();
        let d = db.clone();
        let b = bot.clone();
        tokio::spawn(async move { price_fetch_loop(c, d, b, admin_chat_id).await });
    }

    // Spawn aligned forecast fetcher
    {
        let c = client.clone();
        let d = db.clone();
        let b = bot.clone();
        tokio::spawn(async move { forecast_fetch_loop(c, d, b, admin_chat_id).await });
    }

    // Daily summary + DB cleanup loop
    let mut summary_check = tokio::time::interval(Duration::from_secs(60));
    let mut cleanup_interval = tokio::time::interval(Duration::from_secs(86400));
    let mut summary_sent_today = false;

    summary_check.tick().await;
    cleanup_interval.tick().await;

    loop {
        tokio::select! {
            _ = summary_check.tick() => {
                let now_aest = chrono::Utc::now().with_timezone(&chrono_tz::Australia::Brisbane);
                let hour = now_aest.hour();
                if hour == 21 && !summary_sent_today {
                    summary_sent_today = true;
                    handle_daily_summary(&client, &db, &bot).await;
                }
                if hour == 0 {
                    summary_sent_today = false;
                }
            }
            _ = cleanup_interval.tick() => {
                if let Err(e) = db.cleanup_old_records() {
                    tracing::error!(error=%e, "DB cleanup failed");
                } else {
                    tracing::info!("DB cleanup completed");
                }
            }
        }
    }
}

// ── AEMO clock alignment ──────────────────────────────────────────────

/// Duration to wait until next 5-min aligned fetch slot.
/// Targets :01:30, :06:30, :11:30 ... (90 seconds after each 5-min boundary).
/// This gives AEMO ~90s to publish the data after the interval ends.
fn wait_until_next_price_slot() -> Duration {
    let now = chrono::Utc::now().with_timezone(&chrono_tz::Australia::Brisbane);
    let min = now.minute() as i64;
    let sec = now.second() as i64;
    let current_secs = min * 60 + sec;

    let base = min - (min % 5);
    let target_secs = base * 60 + 90; // 1min30s after interval boundary

    let wait = if target_secs > current_secs {
        target_secs - current_secs
    } else {
        target_secs + 300 - current_secs // next interval
    };

    Duration::from_secs(wait.max(1) as u64)
}

/// Duration to wait until next 30-min aligned forecast fetch slot.
fn wait_until_next_forecast_slot() -> Duration {
    let now = chrono::Utc::now().with_timezone(&chrono_tz::Australia::Brisbane);
    let min = now.minute() as i64;
    let sec = now.second() as i64;
    let current_secs = min * 60 + sec;

    let base = min - (min % 30);
    let target_secs = base * 60 + 90;

    let wait = if target_secs > current_secs {
        target_secs - current_secs
    } else {
        target_secs + 1800 - current_secs
    };

    Duration::from_secs(wait.max(1) as u64)
}

/// Expected SETTLEMENTDATE for the current 5-min interval.
/// AEMO SETTLEMENTDATE marks the END of the interval, i.e. the most recent
/// 5-minute boundary. E.g. at 14:01:30 we expect "2026/02/27 14:00:00".
fn expected_settlement_time() -> String {
    let now = chrono::Utc::now().with_timezone(&chrono_tz::Australia::Brisbane);
    let min = now.minute();
    let base = min - (min % 5);
    now.with_minute(base)
        .and_then(|t| t.with_second(0))
        .and_then(|t| t.with_nanosecond(0))
        .unwrap()
        .format("%Y/%m/%d %H:%M:%S")
        .to_string()
}

// ── Fetch loops ───────────────────────────────────────────────────────

/// Aligned price fetch: wait for AEMO publish slot, fetch, validate
/// SETTLEMENTDATE, retry up to 4 times if data is stale.
async fn price_fetch_loop(client: reqwest::Client, db: Arc<Db>, bot: Bot, admin_chat_id: Option<i64>) {
    loop {
        let wait = wait_until_next_price_slot();
        tracing::debug!(wait_secs = wait.as_secs(), "Next price fetch in");
        tokio::time::sleep(wait).await;

        let expected = expected_settlement_time();
        let mut success = false;

        for attempt in 0..5u32 {
            match price_fetch_checked(&client, &db, &bot, admin_chat_id, &expected).await {
                FetchResult::Success => {
                    success = true;
                    break;
                }
                FetchResult::Stale => {
                    tracing::debug!(attempt, expected=%expected, "Data not yet updated, retrying in 15s");
                    tokio::time::sleep(Duration::from_secs(15)).await;
                }
                FetchResult::Error => break,
            }
        }

        if !success {
            tracing::warn!(expected=%expected, "Could not fetch current interval after retries");
        }
    }
}

async fn forecast_fetch_loop(client: reqwest::Client, db: Arc<Db>, bot: Bot, admin_chat_id: Option<i64>) {
    loop {
        let wait = wait_until_next_forecast_slot();
        tracing::debug!(wait_secs = wait.as_secs(), "Next forecast fetch in");
        tokio::time::sleep(wait).await;
        forecast_fetch(&client, &db, &bot, admin_chat_id).await;
    }
}

// ── Fetch implementations ─────────────────────────────────────────────

enum FetchResult {
    Success,
    Stale,
    Error,
}

/// Fetch prices and validate SETTLEMENTDATE matches expected interval.
async fn price_fetch_checked(
    client: &reqwest::Client,
    db: &Arc<Db>,
    bot: &Bot,
    admin_chat_id: Option<i64>,
    expected_time: &str,
) -> FetchResult {
    match fetcher::fetch_dispatch(client).await {
        Ok(prices) => {
            if !prices.iter().any(|p| p.interval_time == expected_time) {
                return FetchResult::Stale;
            }
            tracing::info!(count = prices.len(), interval=%expected_time, "Fetched aligned prices");
            process_prices(db, bot, &prices).await;
            FetchResult::Success
        }
        Err(e) => {
            tracing::error!(error=%e, "Dispatch fetch failed");
            if let Some(admin) = admin_chat_id {
                let _ = bot
                    .send_message(ChatId(admin), format!("\u{26a0}\u{fe0f} Dispatch fetch failed\n{e}"))
                    .await;
            }
            FetchResult::Error
        }
    }
}

/// Fetch prices without timestamp validation (used on startup).
async fn price_fetch_unchecked(
    client: &reqwest::Client,
    db: &Arc<Db>,
    bot: &Bot,
    admin_chat_id: Option<i64>,
) {
    match fetcher::fetch_dispatch(client).await {
        Ok(prices) => {
            tracing::info!(count = prices.len(), "Initial price fetch");
            process_prices(db, bot, &prices).await;
        }
        Err(e) => {
            tracing::error!(error=%e, "Initial dispatch fetch failed");
            if let Some(admin) = admin_chat_id {
                let _ = bot
                    .send_message(ChatId(admin), format!("\u{26a0}\u{fe0f} Startup fetch failed\n{e}"))
                    .await;
            }
        }
    }
}

/// Store prices in DB and run alert analysis.
async fn process_prices(db: &Arc<Db>, bot: &Bot, prices: &[crate::data::parser::PriceRecord]) {
    for p in prices {
        let _ = db.insert_price(&p.region, p.price, &p.interval_time);
    }
    let alerts = analyzer::analyze(db, prices);
    if !alerts.is_empty() {
        tracing::info!(count = alerts.len(), "Sending price alerts");
        notifier::send_alerts(bot, db, alerts).await;
    }
    for region in REGIONS {
        let current = prices
            .iter()
            .find(|p| p.region == *region)
            .map(|p| p.price)
            .unwrap_or(0.0);
        let fc_alerts = analyzer::analyze_forecasts(db, region, current);
        if !fc_alerts.is_empty() {
            notifier::send_alerts(bot, db, fc_alerts).await;
        }
    }
}

async fn forecast_fetch(
    client: &reqwest::Client,
    db: &Arc<Db>,
    bot: &Bot,
    admin_chat_id: Option<i64>,
) {
    match fetcher::fetch_predispatch(client).await {
        Ok(forecasts) => {
            tracing::info!(count = forecasts.len(), "Fetched pre-dispatch forecasts");
            let published_at = chrono::Utc::now()
                .with_timezone(&chrono_tz::Australia::Brisbane)
                .format("%Y/%m/%d %H:%M:%S")
                .to_string();
            for f in &forecasts {
                let _ = db.insert_forecast(&f.region, &f.forecast_time, f.price, &published_at);
            }
        }
        Err(e) => {
            tracing::error!(error=%e, "Pre-dispatch fetch failed");
            if let Some(admin) = admin_chat_id {
                let _ = bot
                    .send_message(ChatId(admin), format!("\u{26a0}\u{fe0f} Pre-dispatch fetch failed\n{e}"))
                    .await;
            }
        }
    }
}

// ── Daily summary ─────────────────────────────────────────────────────

async fn handle_daily_summary(client: &reqwest::Client, db: &Arc<Db>, bot: &Bot) {
    let now_aest = chrono::Utc::now().with_timezone(&chrono_tz::Australia::Brisbane);
    let date_prefix = now_aest.format("%Y/%m/%d").to_string();
    let date_display = now_aest.format("%d %b %Y").to_string();

    for region in REGIONS {
        let stats = db.get_daily_stats(region, &date_prefix).ok().flatten();
        let peak_time = db
            .get_daily_peak_time(region, &date_prefix)
            .ok()
            .flatten();
        let weather_fc = weather::fetch_tomorrow(client, region).await.ok().flatten();

        let users = match db.get_active_users_by_region(region) {
            Ok(u) => u,
            Err(_) => continue,
        };

        for user in &users {
            let alerts_today = db.count_alerts_last_24h(user.chat_id).unwrap_or(0);
            let text = messages::format_daily_summary(
                region,
                &date_display,
                stats.as_ref(),
                peak_time.as_deref(),
                weather_fc.as_ref(),
                alerts_today,
            );
            let _ = bot.send_message(ChatId(user.chat_id), &text).await;
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    tracing::info!("Daily summary sent");
}
