# Technical Documentation

## Architecture

Single Rust binary, ~30MB memory, runs on a $3-5/month VPS.

```
AEMO Nemweb ──HTTP/CSV──> Scheduler ──> Analyzer ──> Notifier ──> Telegram
                              │                         │
                              └───── SQLite (WAL) ──────┘
```

### Data Flow

1. **Scheduler** fetches AEMO dispatch data every 5 min (clock-aligned), pre-dispatch every 30 min
2. **Parser** extracts prices from AEMO's non-standard CSV (I/C/D row format) with dynamic column mapping
3. **Analyzer** checks thresholds, detects spikes, generates alerts
4. **Notifier** delivers via Telegram Bot API with rate limiting and dedup
5. Old records auto-cleaned after 90 days

### AEMO Clock Alignment

The scheduler aligns to AEMO's publish schedule rather than using a fixed interval:

- Targets :01:30, :06:30, :11:30... (90 seconds after each 5-min boundary)
- Validates SETTLEMENTDATE matches expected interval
- Retries up to 5 times (15s apart) if data is stale
- Startup fetch runs immediately without timestamp validation

### Data Source

- Dispatch prices: `nemweb.com.au/Reports/Current/DispatchIS_Reports/` (every 5 min)
- Pre-dispatch forecasts: `nemweb.com.au/Reports/Current/PredispatchIS_Reports/` (every 30 min)
- Weather: BOM API `api.weather.bom.gov.au` (daily forecasts for solar potential)

### NEM Regions

| Region ID | State | BOM Geohash |
|-----------|-------|-------------|
| NSW1 | New South Wales | r3gx2f (Sydney) |
| VIC1 | Victoria | r1r0fs (Melbourne) |
| QLD1 | Queensland | r7hg1c (Brisbane) |
| SA1 | South Australia | r1f94e (Adelaide) |
| TAS1 | Tasmania | r228fh (Hobart) |

Covers all 5 NEM regions. WA (WEM) and NT are separate markets.

## Alert System

### Alert Types

| Type | Trigger | Dedup |
|------|---------|-------|
| `high_price` | Price > user threshold | 30 min |
| `low_price` | Price < user threshold | 30 min |
| `spike` | Price jumps >$100/MWh in 5 min | 30 min |
| `forecast` | Pre-dispatch predicts price > user high threshold within 1 hour | 60 min |
| `all_clear` | Price returns below high threshold after a high-price event | 60 min |

### Rate Limiting

- Max 10 alerts per user per hour
- Telegram send throttle: 50ms between messages
- Users auto-deactivated on `Forbidden` errors (bot blocked)

### Alert Validation

- High alert: $50 - $15,000, must be > low alert
- Low alert: -$1,000 - $50, must be < high alert
- Defaults: high = $150, low = $0

## Daily Summary

Sent at 21:00 AEST to all active users. Includes:

- Price range (min/max/avg)
- Negative price hours
- Peak price and time
- Alerts sent count
- Tomorrow's weather outlook (BOM) with solar potential classification
- Battery strategy suggestion based on solar forecast

### Solar Classification

| BOM Icon | Solar Potential |
|----------|----------------|
| sunny, clear | Excellent |
| mostly_sunny | Good |
| partly_cloudy, hazy | Moderate |
| Everything else | Poor |

## Project Structure

```
src/
├── main.rs              # Entry point: init DB, start bot + scheduler
├── config.rs            # Environment variable loading
├── bot/
│   ├── commands.rs      # /start, /price, /forecast, /alert, /status, /region, /help, /about
│   ├── callbacks.rs     # Inline keyboard (region selection)
│   ├── messages.rs      # Message templates + price level mapping
│   └── notifier.rs      # Alert delivery with rate limiting
├── data/
│   ├── fetcher.rs       # AEMO HTTP download + ZIP extraction + retries
│   ├── parser.rs        # AEMO CSV parsing (dispatch + pre-dispatch)
│   └── weather.rs       # BOM weather API + solar potential classification
├── engine/
│   ├── analyzer.rs      # Threshold checks, spike detection, all-clear logic
│   └── scheduler.rs     # AEMO clock-aligned fetch orchestration
└── db/
    └── repository.rs    # SQLite queries (users, prices, forecasts, alert_log)
```

## Database

SQLite with WAL mode. Schema: [migrations/001_init.sql](migrations/001_init.sql)

| Table | Purpose | Retention |
|-------|---------|-----------|
| `users` | chat_id, region, alert thresholds, active status | Permanent |
| `price_history` | Rolling spot prices per region | 90 days |
| `forecast` | Pre-dispatch forecast data | 7 days |
| `alert_log` | Sent alerts for dedup and analytics | 90 days |

## Tech Stack

| Crate | Purpose |
|-------|---------|
| `teloxide` 0.13 | Telegram bot framework (long polling + dptree) |
| `tokio` | Async runtime + interval scheduling |
| `reqwest` | HTTP client for AEMO/BOM data |
| `zip` | In-memory ZIP extraction |
| `rusqlite` | SQLite with bundled library |
| `chrono` + `chrono-tz` | AEST timezone handling (Brisbane, no DST) |
| `regex` | AEMO directory listing parsing |
| `tracing` | Structured logging |

## Deployment

### Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `TELOXIDE_TOKEN` | Yes | Telegram bot token from @BotFather |
| `DATABASE_URL` | No | SQLite path (default: `./data/nem_price.db`) |
| `ADMIN_CHAT_ID` | No | Your Telegram chat ID, receives error alerts |
| `RUST_LOG` | No | Log level (default: `nem_price_bot=info`) |

### Build & Run

```bash
cp .env.example .env
# Edit .env: set TELOXIDE_TOKEN and ADMIN_CHAT_ID

cargo build --release
./target/release/nem-price-bot
```

### Docker

```bash
docker compose up -d --build
```

Or manually:

```bash
docker build -t nem-price-bot .
docker run -d \
  --name nem-price-bot \
  --restart unless-stopped \
  -v nem-price-data:/data \
  -e TELOXIDE_TOKEN=your_token \
  -e ADMIN_CHAT_ID=your_id \
  nem-price-bot
```

## Roadmap

Phase 2 features:

- `/savings` estimated savings tracker
- `/battery` capacity configuration
- Richer BOM weather integration for solar forecasting
