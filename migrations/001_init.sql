CREATE TABLE IF NOT EXISTS users (
    chat_id       INTEGER PRIMARY KEY,
    region        TEXT NOT NULL,
    high_alert    REAL NOT NULL DEFAULT 150.0,
    low_alert     REAL NOT NULL DEFAULT 0.0,
    is_active     INTEGER NOT NULL DEFAULT 1,
    battery_kwh   REAL,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS price_history (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    region        TEXT NOT NULL,
    price_mwh     REAL NOT NULL,
    demand_mw     REAL,
    interval_time TEXT NOT NULL,
    fetched_at    TEXT NOT NULL,
    UNIQUE(region, interval_time)
);

CREATE TABLE IF NOT EXISTS forecast (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    region        TEXT NOT NULL,
    forecast_time TEXT NOT NULL,
    price_mwh     REAL NOT NULL,
    published_at  TEXT NOT NULL,
    fetched_at    TEXT NOT NULL,
    UNIQUE(region, forecast_time, published_at)
);

CREATE TABLE IF NOT EXISTS alert_log (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    chat_id       INTEGER NOT NULL,
    alert_type    TEXT NOT NULL,
    price_mwh     REAL NOT NULL,
    region        TEXT NOT NULL,
    sent_at       TEXT NOT NULL,
    FOREIGN KEY (chat_id) REFERENCES users(chat_id)
);

CREATE INDEX IF NOT EXISTS idx_price_region_time ON price_history(region, interval_time);
CREATE INDEX IF NOT EXISTS idx_price_fetched ON price_history(fetched_at);
CREATE INDEX IF NOT EXISTS idx_forecast_region ON forecast(region, forecast_time);
CREATE INDEX IF NOT EXISTS idx_alert_dedup ON alert_log(chat_id, alert_type, sent_at);
