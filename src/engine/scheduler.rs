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

    // Fetch immediately on startup
    fetch_prices(&client, &db, &bot, admin_chat_id).await;
    forecast_fetch(&client, &db, &bot, admin_chat_id).await;

    // Prices every 60s, forecasts every 5min, cleanup daily
    let mut price_tick = tokio::time::interval(Duration::from_secs(60));
    let mut forecast_tick = tokio::time::interval(Duration::from_secs(300));
    let mut cleanup_tick = tokio::time::interval(Duration::from_secs(86400));
    let mut summary_sent_today = false;

    price_tick.tick().await;
    forecast_tick.tick().await;
    cleanup_tick.tick().await;

    loop {
        tokio::select! {
            _ = price_tick.tick() => {
                fetch_prices(&client, &db, &bot, admin_chat_id).await;
                // Check daily summary (piggyback on 60s tick)
                let now_aest = chrono::Utc::now().with_timezone(&chrono_tz::Australia::Brisbane);
                if now_aest.hour() == 21 && !summary_sent_today {
                    summary_sent_today = true;
                    handle_daily_summary(&client, &db, &bot).await;
                }
                if now_aest.hour() == 0 {
                    summary_sent_today = false;
                }
            }
            _ = forecast_tick.tick() => {
                forecast_fetch(&client, &db, &bot, admin_chat_id).await;
            }
            _ = cleanup_tick.tick() => {
                if let Err(e) = db.cleanup_old_records() {
                    tracing::error!(error=%e, "DB cleanup failed");
                } else {
                    tracing::info!("DB cleanup completed");
                }
            }
        }
    }
}

// ── Fetch implementations ─────────────────────────────────────────────

async fn fetch_prices(
    client: &reqwest::Client,
    db: &Arc<Db>,
    bot: &Bot,
    admin_chat_id: Option<i64>,
) {
    match fetcher::fetch_dispatch(client).await {
        Ok(prices) => {
            tracing::info!(count = prices.len(), "Fetched dispatch prices");
            process_prices(db, bot, &prices).await;
        }
        Err(e) => {
            tracing::error!(error=%e, "Dispatch fetch failed");
            if let Some(admin) = admin_chat_id {
                let _ = bot
                    .send_message(ChatId(admin), format!("\u{26a0}\u{fe0f} Dispatch fetch failed\n{e}"))
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
