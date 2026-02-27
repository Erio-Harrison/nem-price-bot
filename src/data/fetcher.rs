use anyhow::{Context, Result};
use regex::Regex;
use std::io::{Cursor, Read};

use crate::data::parser::{self, ForecastRecord, PriceRecord};

const DISPATCH_URL: &str = "https://nemweb.com.au/Reports/Current/DispatchIS_Reports/";
const PREDISPATCH_URL: &str = "https://nemweb.com.au/Reports/Current/PredispatchIS_Reports/";

/// Download and extract the latest CSV from an AEMO directory listing.
async fn fetch_latest_zip(client: &reqwest::Client, base_url: &str, pattern: &str) -> Result<String> {
    let html = client.get(base_url).send().await?.text().await?;

    // AEMO uses uppercase HREF with full paths, e.g. HREF="/Reports/.../PUBLIC_DISPATCHIS_xxx.zip"
    let re = Regex::new(&format!(r#"(?i)href="([^"]*{pattern}[^"]*\.zip)""#))?;
    let mut files: Vec<&str> = re
        .captures_iter(&html)
        .filter_map(|c| c.get(1).map(|m| m.as_str()))
        .collect();
    files.sort();
    let latest = files.last().context("No files found in AEMO listing")?;

    // HREF may be absolute path or relative — build full URL from base domain
    let zip_url = if latest.starts_with('/') {
        format!("https://nemweb.com.au{latest}")
    } else {
        format!("{base_url}{latest}")
    };
    let bytes = client.get(&zip_url).send().await?.bytes().await?;

    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)?;
    let mut file = archive.by_index(0)?;
    let mut csv_text = String::new();
    file.read_to_string(&mut csv_text)?;
    Ok(csv_text)
}

/// Fetch latest dispatch prices with retries.
pub async fn fetch_dispatch(client: &reqwest::Client) -> Result<Vec<PriceRecord>> {
    for attempt in 0..3 {
        match fetch_latest_zip(client, DISPATCH_URL, "PUBLIC_DISPATCHIS_").await {
            Ok(csv) => return Ok(parser::parse_dispatch(&csv)),
            Err(e) => {
                tracing::warn!(attempt, error=%e, "Dispatch fetch failed");
                if attempt < 2 {
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                }
            }
        }
    }
    anyhow::bail!("Failed to fetch dispatch data after 3 attempts")
}

/// Fetch latest pre-dispatch forecasts with retries.
pub async fn fetch_predispatch(client: &reqwest::Client) -> Result<Vec<ForecastRecord>> {
    for attempt in 0..3 {
        match fetch_latest_zip(client, PREDISPATCH_URL, "PUBLIC_PREDISPATCHIS_").await {
            Ok(csv) => return Ok(parser::parse_predispatch(&csv)),
            Err(e) => {
                tracing::warn!(attempt, error=%e, "Pre-dispatch fetch failed");
                if attempt < 2 {
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                }
            }
        }
    }
    anyhow::bail!("Failed to fetch pre-dispatch data after 3 attempts")
}
