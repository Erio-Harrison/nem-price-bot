use anyhow::{Context, Result};

pub struct Config {
    pub teloxide_token: String,
    pub database_url: String,
    pub admin_chat_id: Option<i64>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            teloxide_token: std::env::var("TELOXIDE_TOKEN")
                .context("TELOXIDE_TOKEN not set")?,
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "./data/nem_price.db".into()),
            admin_chat_id: std::env::var("ADMIN_CHAT_ID")
                .ok()
                .and_then(|s| s.parse().ok()),
        })
    }
}
