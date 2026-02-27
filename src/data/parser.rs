use std::collections::HashMap;

pub struct PriceRecord {
    pub region: String,
    pub price: f64,
    pub interval_time: String,
}

pub struct ForecastRecord {
    pub region: String,
    pub forecast_time: String,
    pub price: f64,
}

/// Parse AEMO dispatch CSV. Uses the I-row to dynamically find column positions.
pub fn parse_dispatch(csv: &str) -> Vec<PriceRecord> {
    let mut col_map: HashMap<&str, usize> = HashMap::new();
    let mut records = Vec::new();

    for line in csv.lines() {
        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() < 3 {
            continue;
        }
        let tag = fields[0].trim();
        let table = fields[1].trim();
        let sub = fields[2].trim();

        if tag == "I" && table == "DISPATCH" && sub == "PRICE" {
            col_map.clear();
            for (i, f) in fields.iter().enumerate() {
                col_map.insert(f.trim().trim_matches('"'), i);
            }
        }

        if tag == "D" && table == "DISPATCH" && sub == "PRICE" {
            let ri = col_map.get("REGIONID").copied();
            let pi = col_map.get("RRP").copied();
            let ti = col_map.get("SETTLEMENTDATE").copied();
            if let (Some(ri), Some(pi), Some(ti)) = (ri, pi, ti) {
                if ri < fields.len() && pi < fields.len() && ti < fields.len() {
                    let region = fields[ri].trim().trim_matches('"').to_string();
                    let price: f64 = fields[pi].trim().trim_matches('"').parse().unwrap_or(f64::NAN);
                    let time = fields[ti].trim().trim_matches('"').to_string();
                    if price.is_finite() && price >= -1000.0 && price <= 17500.0 {
                        records.push(PriceRecord { region, price, interval_time: time });
                    }
                }
            }
        }
    }
    records
}

/// Parse AEMO pre-dispatch CSV.
pub fn parse_predispatch(csv: &str) -> Vec<ForecastRecord> {
    let mut col_map: HashMap<&str, usize> = HashMap::new();
    let mut records = Vec::new();

    for line in csv.lines() {
        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() < 3 {
            continue;
        }
        let tag = fields[0].trim();
        let table = fields[1].trim();

        // Match both PREDISPATCH,REGION_PRICES and PREDISPATCH,PRICE
        let is_pred_row = table == "PREDISPATCH"
            && (fields[2].trim() == "PRICE" || fields[2].trim() == "REGION_PRICES");

        if tag == "I" && is_pred_row {
            col_map.clear();
            for (i, f) in fields.iter().enumerate() {
                col_map.insert(f.trim().trim_matches('"'), i);
            }
        }

        if tag == "D" && is_pred_row {
            let ri = col_map.get("REGIONID").copied();
            let pi = col_map.get("RRP").copied();
            // AEMO uses DATETIME or PERIODID for forecast time
            let ti = col_map.get("DATETIME").or(col_map.get("PERIODID")).copied();
            if let (Some(ri), Some(pi), Some(ti)) = (ri, pi, ti) {
                if ri < fields.len() && pi < fields.len() && ti < fields.len() {
                    let region = fields[ri].trim().trim_matches('"').to_string();
                    let price: f64 = fields[pi].trim().trim_matches('"').parse().unwrap_or(f64::NAN);
                    let time = fields[ti].trim().trim_matches('"').to_string();
                    if price.is_finite() && price >= -1000.0 && price <= 17500.0 {
                        records.push(ForecastRecord { region, price, forecast_time: time });
                    }
                }
            }
        }
    }
    records
}
