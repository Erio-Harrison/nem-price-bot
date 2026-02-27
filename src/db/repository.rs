use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::sync::Mutex;

pub struct Db {
    conn: Mutex<Connection>,
}

pub struct DailyStats {
    pub min_price: f64,
    pub max_price: f64,
    pub avg_price: f64,
    pub negative_hours: f64,
}

pub struct User {
    pub chat_id: i64,
    pub region: String,
    pub high_alert: f64,
    pub low_alert: f64,
    pub is_active: bool,
    pub created_at: String,
}

impl Db {
    pub fn new(path: &str) -> Result<Self> {
        if let Some(parent) = std::path::Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
        conn.execute_batch(include_str!("../../migrations/001_init.sql"))?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    // ── Users ──

    pub fn upsert_user(&self, chat_id: i64, region: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO users (chat_id, region, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?3)
             ON CONFLICT(chat_id) DO UPDATE SET region=?2, updated_at=?3",
            params![chat_id, region, now],
        )?;
        Ok(())
    }

    pub fn get_user(&self, chat_id: i64) -> Result<Option<User>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT chat_id, region, high_alert, low_alert, is_active, created_at
             FROM users WHERE chat_id=?1",
            params![chat_id],
            |row| {
                Ok(User {
                    chat_id: row.get(0)?,
                    region: row.get(1)?,
                    high_alert: row.get(2)?,
                    low_alert: row.get(3)?,
                    is_active: row.get::<_, i32>(4)? != 0,
                    created_at: row.get(5)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn update_high_alert(&self, chat_id: i64, value: f64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE users SET high_alert=?1, updated_at=?2 WHERE chat_id=?3",
            params![value, now, chat_id],
        )?;
        Ok(())
    }

    pub fn update_low_alert(&self, chat_id: i64, value: f64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE users SET low_alert=?1, updated_at=?2 WHERE chat_id=?3",
            params![value, now, chat_id],
        )?;
        Ok(())
    }

    pub fn set_active(&self, chat_id: i64, active: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE users SET is_active=?1, updated_at=?2 WHERE chat_id=?3",
            params![active as i32, now, chat_id],
        )?;
        Ok(())
    }

    // ── Prices ──

    pub fn insert_price(&self, region: &str, price: f64, interval_time: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO price_history (region, price_mwh, interval_time, fetched_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![region, price, interval_time, chrono::Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn get_latest_price(&self, region: &str) -> Result<Option<(f64, String)>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT price_mwh, interval_time FROM price_history
             WHERE region=?1 ORDER BY interval_time DESC LIMIT 1",
            params![region],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn get_previous_price(&self, region: &str) -> Result<Option<f64>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT price_mwh FROM price_history
             WHERE region=?1 ORDER BY interval_time DESC LIMIT 1 OFFSET 1",
            params![region],
            |row| row.get(0),
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn get_daily_range(&self, region: &str, today_prefix: &str) -> Result<Option<(f64, f64)>> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            "SELECT MIN(price_mwh), MAX(price_mwh) FROM price_history
             WHERE region=?1 AND interval_time LIKE ?2",
            params![region, format!("{today_prefix}%")],
            |row| Ok((row.get::<_, Option<f64>>(0)?, row.get::<_, Option<f64>>(1)?)),
        )?;
        match result {
            (Some(min), Some(max)) => Ok(Some((min, max))),
            _ => Ok(None),
        }
    }

    // ── Forecasts ──

    pub fn insert_forecast(
        &self, region: &str, forecast_time: &str, price: f64, published_at: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO forecast (region, forecast_time, price_mwh, published_at, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![region, forecast_time, price, published_at, chrono::Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn get_forecasts(&self, region: &str, after: &str, before: &str) -> Result<Vec<(String, f64)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT forecast_time, price_mwh FROM forecast
             WHERE region=?1 AND forecast_time>?2 AND forecast_time<=?3
             ORDER BY forecast_time, published_at DESC",
        )?;
        let rows: Vec<(String, f64)> = stmt
            .query_map(params![region, after, before], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // Keep only the latest published forecast per time slot
        let mut seen = std::collections::HashSet::new();
        Ok(rows.into_iter().filter(|(t, _)| seen.insert(t.clone())).collect())
    }

    // ── Alert queries ──

    pub fn get_active_users_by_region(&self, region: &str) -> Result<Vec<User>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT chat_id, region, high_alert, low_alert, is_active, created_at
             FROM users WHERE region=?1 AND is_active=1",
        )?;
        let users = stmt
            .query_map(params![region], |row| {
                Ok(User {
                    chat_id: row.get(0)?,
                    region: row.get(1)?,
                    high_alert: row.get(2)?,
                    low_alert: row.get(3)?,
                    is_active: true,
                    created_at: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(users)
    }

    pub fn log_alert(&self, chat_id: i64, alert_type: &str, price: f64, region: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO alert_log (chat_id, alert_type, price_mwh, region, sent_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![chat_id, alert_type, price, region, chrono::Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn was_alert_sent_recently(&self, chat_id: i64, alert_type: &str, minutes: i64) -> Result<bool> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::minutes(minutes)).to_rfc3339();
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM alert_log
             WHERE chat_id=?1 AND alert_type=?2 AND sent_at>?3",
            params![chat_id, alert_type, cutoff],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn count_alerts_this_hour(&self, chat_id: i64) -> Result<i64> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM alert_log WHERE chat_id=?1 AND sent_at>?2",
            params![chat_id, cutoff],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub fn count_alerts_this_week(&self, chat_id: i64) -> Result<i64> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM alert_log WHERE chat_id=?1 AND sent_at>?2",
            params![chat_id, cutoff],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    // ── Daily summary queries ──

    pub fn get_daily_stats(&self, region: &str, date_prefix: &str) -> Result<Option<DailyStats>> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            "SELECT MIN(price_mwh), MAX(price_mwh), AVG(price_mwh),
                    SUM(CASE WHEN price_mwh < 0 THEN 1 ELSE 0 END),
                    COUNT(*)
             FROM price_history
             WHERE region=?1 AND interval_time LIKE ?2",
            params![region, format!("{date_prefix}%")],
            |row| {
                Ok((
                    row.get::<_, Option<f64>>(0)?,
                    row.get::<_, Option<f64>>(1)?,
                    row.get::<_, Option<f64>>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            },
        )?;
        match result {
            (Some(min), Some(max), Some(avg), neg_count, total) if total > 0 => {
                Ok(Some(DailyStats {
                    min_price: min,
                    max_price: max,
                    avg_price: avg,
                    negative_hours: neg_count as f64 * 5.0 / 60.0,
                }))
            }
            _ => Ok(None),
        }
    }

    pub fn get_daily_peak_time(&self, region: &str, date_prefix: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT interval_time FROM price_history
             WHERE region=?1 AND interval_time LIKE ?2
             ORDER BY price_mwh DESC LIMIT 1",
            params![region, format!("{date_prefix}%")],
            |row| row.get(0),
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn count_alerts_last_24h(&self, chat_id: i64) -> Result<i64> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::hours(24)).to_rfc3339();
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM alert_log WHERE chat_id=?1 AND sent_at>?2",
            params![chat_id, cutoff],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub fn cleanup_old_records(&self) -> Result<()> {
        let cutoff_90d = (chrono::Utc::now() - chrono::Duration::days(90)).to_rfc3339();
        let cutoff_7d = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM price_history WHERE fetched_at<?1", params![cutoff_90d])?;
        conn.execute("DELETE FROM alert_log WHERE sent_at<?1", params![cutoff_90d])?;
        conn.execute("DELETE FROM forecast WHERE fetched_at<?1", params![cutoff_7d])?;
        Ok(())
    }
}
