//! Natural-language quick add:
//!
//! ```text
//! coffee 4.50 #treats yesterday          -> $4.50 expense, payee "Coffee"
//! +3200 @Acme salary /income             -> $3,200 income from "Acme"
//! 42 uber /transport ~amex sep 3         -> dated, categorized, on the Amex account
//! ```
//!
//! * a number is the amount (an expense unless prefixed with `+`)
//! * `#tag` adds a tag, `@payee` sets the payee, `/category` and `~account`
//!   pick those by name (use `-` or `_` for spaces)
//! * `today`, `yesterday`, weekday names, `3d` (3 days ago), `2026-09-01`,
//!   `sep 3` / `3 sep` set the date
//! * any other words become the payee (or the note when `@payee` is given)

use crate::money::{self, Cur};
use jiff::ToSpan;
use jiff::civil::{Date, Weekday};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct QuickEntry {
    /// Signed amount in minor units (negative = expense).
    pub amount: Option<i64>,
    pub payee: String,
    pub note: String,
    pub tags: Vec<String>,
    pub date: Option<Date>,
    pub category: Option<String>,
    pub account: Option<String>,
}

fn month_of(tok: &str) -> Option<i8> {
    let t = tok.to_ascii_lowercase();
    if t.len() < 3 {
        return None;
    }
    crate::model::MONTH_NAMES
        .iter()
        .position(|full| full.to_ascii_lowercase().starts_with(&t))
        .map(|i| i as i8 + 1)
}

fn weekday_of(tok: &str) -> Option<Weekday> {
    let t = tok.to_ascii_lowercase();
    let days = [
        ("monday", Weekday::Monday),
        ("tuesday", Weekday::Tuesday),
        ("wednesday", Weekday::Wednesday),
        ("thursday", Weekday::Thursday),
        ("friday", Weekday::Friday),
        ("saturday", Weekday::Saturday),
        ("sunday", Weekday::Sunday),
    ];
    if t.len() < 3 {
        return None;
    }
    days.iter()
        .find(|(name, _)| name.starts_with(&t))
        .map(|(_, w)| *w)
}

fn most_recent(today: Date, w: Weekday) -> Date {
    let back = (today.weekday().to_monday_zero_offset() - w.to_monday_zero_offset()).rem_euclid(7);
    today - (back as i64).days()
}

/// A date in the past year for `month`/`day` (e.g. "dec 30" typed in January
/// means last December).
fn recent_md(today: Date, month: i8, day: i8) -> Option<Date> {
    let d = Date::new(today.year(), month, day).ok()?;
    if d > today {
        Date::new(today.year() - 1, month, day).ok()
    } else {
        Some(d)
    }
}

fn looks_numeric(tok: &str) -> bool {
    let t = tok.trim_start_matches(['+', '-', '$', '€', '£', '₹', '¥']);
    !t.is_empty()
        && t.chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit() || c == '.')
        && t.chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == ',')
}

pub fn parse(input: &str, cur: Cur, today: Date) -> QuickEntry {
    let mut e = QuickEntry::default();
    let mut words: Vec<&str> = Vec::new();
    let toks: Vec<&str> = input.split_whitespace().collect();
    let mut i = 0;
    let unslug = |s: &str| s.replace(['-', '_'], " ");
    while i < toks.len() {
        let tok = toks[i];
        let lower = tok.to_ascii_lowercase();
        let next = toks.get(i + 1).copied();
        if let Some(t) = tok.strip_prefix('#').filter(|t| !t.is_empty()) {
            e.tags.push(t.to_lowercase());
        } else if let Some(p) = tok.strip_prefix('@').filter(|t| !t.is_empty()) {
            e.payee = unslug(p);
        } else if let Some(c) = tok.strip_prefix('/').filter(|t| !t.is_empty()) {
            e.category = Some(unslug(c));
        } else if let Some(a) = tok.strip_prefix('~').filter(|t| !t.is_empty()) {
            e.account = Some(unslug(a));
        } else if e.amount.is_none() && looks_numeric(tok) {
            let income = tok.starts_with('+');
            if let Some(v) = money::parse(tok.trim_start_matches(['+', '-']), cur) {
                e.amount = Some(if income { v } else { -v });
            } else {
                words.push(tok);
            }
        } else if e.date.is_none() && lower == "today" {
            e.date = Some(today);
        } else if e.date.is_none() && lower == "yesterday" {
            e.date = Some(today - 1.day());
        } else if e.date.is_none()
            && lower.len() >= 2
            && lower.ends_with('d')
            && lower[..lower.len() - 1]
                .parse::<i64>()
                .is_ok_and(|n| n < 1000)
        {
            let n: i64 = lower[..lower.len() - 1].parse().unwrap_or(0);
            e.date = Some(today - n.days());
        } else if e.date.is_none() && lower.len() == 10 && lower.parse::<Date>().is_ok() {
            e.date = lower.parse().ok();
        } else if e.date.is_none()
            && month_of(tok).is_some()
            && next.and_then(|n| n.parse::<i8>().ok()).is_some()
        {
            let d = next.and_then(|n| n.parse::<i8>().ok()).unwrap_or(1);
            e.date = recent_md(today, month_of(tok).unwrap_or(1), d);
            i += 1;
        } else if e.date.is_none()
            && tok.parse::<i8>().is_ok_and(|d| (1..=31).contains(&d))
            && next.and_then(month_of).is_some()
            && e.amount.is_some()
        {
            e.date = recent_md(
                today,
                next.and_then(month_of).unwrap_or(1),
                tok.parse().unwrap_or(1),
            );
            i += 1;
        } else if e.date.is_none() && weekday_of(tok).is_some() && lower.len() >= 3 {
            e.date = weekday_of(tok).map(|w| most_recent(today, w));
        } else {
            words.push(tok);
        }
        i += 1;
    }
    let text = words.join(" ");
    let text = capitalize(&text);
    if e.payee.is_empty() {
        e.payee = text;
    } else {
        e.note = text;
    }
    e.payee = capitalize(&e.payee);
    e
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    const TODAY: Date = date(2026, 9, 30); // a Wednesday

    #[test]
    fn basic_expense() {
        let e = parse("coffee 4.50 #treats yesterday", Cur::USD, TODAY);
        assert_eq!(e.amount, Some(-450));
        assert_eq!(e.payee, "Coffee");
        assert_eq!(e.tags, vec!["treats"]);
        assert_eq!(e.date, Some(date(2026, 9, 29)));
    }

    #[test]
    fn income_with_payee_and_category() {
        let e = parse("+3,200 @acme september salary /income", Cur::USD, TODAY);
        assert_eq!(e.amount, Some(320_000));
        assert_eq!(e.payee, "Acme");
        assert_eq!(e.note, "September salary");
        assert_eq!(e.category.as_deref(), Some("income"));
    }

    #[test]
    fn dates() {
        assert_eq!(
            parse("x 1 mon", Cur::USD, TODAY).date,
            Some(date(2026, 9, 28))
        );
        assert_eq!(parse("x 1 wed", Cur::USD, TODAY).date, Some(TODAY));
        assert_eq!(
            parse("x 1 3d", Cur::USD, TODAY).date,
            Some(date(2026, 9, 27))
        );
        assert_eq!(
            parse("x 1 sep 3", Cur::USD, TODAY).date,
            Some(date(2026, 9, 3))
        );
        assert_eq!(
            parse("x 1 dec 24", Cur::USD, TODAY).date,
            Some(date(2025, 12, 24))
        );
        assert_eq!(
            parse("x 12 3 sep", Cur::USD, TODAY).date,
            Some(date(2026, 9, 3))
        );
        assert_eq!(
            parse("x 1 2026-01-02", Cur::USD, TODAY).date,
            Some(date(2026, 1, 2))
        );
    }

    #[test]
    fn account_and_multiword_slugs() {
        let e = parse("42 uber /eating-out ~amex_gold", Cur::USD, TODAY);
        assert_eq!(e.category.as_deref(), Some("eating out"));
        assert_eq!(e.account.as_deref(), Some("amex gold"));
        assert_eq!(e.payee, "Uber");
    }

    #[test]
    fn words_that_look_like_months_are_kept() {
        let e = parse("march madness tickets 80", Cur::USD, TODAY);
        assert_eq!(e.payee, "March madness tickets");
        assert_eq!(e.amount, Some(-8000));
    }
}
