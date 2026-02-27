mod bot;
mod config;
mod data;
mod db;
mod engine;

use std::sync::Arc;
use teloxide::dispatching::UpdateFilterExt;
use teloxide::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "nem_price_bot=info".into()),
        )
        .init();

    let cfg = config::Config::from_env()?;
    let db = Arc::new(db::Db::new(&cfg.database_url)?);
    let bot = Bot::new(&cfg.teloxide_token);

    tracing::info!("NEM Price Bot starting...");

    // Spawn background scheduler
    let sched_db = db.clone();
    let sched_bot = bot.clone();
    let admin_id = cfg.admin_chat_id;
    tokio::spawn(async move {
        engine::scheduler::run(sched_db, sched_bot, admin_id).await;
    });

    // Bot dispatcher
    let handler = dptree::entry()
        .branch(
            Update::filter_message()
                .filter_command::<bot::commands::Command>()
                .endpoint(bot::commands::handle),
        )
        .branch(Update::filter_callback_query().endpoint(bot::callbacks::handle));

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![db])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    Ok(())
}
