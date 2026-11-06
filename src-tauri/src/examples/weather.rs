//! Example: weather from Open-Meteo (free, no key, no account). Current
//! conditions, today's high and low, and the next 12 hours.
//!
//! Set your location in LAT / LON below.

use serde::Serialize;
use serde_json::Value;

/// Where to get weather for. Change to your location.
pub const LAT: f64 = 41.88;
pub const LON: f64 = -87.63;

#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Hour {
    /// Local time, "2026-11-06T14:00".
    pub time: String,
    pub temp: i64,
    pub precip_chance: i64,
    pub kind: &'static str,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Weather {
    pub temp: i64,
    pub feels_like: i64,
    pub high: i64,
    pub low: i64,
    /// "clear" | "partly" | "cloudy" | "fog" | "drizzle" | "rain" | "snow" | "storm"
    pub kind: &'static str,
    pub label: &'static str,
    pub is_day: bool,
    pub hours: Vec<Hour>,
}

/// WMO weather code -> (kind, label).
pub fn describe(code: i64) -> (&'static str, &'static str) {
    match code {
        0 => ("clear", "Clear"),
        1 | 2 => ("partly", "Partly cloudy"),
        3 => ("cloudy", "Cloudy"),
        45 | 48 => ("fog", "Fog"),
        51..=57 => ("drizzle", "Drizzle"),
        61..=67 | 80..=82 => ("rain", "Rain"),
        71..=77 | 85 | 86 => ("snow", "Snow"),
        95..=99 => ("storm", "Thunderstorms"),
        _ => ("cloudy", "Cloudy"),
    }
}

pub fn parse(v: &Value) -> Option<Weather> {
    let (c, d, h) = (&v["current"], &v["daily"], &v["hourly"]);
    let r = |x: &Value| x.as_f64().map(|f| f.round() as i64);
    let (kind, label) = describe(c["weather_code"].as_i64()?);
    let now_hour = c["time"].as_str().unwrap_or("").get(..13).unwrap_or("").to_string();
    let times = h["time"].as_array()?;
    let start = times.iter().position(|t| t.as_str().is_some_and(|t| t.starts_with(&now_hour))).unwrap_or(0);
    let hours = (start..(start + 12).min(times.len()))
        .filter_map(|i| {
            Some(Hour {
                time: times[i].as_str()?.to_string(),
                temp: r(&h["temperature_2m"][i])?,
                precip_chance: r(&h["precipitation_probability"][i]).unwrap_or(0),
                kind: describe(h["weather_code"][i].as_i64().unwrap_or(3)).0,
            })
        })
        .collect();
    Some(Weather {
        temp: r(&c["temperature_2m"])?,
        feels_like: r(&c["apparent_temperature"]).unwrap_or(0),
        high: r(&d["temperature_2m_max"][0]).unwrap_or(0),
        low: r(&d["temperature_2m_min"][0]).unwrap_or(0),
        kind,
        label,
        is_day: c["is_day"].as_i64() == Some(1),
        hours,
    })
}

pub fn fetch() -> Result<Weather, String> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={LAT}&longitude={LON}\
         &current=temperature_2m,apparent_temperature,weather_code,is_day\
         &hourly=temperature_2m,precipitation_probability,weather_code\
         &daily=temperature_2m_max,temperature_2m_min\
         &temperature_unit=fahrenheit&timezone=auto&forecast_days=2"
    );
    let v: Value = ureq::get(&url).call().map_err(|e| e.to_string())?.body_mut().read_json().map_err(|e| e.to_string())?;
    parse(&v).ok_or_else(|| "unexpected weather response".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_open_meteo() {
        let v: Value = serde_json::from_str(r#"{
          "current":{"time":"2026-11-06T14:15","temperature_2m":47.4,"apparent_temperature":38.9,"weather_code":3,"is_day":1},
          "hourly":{"time":["2026-11-06T13:00","2026-11-06T14:00","2026-11-06T15:00"],"temperature_2m":[46,47,48],"precipitation_probability":[0,5,40],"weather_code":[3,3,61]},
          "daily":{"temperature_2m_max":[52],"temperature_2m_min":[39]}}"#).unwrap();
        let w = parse(&v).unwrap();
        assert_eq!((w.temp, w.high, w.low, w.kind, w.hours.len()), (47, 52, 39, "cloudy", 2));
        assert_eq!((w.hours[1].kind, w.hours[1].precip_chance), ("rain", 40));
    }
}
