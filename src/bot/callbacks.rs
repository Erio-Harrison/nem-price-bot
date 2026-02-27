use std::sync::Arc;
use teloxide::prelude::*;
use crate::bot::messages;
use crate::db::Db;

type HandlerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

pub async fn handle(bot: Bot, q: CallbackQuery, db: Arc<Db>) -> HandlerResult {
    let data = match q.data.as_deref() {
        Some(d) => d,
        None => return Ok(()),
    };

    if let Some(region) = data.strip_prefix("region:") {
        let chat_id = q.from.id.0 as i64;
        db.upsert_user(chat_id, region)?;

        let user = db.get_user(chat_id)?;
        let (high, low) = user
            .as_ref()
            .map(|u| (u.high_alert, u.low_alert))
            .unwrap_or((150.0, 0.0));
        let text = messages::confirm_region(region, high, low);

        // Answer callback to remove loading spinner
        bot.answer_callback_query(&q.id).await?;

        // Edit the original message or send new one
        if let Some(msg) = q.message {
            bot.edit_message_text(msg.chat().id, msg.id(), &text).await?;
        } else {
            bot.send_message(ChatId(chat_id), &text).await?;
        }
    }

    Ok(())
}
