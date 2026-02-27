# NEM Price Bot

Telegram bot for Australian NEM electricity price alerts. Monitors wholesale spot prices from AEMO and sends push notifications to help solar + battery households optimise charge/discharge timing.

## Quick Start

```bash
# 1. Clone and configure
cp .env.example .env
# Edit .env: set TELOXIDE_TOKEN (from @BotFather) and ADMIN_CHAT_ID

# 2. Build and run
cargo build --release
./target/release/nem-price-bot
```

### Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `TELOXIDE_TOKEN` | Yes | Telegram bot token from @BotFather |
| `DATABASE_URL` | No | SQLite path (default: `./data/nem_price.db`) |
| `ADMIN_CHAT_ID` | No | Your Telegram ID, receives error alerts |
| `RUST_LOG` | No | Log level (default: `nem_price_bot=info`) |

### Docker

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

## Bot Commands

| Command | Description |
|---------|-------------|
| `/start` | Register and select NEM region (NSW/VIC/QLD/SA/TAS) |
| `/price` | Current 5-minute spot price with action suggestion |
| `/forecast` | Price forecast for next 4-6 hours |
| `/alert high 200` | Set high price alert threshold ($50-$15,000) |
| `/alert low -20` | Set low price alert threshold (-$1,000-$50) |
| `/alert off` / `on` | Pause / resume notifications |
| `/status` | View current settings |
| `/region` | Change NEM region |
| `/help` | All commands |
| `/about` | Data source and disclaimer |

## Automatic Alerts

The bot pushes notifications without user action:

- **High price** -- spot price exceeds user threshold (30min dedup)
- **Negative/low price** -- spot price drops below user threshold (30min dedup)
- **Spike** -- price jumps >$100/MWh in 5 minutes
- **Forecast warning** -- pre-dispatch predicts high prices within 1 hour (60min dedup)
- **All clear** -- price returns to normal after a high-price event

Rate limit: max 10 messages per user per hour.

## Architecture

Single Rust binary, ~30MB memory, runs on a $3-5/month VPS.

```
AEMO Nemweb ──HTTP/CSV──> Scheduler ──> Analyzer ──> Notifier ──> Telegram
                              │                         │
                              └───── SQLite (WAL) ──────┘
```

### Data Flow

1. **Scheduler** fetches AEMO dispatch data every 5 min, pre-dispatch every 30 min
2. **Parser** extracts prices from AEMO's non-standard CSV (I/C/D row format) with dynamic column mapping
3. **Analyzer** checks thresholds, detects spikes, generates alerts
4. **Notifier** delivers via Telegram Bot API with rate limiting and dedup
5. Old records auto-cleaned after 90 days

### Data Source

- Dispatch prices: `nemweb.com.au/Reports/Current/DispatchIS_Reports/` (every 5 min)
- Pre-dispatch forecasts: `nemweb.com.au/Reports/Current/PredispatchIS_Reports/` (every 30 min)

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
│   └── parser.rs        # AEMO CSV parsing (dispatch + pre-dispatch)
├── engine/
│   ├── analyzer.rs      # Threshold checks, spike detection, all-clear logic
│   └── scheduler.rs     # Tokio interval orchestration (5min/30min/24h)
└── db/
    └── repository.rs    # SQLite queries (users, prices, forecasts, alert_log)
```

## Database

SQLite with WAL mode. 4 tables:

- `users` -- chat_id, region, alert thresholds, active status
- `price_history` -- rolling 90-day spot prices per region
- `forecast` -- pre-dispatch forecast data
- `alert_log` -- sent alerts for deduplication and analytics

Schema: [migrations/001_init.sql](migrations/001_init.sql)

## Tech Stack

| Crate | Purpose |
|-------|---------|
| `teloxide` 0.13 | Telegram bot framework (long polling + dptree) |
| `tokio` | Async runtime + interval scheduling |
| `reqwest` | HTTP client for AEMO data |
| `zip` | In-memory ZIP extraction |
| `rusqlite` | SQLite with bundled library |
| `chrono` + `chrono-tz` | AEST timezone handling |
| `regex` | AEMO directory listing parsing |
| `tracing` | Structured logging |

## Price Level Mapping

| $/MWh | Level | Action |
|-------|-------|--------|
| < 0 | Negative | Charge from grid, run appliances |
| 0-50 | Low | Good time to charge battery |
| 50-100 | Normal | No action needed |
| 100-200 | Elevated | Consider battery power |
| 200-500 | High | Discharge battery |
| > 500 | Extreme | Discharge + export immediately |

## Roadmap

See [FEATURES.md](FEATURES.md) for full spec. Phase 2 features:

- Daily summary at 21:00 AEST
- `/savings` estimated savings tracker
- `/battery` capacity configuration
- BOM weather integration for solar forecasting

## License

MIT
