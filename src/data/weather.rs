use anyhow::Result;
use serde::Deserialize;

// BOM API geohashes for NEM region capital cities
fn region_geohash(region: &str) -> Option<&'static str> {
    match region {
        "NSW1" => Some("r3gx2f"),  // Sydney
        "VIC1" => Some("r1r0fs"),  // Melbourne
        "QLD1" => Some("r7hg1c"),  // Brisbane
        "SA1"  => Some("r1f94e"),  // Adelaide
        "TAS1" => Some("r228fh"),  // Hobart
        _ => None,
    }
}

#[derive(Deserialize)]
struct BomResponse {
    data: Vec<DayForecast>,
}

#[derive(Deserialize)]
struct DayForecast {
    temp_max: Option<f64>,
    icon_descriptor: Option<String>,
    short_text: Option<String>,
}

pub struct WeatherForecast {
    pub temp_max: Option<f64>,
    pub description: String,
    pub solar: SolarPotential,
}

pub enum SolarPotential {
    Excellent,
    Good,
    Moderate,
    Poor,
}

impl SolarPotential {
    pub fn emoji(&self) -> &'static str {
        match self {
            Self::Excellent => "\u{2600}\u{fe0f}",  // ☀️
            Self::Good      => "\u{1f324}\u{fe0f}",  // 🌤️
            Self::Moderate  => "\u{26c5}",            // ⛅
            Self::Poor      => "\u{1f327}\u{fe0f}",  // 🌧️
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::Excellent => "Excellent solar day",
            Self::Good      => "Good solar day",
            Self::Moderate  => "Moderate solar",
            Self::Poor      => "Poor solar day",
        }
    }
}

fn classify_solar(icon: &str) -> SolarPotential {
    match icon {
        "sunny" | "clear" => SolarPotential::Excellent,
        "mostly_sunny" => SolarPotential::Good,
        "partly_cloudy" | "hazy" => SolarPotential::Moderate,
        _ => SolarPotential::Poor,
    }
}

/// Fetch tomorrow's weather forecast for a NEM region.
pub async fn fetch_tomorrow(client: &reqwest::Client, region: &str) -> Result<Option<WeatherForecast>> {
    let geohash = match region_geohash(region) {
        Some(g) => g,
        None => return Ok(None),
    };
    let url = format!("https://api.weather.bom.gov.au/v1/locations/{geohash}/forecasts/daily");
    let resp: BomResponse = client.get(&url).send().await?.json().await?;

    // index 0 = today, index 1 = tomorrow
    let tomorrow = match resp.data.get(1) {
        Some(d) => d,
        None => return Ok(None),
    };
    let icon = tomorrow.icon_descriptor.clone().unwrap_or_default();
    Ok(Some(WeatherForecast {
        temp_max: tomorrow.temp_max,
        description: tomorrow.short_text.clone().unwrap_or_default(),
        solar: classify_solar(&icon),
    }))
}
