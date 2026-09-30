//! Exchange rates. Everything is stored as "units per 1 EUR" because that is
//! what the ECB publishes; converting A→B goes through EUR.

use crate::money::Cur;
use crate::{Error, Result};
use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub struct Rates {
    per_eur: HashMap<Cur, f64>,
    manual: HashMap<Cur, bool>,
}

/// Rough built-in rates so conversion works offline before the first
/// refresh. The UI labels them as estimates until real rates arrive.
const DEFAULTS: &[(&str, f64)] = &[
    ("EUR", 1.0),
    ("USD", 1.08),
    ("GBP", 0.85),
    ("INR", 90.0),
    ("JPY", 162.0),
    ("CNY", 7.8),
    ("KRW", 1450.0),
    ("AUD", 1.65),
    ("CAD", 1.48),
    ("CHF", 0.95),
    ("SGD", 1.45),
    ("HKD", 8.4),
    ("NZD", 1.8),
    ("SEK", 11.4),
    ("NOK", 11.6),
    ("DKK", 7.46),
    ("PLN", 4.3),
    ("MXN", 19.5),
    ("BRL", 5.9),
    ("ZAR", 20.0),
    ("TRY", 36.0),
    ("THB", 38.0),
    ("IDR", 17500.0),
    ("AED", 3.97),
    ("BTC", 0.000_016),
];

impl Rates {
    pub fn defaults() -> Rates {
        let mut r = Rates::default();
        for &(c, v) in DEFAULTS {
            if let Some(c) = Cur::new(c) {
                r.per_eur.insert(c, v);
            }
        }
        r
    }

    pub fn set(&mut self, c: Cur, per_eur: f64, manual: bool) {
        if per_eur.is_finite() && per_eur > 0.0 {
            self.per_eur.insert(c, per_eur);
            self.manual.insert(c, manual);
        }
    }

    pub fn get(&self, c: Cur) -> Option<f64> {
        self.per_eur.get(&c).copied()
    }

    pub fn is_manual(&self, c: Cur) -> bool {
        self.manual.get(&c).copied().unwrap_or(false)
    }

    /// How many units of `to` one unit of `from` buys.
    pub fn rate(&self, from: Cur, to: Cur) -> Option<f64> {
        if from == to {
            return Some(1.0);
        }
        Some(self.get(to)? / self.get(from)?)
    }

    /// Converts minor units of `from` into minor units of `to`. Unknown
    /// currencies pass through unchanged rather than becoming zero.
    pub fn convert(&self, amount: i64, from: Cur, to: Cur) -> i64 {
        if from == to || amount == 0 {
            return amount;
        }
        match self.rate(from, to) {
            Some(r) => to.from_major(from.to_major(amount) * r),
            None => amount,
        }
    }
}

/// Fetches the latest ECB reference rates from the Frankfurter API
/// (free, no key). Blocking — call it from a background thread.
/// Returns `(rates per EUR, publication date)`.
pub fn fetch_ecb() -> Result<(Vec<(Cur, f64)>, String)> {
    let body = ureq::get("https://api.frankfurter.dev/v1/latest?base=EUR")
        .header("User-Agent", concat!("magpie/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|e| Error::Http(e.to_string()))?
        .body_mut()
        .read_to_string()
        .map_err(|e| Error::Http(e.to_string()))?;
    parse_frankfurter(&body)
}

fn parse_frankfurter(body: &str) -> Result<(Vec<(Cur, f64)>, String)> {
    #[derive(serde::Deserialize)]
    struct Resp {
        date: String,
        rates: HashMap<String, f64>,
    }
    let r: Resp = serde_json::from_str(body)?;
    let mut out: Vec<(Cur, f64)> = r.rates.iter().filter_map(|(k, v)| Some((Cur::new(k)?, *v))).collect();
    out.push((Cur::EUR, 1.0));
    out.sort_by_key(|(c, _)| *c);
    Ok((out, r.date))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_through_eur() {
        let mut r = Rates::default();
        r.set(Cur::EUR, 1.0, false);
        r.set(Cur::USD, 1.25, false);
        let gbp = Cur::new("GBP").unwrap();
        r.set(gbp, 0.8, false);
        assert_eq!(r.convert(10_000, Cur::EUR, Cur::USD), 12_500);
        assert_eq!(r.convert(12_500, Cur::USD, Cur::EUR), 10_000);
        assert_eq!(r.convert(10_000, Cur::USD, gbp), 6_400);
        let jpy = Cur::new("JPY").unwrap();
        assert_eq!(r.convert(100, Cur::USD, jpy), 100, "unknown rate passes through");
        r.set(jpy, 160.0, false);
        assert_eq!(r.convert(125, Cur::USD, jpy), 160);
    }

    #[test]
    fn parses_api_payload() {
        let body = r#"{"amount":1.0,"base":"EUR","date":"2026-09-29","rates":{"USD":1.1,"GBP":0.84}}"#;
        let (rates, date) = parse_frankfurter(body).unwrap();
        assert_eq!(date, "2026-09-29");
        assert_eq!(rates.len(), 3);
    }
}
