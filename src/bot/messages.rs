/// Returns (emoji, label, suggestion) for a price level.
pub fn price_level(price: f64) -> (&'static str, &'static str, &'static str) {
    if price < 0.0 {
        ("\u{1f7e2}\u{1f4b0}", "Negative", "Charge from grid. Run heavy appliances. You're being paid to use power.")
    } else if price < 50.0 {
        ("\u{1f7e2}", "Low", "Good time to charge battery from grid.")
    } else if price < 100.0 {
        ("\u{1f7e1}", "Normal", "No action needed \u{2014} prices are within typical range.")
    } else if price < 200.0 {
        ("\u{1f7e0}", "Elevated", "Consider switching to battery power.")
    } else if price < 500.0 {
        ("\u{1f534}", "High", "Discharge battery. Minimise grid usage.")
    } else {
        ("\u{1f534}\u{1f525}", "Extreme", "Discharge and export to grid immediately. Pause heavy appliances.")
    }
}

pub fn region_display(region: &str) -> &str {
    match region {
        "NSW1" => "NSW",
        "VIC1" => "VIC",
        "QLD1" => "QLD",
        "SA1" => "SA",
        "TAS1" => "TAS",
        _ => region,
    }
}

fn format_time_short(interval_time: &str) -> &str {
    // "2026/02/27 14:35:00" -> "14:35"
    if interval_time.len() >= 16 {
        &interval_time[11..16]
    } else {
        interval_time
    }
}

pub fn format_price_response(
    region: &str,
    price: f64,
    interval_time: &str,
    daily_range: Option<(f64, f64)>,
    age_minutes: i64,
) -> String {
    let (emoji, label, suggestion) = price_level(price);
    let time_str = format_time_short(interval_time);
    let range_str = match daily_range {
        Some((min, max)) => format!("Today's range: ${:.0} ~ ${:.0}", min, max),
        None => "No data for today yet.".into(),
    };
    let age_str = if age_minutes < 0 {
        String::new()
    } else if age_minutes <= 1 {
        " (just now)".to_string()
    } else {
        format!(" ({} min ago)", age_minutes)
    };
    let stale = if age_minutes > 5 { " \u{26a0}\u{fe0f}" } else { "" };
    format!(
        "\u{26a1} {} Spot Price\n\n${:.2}/MWh {} {}\n\n{}\n\nUpdated: {} AEST{}{} | {}",
        region_display(region), price, emoji, label, suggestion, time_str, age_str, stale, range_str
    )
}

pub fn format_forecast_response(region: &str, forecasts: &[(String, f64)]) -> String {
    if forecasts.is_empty() {
        return format!("\u{1f4c8} {} Price Forecast\n\nNo forecast data available.", region_display(region));
    }
    let mut lines = vec![format!("\u{1f4c8} {} Price Forecast\n", region_display(region))];
    let mut peak_price = f64::MIN;
    let mut peak_time = "";
    for (time, price) in forecasts {
        let (emoji, _, _) = price_level(*price);
        let ts = format_time_short(time);
        let marker = if *price > peak_price {
            peak_price = *price;
            peak_time = time;
            "  \u{2190} Peak expected"
        } else {
            ""
        };
        lines.push(format!("{}  ${:.0}/MWh   {}{}", ts, price, emoji, marker));
    }
    // Re-mark the actual peak (remove intermediate markers)
    let peak_ts = format_time_short(peak_time);
    lines.push(format!(
        "\n\u{1f4a1} Peak expected around {}.\n\n\u{26a0}\u{fe0f} Forecasts are estimates and may change.",
        peak_ts
    ));
    lines.join("\n")
}

pub fn format_high_alert(region: &str, price: f64, threshold: f64, daily_range: Option<(f64, f64)>) -> String {
    let range_str = match daily_range {
        Some((min, max)) => format!("Today's range: ${:.0} ~ ${:.0}", min, max),
        None => String::new(),
    };
    format!(
        "\u{26a1} HIGH PRICE \u{2014} {}\n\n\
         Current price: ${:.2}/MWh \u{1f534}\n\
         Your threshold: ${:.0}/MWh\n\n\
         \u{1f4a1} What to do:\n\
         \u{2192} Switch battery to discharge / export mode\n\
         \u{2192} Avoid running dishwasher, dryer, pool pump\n\
         \u{2192} If on a VPP, ensure export is enabled\n\n\
         {}",
        region_display(region), price, threshold, range_str
    )
}

pub fn format_low_alert(region: &str, price: f64) -> String {
    let label = if price < 0.0 { "NEGATIVE PRICE" } else { "LOW PRICE" };
    format!(
        "\u{1f50b} {} \u{2014} {}\n\n\
         Current price: ${:.2}/MWh \u{1f7e2}\u{1f4b0}\n\n\
         \u{1f4a1} What to do:\n\
         \u{2192} Switch battery to charge from grid\n\
         \u{2192} Run washing machine, dryer, dishwasher\n\
         {}",
        label,
        region_display(region),
        price,
        if price < 0.0 { "\u{2192} You're being PAID to use electricity!" } else { "" }
    )
}

pub fn format_spike_alert(region: &str, prev: f64, current: f64) -> String {
    format!(
        "\u{26a0}\u{fe0f} PRICE SPIKE \u{2014} {}\n\n\
         Price jumped from ${:.0} \u{2192} ${:.0}/MWh in 5 minutes!\n\
         This is unusual and may indicate a supply event.\n\n\
         \u{1f4a1} Switch to battery power immediately if you haven't already.",
        region_display(region), prev, current
    )
}

pub fn format_forecast_alert(region: &str, forecast_price: f64, forecast_time: &str, current_price: f64) -> String {
    let ts = format_time_short(forecast_time);
    format!(
        "\u{1f4e2} HEADS UP \u{2014} {}\n\n\
         Prices forecast to reach ${:.0}+/MWh around {}.\n\
         Current price: ${:.0}/MWh \u{1f7e1}\n\n\
         \u{1f4a1} Prepare now:\n\
         \u{2192} Ensure battery is fully charged\n\
         \u{2192} Set battery to discharge when peak begins\n\
         \u{2192} Delay any heavy appliance usage",
        region_display(region), forecast_price, ts, current_price
    )
}

pub fn format_all_clear(region: &str, price: f64, peak: Option<f64>) -> String {
    let peak_str = match peak {
        Some(p) => format!("\nPeak reached: ${:.0}/MWh", p),
        None => String::new(),
    };
    let (emoji, _, _) = price_level(price);
    format!(
        "\u{2705} PRICES NORMAL \u{2014} {}\n\n\
         Price has dropped back to ${:.2}/MWh {}\n\
         {}",
        region_display(region), price, emoji, peak_str
    )
}

pub fn format_daily_summary(
    region: &str,
    date_display: &str,
    stats: Option<&crate::db::repository::DailyStats>,
    peak_time: Option<&str>,
    weather: Option<&crate::data::weather::WeatherForecast>,
    alerts_today: i64,
) -> String {
    let mut lines = vec![format!(
        "\u{1f4ca} Daily Summary \u{2014} {} \u{2014} {}\n",
        region_display(region), date_display
    )];

    if let Some(s) = stats {
        lines.push(format!("Price range: ${:.0} ~ ${:.0}/MWh", s.min_price, s.max_price));
        lines.push(format!("Average price: ${:.0}/MWh", s.avg_price));
        if s.negative_hours > 0.0 {
            lines.push(format!("Negative price hours: {:.1}h", s.negative_hours));
        }
        if let Some(pt) = peak_time {
            lines.push(format!("Peak: ${:.0}/MWh at {} AEST", s.max_price, format_time_short(pt)));
        }
    } else {
        lines.push("No price data recorded today.".into());
    }

    lines.push(format!("\nAlerts sent today: {}", alerts_today));

    // Tomorrow's weather outlook
    if let Some(w) = weather {
        let temp_str = match w.temp_max {
            Some(t) => format!(", {:.0}\u{00b0}C", t),
            None => String::new(),
        };
        lines.push(format!(
            "\nTomorrow's outlook:\n{} {}{} \u{2014} {}",
            w.solar.emoji(), w.description, temp_str, w.solar.label()
        ));
        // Strategy based on solar potential
        lines.push(match &w.solar {
            crate::data::weather::SolarPotential::Excellent | crate::data::weather::SolarPotential::Good => {
                "\u{1f50b} Likely negative prices midday\n\
                 \u{2022} Morning: Let solar charge battery\n\
                 \u{2022} Midday: Charge from grid (negative prices)\n\
                 \u{2022} Evening: Discharge during peak".into()
            }
            crate::data::weather::SolarPotential::Moderate => {
                "\u{26c5} Some solar generation expected\n\
                 \u{2022} Midday prices may dip but unlikely negative\n\
                 \u{2022} Evening: Discharge during peak if prices rise".into()
            }
            crate::data::weather::SolarPotential::Poor => {
                "\u{1f327}\u{fe0f} Low solar generation expected\n\
                 \u{2022} Prices unlikely to go negative\n\
                 \u{2022} Conserve battery for evening peak".into()
            }
        });
        // Heat warning
        if let Some(t) = w.temp_max {
            if t >= 35.0 {
                lines.push("\u{26a1} Extreme heat \u{2014} expect high evening demand and prices".into());
            } else if t >= 30.0 {
                lines.push("\u{26a1} Hot day \u{2014} possible elevated evening prices".into());
            }
        }
    }

    lines.push("\nPowered by AEMO + BOM data | /help for commands".into());
    lines.join("\n")
}

pub fn welcome_message() -> &'static str {
    "Welcome to NEM Price Bot! \u{26a1}\n\n\
     I'll send you real-time electricity price alerts so you know\n\
     when to charge and discharge your home battery.\n\n\
     Select your NEM region:"
}

pub fn confirm_region(region: &str, high_alert: f64, low_alert: f64) -> String {
    format!(
        "\u{2705} You're set up for {}.\n\n\
         Current alerts:\n\
         \u{2022} High price: ${:.0}/MWh (notify when price goes above)\n\
         \u{2022} Low price: ${:.0}/MWh (notify when price drops below)\n\n\
         Commands:\n\
         /price \u{2014} Current spot price\n\
         /forecast \u{2014} Next few hours outlook\n\
         /alert \u{2014} Customise alert thresholds\n\
         /status \u{2014} View your settings\n\
         /help \u{2014} All commands",
        region_display(region), high_alert, low_alert
    )
}

pub fn help_message() -> &'static str {
    "NEM Price Bot \u{2014} Help \u{26a1}\n\n\
     \u{1f4ca} Check prices:\n\
     /price \u{2014} Current spot price for your region\n\
     /forecast \u{2014} Price forecast for next 4\u{2013}6 hours\n\n\
     \u{1f514} Manage alerts:\n\
     /alert high 200 \u{2014} Notify above $200/MWh\n\
     /alert low -20 \u{2014} Notify below -$20/MWh\n\
     /alert off \u{2014} Pause notifications\n\
     /alert on \u{2014} Resume notifications\n\n\
     \u{2699}\u{fe0f} Settings:\n\
     /status \u{2014} View current settings\n\
     /region \u{2014} Change your NEM region\n\n\
     \u{2139}\u{fe0f} About:\n\
     /about \u{2014} What is this bot and where does the data come from\n\n\
     Data source: AEMO (aemo.com.au)\n\
     Prices update every 5 minutes.\n\n\
     \u{26a0}\u{fe0f} This is an information service only. Always verify\n\
     before making decisions. Not financial advice."
}

pub fn about_message() -> &'static str {
    "NEM Price Bot \u{26a1}\n\n\
     An independent electricity price alert tool for Australian\n\
     solar + battery households.\n\n\
     \u{1f4e1} Data source:\n\
     Wholesale spot prices from AEMO's NEM dispatch system\n\
     (nemweb.com.au). Updated every 5 minutes.\n\n\
     \u{1f512} Privacy:\n\
     We only store your Telegram chat ID and region selection.\n\
     No personal information is collected.\n\n\
     \u{26a0}\u{fe0f} Disclaimer:\n\
     This service provides wholesale market data for\n\
     informational purposes only. It does not constitute\n\
     financial, energy, or investment advice. Always verify\n\
     information before acting. Battery operation is entirely\n\
     at your own discretion and risk.\n\n\
     Built with \u{1f980} Rust"
}
