use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};
use teloxide::utils::command::BotCommands;

use crate::bot::messages;
use crate::db::Db;

type HandlerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
pub enum Command {
    Start,
    Price,
    Forecast,
    Alert(String),
    Status,
    Region,
    Help,
    About,
}

fn region_keyboard() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![vec![
        InlineKeyboardButton::callback("NSW", "region:NSW1"),
        InlineKeyboardButton::callback("VIC", "region:VIC1"),
        InlineKeyboardButton::callback("QLD", "region:QLD1"),
        InlineKeyboardButton::callback("SA", "region:SA1"),
        InlineKeyboardButton::callback("TAS", "region:TAS1"),
    ]])
}

pub async fn handle(bot: Bot, msg: Message, cmd: Command, db: Arc<Db>) -> HandlerResult {
    let chat_id = msg.chat.id.0;
    match cmd {
        Command::Start => cmd_start(&bot, &msg).await?,
        Command::Price => cmd_price(&bot, &msg, &db, chat_id).await?,
        Command::Forecast => cmd_forecast(&bot, &msg, &db, chat_id).await?,
        Command::Alert(args) => cmd_alert(&bot, &msg, &db, chat_id, &args).await?,
        Command::Status => cmd_status(&bot, &msg, &db, chat_id).await?,
        Command::Region => cmd_region(&bot, &msg).await?,
        Command::Help => { bot.send_message(msg.chat.id, messages::help_message()).await?; }
        Command::About => { bot.send_message(msg.chat.id, messages::about_message()).await?; }
    }
    Ok(())
}

async fn cmd_start(bot: &Bot, msg: &Message) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    bot.send_message(msg.chat.id, messages::welcome_message())
        .reply_markup(region_keyboard())
        .await?;
    Ok(())
}

async fn cmd_region(bot: &Bot, msg: &Message) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    bot.send_message(msg.chat.id, "Select your new region:")
        .reply_markup(region_keyboard())
        .await?;
    Ok(())
}

async fn cmd_price(bot: &Bot, msg: &Message, db: &Db, chat_id: i64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let user = match db.get_user(chat_id)? {
        Some(u) => u,
        None => {
            bot.send_message(msg.chat.id, "Please use /start to set your region first.").await?;
            return Ok(());
        }
    };
    let (price, time) = match db.get_latest_price(&user.region)? {
        Some(p) => p,
        None => {
            bot.send_message(msg.chat.id, "No price data available yet. Please try again shortly.").await?;
            return Ok(());
        }
    };
    let today_prefix = now_aest_date();
    let range = db.get_daily_range(&user.region, &today_prefix)?;
    let age = interval_age_minutes(&time);
    let text = messages::format_price_response(&user.region, price, &time, range, age);
    bot.send_message(msg.chat.id, text).await?;
    Ok(())
}

async fn cmd_forecast(bot: &Bot, msg: &Message, db: &Db, chat_id: i64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let user = match db.get_user(chat_id)? {
        Some(u) => u,
        None => {
            bot.send_message(msg.chat.id, "Please use /start to set your region first.").await?;
            return Ok(());
        }
    };
    let now = now_aest_str();
    let later = later_aest_str(6);
    let forecasts = db.get_forecasts(&user.region, &now, &later)?;
    let text = messages::format_forecast_response(&user.region, &forecasts);
    bot.send_message(msg.chat.id, text).await?;
    Ok(())
}

async fn cmd_alert(bot: &Bot, msg: &Message, db: &Db, chat_id: i64, args: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let user = match db.get_user(chat_id)? {
        Some(u) => u,
        None => {
            bot.send_message(msg.chat.id, "Please use /start to set your region first.").await?;
            return Ok(());
        }
    };

    let parts: Vec<&str> = args.split_whitespace().collect();
    let reply = match parts.as_slice() {
        ["high", val] => {
            let v: f64 = val.parse().map_err(|_| "Invalid number")?;
            if !(50.0..=15000.0).contains(&v) {
                "High alert must be between $50 and $15,000.".to_string()
            } else if v <= user.low_alert {
                format!(
                    "High alert (${:.0}) must be greater than your current low alert (${:.0}).\n\
                     Adjust low alert first: /alert low <value>",
                    v, user.low_alert
                )
            } else {
                let prev = user.high_alert;
                db.update_high_alert(chat_id, v)?;
                format!(
                    "\u{2705} High price alert updated.\n\n\
                     You'll be notified when {} spot price exceeds ${:.0}/MWh.\n\
                     Previous threshold: ${:.0}/MWh\n\n\
                     Current settings:\n\
                     \u{2022} High alert: ${:.0}/MWh\n\
                     \u{2022} Low alert: ${:.0}/MWh\n\
                     \u{2022} Status: {}",
                    messages::region_display(&user.region), v, prev, v, user.low_alert,
                    if user.is_active { "Active" } else { "Paused" }
                )
            }
        }
        ["low", val] => {
            let v: f64 = val.parse().map_err(|_| "Invalid number")?;
            if !(-1000.0..=50.0).contains(&v) {
                "Low alert must be between -$1,000 and $50.".to_string()
            } else if v >= user.high_alert {
                format!(
                    "Low alert (${:.0}) must be less than your current high alert (${:.0}).\n\
                     Adjust high alert first: /alert high <value>",
                    v, user.high_alert
                )
            } else {
                let prev = user.low_alert;
                db.update_low_alert(chat_id, v)?;
                format!(
                    "\u{2705} Low price alert updated.\n\n\
                     You'll be notified when {} spot price drops below ${:.0}/MWh.\n\
                     Previous threshold: ${:.0}/MWh\n\n\
                     Current settings:\n\
                     \u{2022} High alert: ${:.0}/MWh\n\
                     \u{2022} Low alert: ${:.0}/MWh\n\
                     \u{2022} Status: {}",
                    messages::region_display(&user.region), v, prev, user.high_alert, v,
                    if user.is_active { "Active" } else { "Paused" }
                )
            }
        }
        ["off"] => {
            db.set_active(chat_id, false)?;
            "\u{23f8}\u{fe0f} Alerts paused. Use /alert on to resume.".to_string()
        }
        ["on"] => {
            db.set_active(chat_id, true)?;
            "\u{25b6}\u{fe0f} Alerts resumed.".to_string()
        }
        _ => format!(
            "Your current settings:\n\
             \u{2022} High alert: ${:.0}/MWh\n\
             \u{2022} Low alert: ${:.0}/MWh\n\
             \u{2022} Status: {}\n\n\
             Usage:\n\
             /alert high <value> \u{2014} e.g. /alert high 200\n\
             /alert low <value> \u{2014} e.g. /alert low -20\n\
             /alert off \u{2014} Pause notifications\n\
             /alert on \u{2014} Resume notifications",
            user.high_alert, user.low_alert,
            if user.is_active { "Active \u{2705}" } else { "Paused \u{23f8}\u{fe0f}" }
        ),
    };

    bot.send_message(msg.chat.id, reply).await?;
    Ok(())
}

async fn cmd_status(bot: &Bot, msg: &Message, db: &Db, chat_id: i64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let user = match db.get_user(chat_id)? {
        Some(u) => u,
        None => {
            bot.send_message(msg.chat.id, "Please use /start to set your region first.").await?;
            return Ok(());
        }
    };
    let weekly_alerts = db.count_alerts_this_week(chat_id).unwrap_or(0);
    let member_since = if user.created_at.len() >= 10 { &user.created_at[..10] } else { &user.created_at };
    let text = format!(
        "\u{1f4cb} Your Settings\n\n\
         Region: {}\n\
         High price alert: ${:.0}/MWh\n\
         Low price alert: ${:.0}/MWh\n\
         Alerts: {} {}\n\
         Member since: {}\n\
         Alerts received this week: {}",
        messages::region_display(&user.region),
        user.high_alert,
        user.low_alert,
        if user.is_active { "Active" } else { "Paused" },
        if user.is_active { "\u{2705}" } else { "\u{23f8}\u{fe0f}" },
        member_since,
        weekly_alerts,
    );
    bot.send_message(msg.chat.id, text).await?;
    Ok(())
}

// ── Time helpers (AEST via Brisbane, no DST) ──

fn now_aest_str() -> String {
    chrono::Utc::now()
        .with_timezone(&chrono_tz::Australia::Brisbane)
        .format("%Y/%m/%d %H:%M:%S")
        .to_string()
}

fn later_aest_str(hours: i64) -> String {
    (chrono::Utc::now() + chrono::Duration::hours(hours))
        .with_timezone(&chrono_tz::Australia::Brisbane)
        .format("%Y/%m/%d %H:%M:%S")
        .to_string()
}

fn now_aest_date() -> String {
    chrono::Utc::now()
        .with_timezone(&chrono_tz::Australia::Brisbane)
        .format("%Y/%m/%d")
        .to_string()
}

/// Calculate how many minutes ago an AEMO interval_time was.
/// Returns -1 if the timestamp cannot be parsed.
fn interval_age_minutes(interval_time: &str) -> i64 {
    let now = chrono::Utc::now().with_timezone(&chrono_tz::Australia::Brisbane);
    chrono::NaiveDateTime::parse_from_str(interval_time, "%Y/%m/%d %H:%M:%S")
        .ok()
        .and_then(|naive| naive.and_local_timezone(chrono_tz::Australia::Brisbane).single())
        .map(|dt| now.signed_duration_since(dt).num_minutes().max(0))
        .unwrap_or(-1)
}
