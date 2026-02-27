use crate::bot::messages;
use crate::data::parser::PriceRecord;
use crate::db::Db;

pub struct PendingAlert {
    pub chat_id: i64,
    pub text: String,
    pub alert_type: String,
    pub price: f64,
    pub region: String,
}

/// Analyze latest prices and generate alerts for all affected users.
pub fn analyze(db: &Db, prices: &[PriceRecord]) -> Vec<PendingAlert> {
    let mut alerts = Vec::new();
    let today_prefix = chrono::Utc::now()
        .with_timezone(&chrono_tz::Australia::Brisbane)
        .format("%Y/%m/%d")
        .to_string();

    for rec in prices {
        let region = &rec.region;
        let current = rec.price;

        // Spike detection: compare with previous price
        let prev_price = db.get_previous_price(region).ok().flatten();
        if let Some(prev) = prev_price {
            if (current - prev).abs() > 100.0 {
                if let Ok(users) = db.get_active_users_by_region(region) {
                    for user in &users {
                        if can_alert(db, user.chat_id, "spike", 30) {
                            alerts.push(PendingAlert {
                                chat_id: user.chat_id,
                                text: messages::format_spike_alert(region, prev, current),
                                alert_type: "spike".into(),
                                price: current,
                                region: region.clone(),
                            });
                        }
                    }
                }
            }
        }

        // Threshold alerts
        let users = match db.get_active_users_by_region(region) {
            Ok(u) => u,
            Err(_) => continue,
        };
        let daily_range = db.get_daily_range(region, &today_prefix).ok().flatten();

        for user in &users {
            // High price alert
            if current > user.high_alert && can_alert(db, user.chat_id, "high_price", 30) {
                alerts.push(PendingAlert {
                    chat_id: user.chat_id,
                    text: messages::format_high_alert(region, current, user.high_alert, daily_range),
                    alert_type: "high_price".into(),
                    price: current,
                    region: region.clone(),
                });
            }

            // Low price alert
            if current < user.low_alert && can_alert(db, user.chat_id, "low_price", 30) {
                alerts.push(PendingAlert {
                    chat_id: user.chat_id,
                    text: messages::format_low_alert(region, current),
                    alert_type: "low_price".into(),
                    price: current,
                    region: region.clone(),
                });
            }

            // All clear: was high, now normal
            if current <= user.high_alert {
                let was_high = db.was_alert_sent_recently(user.chat_id, "high_price", 180).unwrap_or(false);
                let already_cleared = db.was_alert_sent_recently(user.chat_id, "all_clear", 60).unwrap_or(false);
                if was_high && !already_cleared {
                    let peak = daily_range.map(|(_, max)| max);
                    alerts.push(PendingAlert {
                        chat_id: user.chat_id,
                        text: messages::format_all_clear(region, current, peak),
                        alert_type: "all_clear".into(),
                        price: current,
                        region: region.clone(),
                    });
                }
            }
        }
    }

    alerts
}

/// Check forecasts and generate pre-dispatch warnings.
pub fn analyze_forecasts(db: &Db, region: &str, current_price: f64) -> Vec<PendingAlert> {
    let mut alerts = Vec::new();
    let now = chrono::Utc::now().with_timezone(&chrono_tz::Australia::Brisbane);
    let now_str = now.format("%Y/%m/%d %H:%M:%S").to_string();
    let later_str = (now + chrono::Duration::hours(1)).format("%Y/%m/%d %H:%M:%S").to_string();

    let forecasts = match db.get_forecasts(region, &now_str, &later_str) {
        Ok(f) => f,
        Err(_) => return alerts,
    };
    let users = match db.get_active_users_by_region(region) {
        Ok(u) => u,
        Err(_) => return alerts,
    };

    for (fc_time, fc_price) in &forecasts {
        for user in &users {
            if *fc_price > user.high_alert && can_alert(db, user.chat_id, "forecast", 60) {
                alerts.push(PendingAlert {
                    chat_id: user.chat_id,
                    text: messages::format_forecast_alert(region, *fc_price, fc_time, current_price),
                    alert_type: "forecast".into(),
                    price: *fc_price,
                    region: region.into(),
                });
            }
        }
    }

    alerts
}

fn can_alert(db: &Db, chat_id: i64, alert_type: &str, dedup_minutes: i64) -> bool {
    let not_dup = !db.was_alert_sent_recently(chat_id, alert_type, dedup_minutes).unwrap_or(true);
    let under_limit = db.count_alerts_this_hour(chat_id).unwrap_or(10) < 10;
    not_dup && under_limit
}
