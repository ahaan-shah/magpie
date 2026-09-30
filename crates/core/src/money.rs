//! Money is always an `i64` count of a currency's minor unit (cents, satoshi,
//! yen…). Floats only appear when converting between currencies.

use serde::{Deserialize, Serialize};
use std::fmt;

pub struct CurrencyInfo {
    pub code: &'static str,
    pub symbol: &'static str,
    pub name: &'static str,
    /// Number of minor-unit digits (USD 2, JPY 0, BTC 8).
    pub exponent: u32,
}

pub const CURRENCIES: &[CurrencyInfo] = &[
    CurrencyInfo {
        code: "USD",
        symbol: "$",
        name: "US Dollar",
        exponent: 2,
    },
    CurrencyInfo {
        code: "EUR",
        symbol: "€",
        name: "Euro",
        exponent: 2,
    },
    CurrencyInfo {
        code: "GBP",
        symbol: "£",
        name: "British Pound",
        exponent: 2,
    },
    CurrencyInfo {
        code: "INR",
        symbol: "₹",
        name: "Indian Rupee",
        exponent: 2,
    },
    CurrencyInfo {
        code: "JPY",
        symbol: "¥",
        name: "Japanese Yen",
        exponent: 0,
    },
    CurrencyInfo {
        code: "CNY",
        symbol: "CN¥",
        name: "Chinese Yuan",
        exponent: 2,
    },
    CurrencyInfo {
        code: "KRW",
        symbol: "₩",
        name: "South Korean Won",
        exponent: 0,
    },
    CurrencyInfo {
        code: "AUD",
        symbol: "A$",
        name: "Australian Dollar",
        exponent: 2,
    },
    CurrencyInfo {
        code: "CAD",
        symbol: "C$",
        name: "Canadian Dollar",
        exponent: 2,
    },
    CurrencyInfo {
        code: "CHF",
        symbol: "CHF ",
        name: "Swiss Franc",
        exponent: 2,
    },
    CurrencyInfo {
        code: "SGD",
        symbol: "S$",
        name: "Singapore Dollar",
        exponent: 2,
    },
    CurrencyInfo {
        code: "HKD",
        symbol: "HK$",
        name: "Hong Kong Dollar",
        exponent: 2,
    },
    CurrencyInfo {
        code: "NZD",
        symbol: "NZ$",
        name: "New Zealand Dollar",
        exponent: 2,
    },
    CurrencyInfo {
        code: "SEK",
        symbol: "kr ",
        name: "Swedish Krona",
        exponent: 2,
    },
    CurrencyInfo {
        code: "NOK",
        symbol: "kr ",
        name: "Norwegian Krone",
        exponent: 2,
    },
    CurrencyInfo {
        code: "DKK",
        symbol: "kr ",
        name: "Danish Krone",
        exponent: 2,
    },
    CurrencyInfo {
        code: "PLN",
        symbol: "zł ",
        name: "Polish Złoty",
        exponent: 2,
    },
    CurrencyInfo {
        code: "MXN",
        symbol: "MX$",
        name: "Mexican Peso",
        exponent: 2,
    },
    CurrencyInfo {
        code: "BRL",
        symbol: "R$",
        name: "Brazilian Real",
        exponent: 2,
    },
    CurrencyInfo {
        code: "ZAR",
        symbol: "R ",
        name: "South African Rand",
        exponent: 2,
    },
    CurrencyInfo {
        code: "TRY",
        symbol: "₺",
        name: "Turkish Lira",
        exponent: 2,
    },
    CurrencyInfo {
        code: "THB",
        symbol: "฿",
        name: "Thai Baht",
        exponent: 2,
    },
    CurrencyInfo {
        code: "IDR",
        symbol: "Rp ",
        name: "Indonesian Rupiah",
        exponent: 2,
    },
    CurrencyInfo {
        code: "AED",
        symbol: "AED ",
        name: "UAE Dirham",
        exponent: 2,
    },
    CurrencyInfo {
        code: "BTC",
        symbol: "₿",
        name: "Bitcoin",
        exponent: 8,
    },
];

/// A currency, stored as its 3-letter ISO code so it is `Copy` and 3 bytes.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Cur([u8; 3]);

impl Cur {
    pub const USD: Cur = Cur(*b"USD");
    pub const EUR: Cur = Cur(*b"EUR");

    /// Parses a 3-letter code. Unknown but well-formed codes are accepted and
    /// formatted with a 2-digit exponent.
    pub fn new(code: &str) -> Option<Cur> {
        let b = code.trim().as_bytes();
        if b.len() != 3 || !b.iter().all(u8::is_ascii_alphabetic) {
            return None;
        }
        Some(Cur([
            b[0].to_ascii_uppercase(),
            b[1].to_ascii_uppercase(),
            b[2].to_ascii_uppercase(),
        ]))
    }

    pub fn code(&self) -> &str {
        std::str::from_utf8(&self.0).unwrap_or("???")
    }

    pub fn info(&self) -> Option<&'static CurrencyInfo> {
        CURRENCIES.iter().find(|c| c.code.as_bytes() == self.0)
    }

    pub fn symbol(&self) -> &str {
        self.info().map(|i| i.symbol).unwrap_or_else(|| self.code())
    }

    pub fn exponent(&self) -> u32 {
        self.info().map(|i| i.exponent).unwrap_or(2)
    }

    pub fn scale(&self) -> i64 {
        10i64.pow(self.exponent())
    }

    pub fn to_major(&self, minor: i64) -> f64 {
        minor as f64 / self.scale() as f64
    }

    pub fn from_major(&self, major: f64) -> i64 {
        (major * self.scale() as f64).round() as i64
    }

    pub fn all() -> impl Iterator<Item = Cur> {
        CURRENCIES.iter().filter_map(|c| Cur::new(c.code))
    }
}

impl Default for Cur {
    fn default() -> Self {
        Cur::USD
    }
}

impl fmt::Debug for Cur {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl fmt::Display for Cur {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl Serialize for Cur {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.code())
    }
}

impl<'de> Deserialize<'de> for Cur {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Cur::new(&s).ok_or_else(|| serde::de::Error::custom(format!("bad currency {s:?}")))
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FmtOpts {
    /// Always show a leading `+` for positive values.
    pub plus: bool,
    /// Drop the minor digits when they are zero (e.g. `$1,200`).
    pub trim_zero_cents: bool,
    /// Compact large numbers: `$12.3k`, `$1.2M`.
    pub compact: bool,
    /// Omit the currency symbol.
    pub no_symbol: bool,
}

/// Formats `minor` units as a human string such as `-$1,234.50`.
pub fn format(minor: i64, cur: Cur, opts: FmtOpts) -> String {
    let neg = minor < 0;
    let abs = minor.unsigned_abs();
    let exp = cur.exponent();
    let scale = 10u64.pow(exp);
    let sign = if neg {
        "−"
    } else if opts.plus && minor > 0 {
        "+"
    } else {
        ""
    };
    let sym = if opts.no_symbol { "" } else { cur.symbol() };

    if opts.compact {
        let major = abs as f64 / scale as f64;
        let (v, suffix) = if major >= 1e9 {
            (major / 1e9, "B")
        } else if major >= 1e6 {
            (major / 1e6, "M")
        } else if major >= 1e4 {
            (major / 1e3, "k")
        } else {
            (major, "")
        };
        if !suffix.is_empty() {
            let digits = if v >= 100.0 { 0 } else { 1 };
            let mut s = format!("{v:.digits$}");
            if s.ends_with(".0") {
                s.truncate(s.len() - 2);
            }
            return format!("{sign}{sym}{s}{suffix}");
        }
        let whole = abs / scale;
        return format!("{sign}{sym}{}", group(whole));
    }

    let whole = abs / scale;
    let frac = abs % scale;
    let mut out = String::with_capacity(24);
    out.push_str(sign);
    out.push_str(sym);
    out.push_str(&group(whole));
    if exp > 0 && !(opts.trim_zero_cents && frac == 0) {
        // BTC shows 8 digits which is noisy; trim trailing zeros past 2.
        let mut f = format!("{frac:0width$}", width = exp as usize);
        while f.len() > 2 && f.ends_with('0') {
            f.pop();
        }
        out.push('.');
        out.push_str(&f);
    }
    out
}

pub fn fmt(minor: i64, cur: Cur) -> String {
    format(minor, cur, FmtOpts::default())
}

fn group(mut n: u64) -> String {
    if n < 1000 {
        return n.to_string();
    }
    let mut parts = Vec::with_capacity(7);
    while n >= 1000 {
        parts.push(format!("{:03}", n % 1000));
        n /= 1000;
    }
    let mut out = n.to_string();
    for p in parts.iter().rev() {
        out.push(',');
        out.push_str(p);
    }
    out
}

/// Parses user input such as `1,234.5`, `-12`, `$4.99`, `(3.50)` into minor
/// units. Never goes through a float, so `0.1 + 0.2` style errors cannot
/// happen. Extra fractional digits are rounded half-away-from-zero.
pub fn parse(input: &str, cur: Cur) -> Option<i64> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }
    let mut neg = false;
    let mut digits_int = String::new();
    let mut digits_frac = String::new();
    let mut seen_dot = false;
    let mut s = s;
    if s.starts_with('(') && s.ends_with(')') {
        neg = true;
        s = &s[1..s.len() - 1];
    }
    for ch in s.chars() {
        match ch {
            '-' | '−' => neg = !neg,
            '+' => {}
            '0'..='9' => {
                if seen_dot {
                    digits_frac.push(ch)
                } else {
                    digits_int.push(ch)
                }
            }
            '.' => {
                if seen_dot {
                    return None;
                }
                seen_dot = true;
            }
            ',' | '_' | ' ' | '\'' => {}
            c if c.is_alphabetic() || "$€£₹¥₩₿₺฿".contains(c) => {}
            _ => return None,
        }
    }
    if digits_int.is_empty() && digits_frac.is_empty() {
        return None;
    }
    let exp = cur.exponent() as usize;
    let whole: i64 = if digits_int.is_empty() {
        0
    } else {
        digits_int.parse().ok()?
    };
    let mut frac_str: String = digits_frac.chars().take(exp).collect();
    while frac_str.len() < exp {
        frac_str.push('0');
    }
    let mut frac: i64 = if frac_str.is_empty() { 0 } else { frac_str.parse().ok()? };
    // Round on the first dropped digit.
    if let Some(next) = digits_frac.chars().nth(exp) {
        if next >= '5' {
            frac += 1;
        }
    }
    let v = whole.checked_mul(cur.scale())?.checked_add(frac)?;
    Some(if neg { -v } else { v })
}

/// Converts a minor-unit amount into an editable string without grouping,
/// e.g. `123450` USD → `"1234.50"`.
pub fn to_input(minor: i64, cur: Cur) -> String {
    let exp = cur.exponent();
    if exp == 0 {
        return minor.to_string();
    }
    let scale = cur.scale();
    let sign = if minor < 0 { "-" } else { "" };
    let abs = minor.unsigned_abs() as i64;
    format!("{sign}{}.{:0w$}", abs / scale, abs % scale, w = exp as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usd() -> Cur {
        Cur::USD
    }

    #[test]
    fn formats_grouping_and_sign() {
        assert_eq!(fmt(123456789, usd()), "$1,234,567.89");
        assert_eq!(fmt(-97000, usd()), "−$970.00");
        assert_eq!(fmt(5, usd()), "$0.05");
        assert_eq!(fmt(0, usd()), "$0.00");
        let jpy = Cur::new("jpy").unwrap();
        assert_eq!(fmt(1500, jpy), "¥1,500");
    }

    #[test]
    fn formats_options() {
        let o = FmtOpts {
            plus: true,
            ..Default::default()
        };
        assert_eq!(format(1000, usd(), o), "+$10.00");
        let o = FmtOpts {
            trim_zero_cents: true,
            ..Default::default()
        };
        assert_eq!(format(120000, usd(), o), "$1,200");
        assert_eq!(format(120050, usd(), o), "$1,200.50");
        let o = FmtOpts {
            compact: true,
            ..Default::default()
        };
        assert_eq!(format(1_234_500_00, usd(), o), "$1.2M");
        assert_eq!(format(45_600_00, usd(), o), "$45.6k");
        assert_eq!(format(9_999_00, usd(), o), "$9,999");
    }

    #[test]
    fn btc_trims_trailing_zeros() {
        let btc = Cur::new("BTC").unwrap();
        assert_eq!(fmt(150_000_000, btc), "₿1.50");
        assert_eq!(fmt(12_345, btc), "₿0.00012345");
    }

    #[test]
    fn parses_inputs() {
        assert_eq!(parse("1,234.56", usd()), Some(123456));
        assert_eq!(parse("$4.5", usd()), Some(450));
        assert_eq!(parse("-12", usd()), Some(-1200));
        assert_eq!(parse("(3.50)", usd()), Some(-350));
        assert_eq!(parse(".99", usd()), Some(99));
        assert_eq!(parse("0.105", usd()), Some(11));
        assert_eq!(parse("1.2.3", usd()), None);
        assert_eq!(parse("", usd()), None);
        assert_eq!(parse("abc", usd()), None);
        assert_eq!(parse("0.1", Cur::new("BTC").unwrap()), Some(10_000_000));
        assert_eq!(parse("1500", Cur::new("JPY").unwrap()), Some(1500));
    }

    #[test]
    fn input_roundtrip() {
        for v in [0, 1, -1, 99, 100, 123456, -98765] {
            assert_eq!(parse(&to_input(v, usd()), usd()), Some(v));
        }
    }

    #[test]
    fn cur_parsing() {
        assert_eq!(Cur::new("eur"), Some(Cur::EUR));
        assert_eq!(Cur::new("EURO"), None);
        assert_eq!(Cur::new("E1R"), None);
    }
}
