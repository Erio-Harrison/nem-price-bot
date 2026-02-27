use std::sync::Arc;
use teloxide::prelude::*;
use crate::db::Db;
use crate::engine::analyzer::PendingAlert;

pub async fn send_alerts(bot: &Bot, db: &Arc<Db>, alerts: Vec<PendingAlert>) {
    for alert in alerts {
        // Rate limit: max 10/hour per user
        if db.count_alerts_this_hour(alert.chat_id).unwrap_or(10) >= 10 {
            continue;
        }

        match bot.send_message(ChatId(alert.chat_id), &alert.text).await {
            Ok(_) => {
                let _ = db.log_alert(alert.chat_id, &alert.alert_type, alert.price, &alert.region);
            }
            Err(e) => {
                tracing::error!(chat_id = alert.chat_id, error = %e, "Failed to send alert");
                if e.to_string().contains("Forbidden") {
                    let _ = db.set_active(alert.chat_id, false);
                }
            }
        }
        // Basic throttle: avoid hitting Telegram rate limits
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}
