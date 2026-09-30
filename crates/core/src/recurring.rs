//! Recurring transactions (rent, salary, subscriptions).
//!
//! The n-th occurrence is always computed from the rule's start date, so a
//! rule starting on Jan 31 lands on Feb 28/29, Mar 31, Apr 30… without
//! drifting to the 28th forever.

use crate::Result;
use crate::model::*;
use crate::store::Store;
use jiff::Span;
use jiff::civil::Date;

/// Date of occurrence `n` (0-based), or `None` if it overflows.
pub fn occurrence(rule: &RecurringRule, n: i64) -> Option<Date> {
    let k = n.checked_mul(rule.interval.max(1))?;
    let span = match rule.freq {
        Freq::Daily => Span::new().try_days(k).ok()?,
        Freq::Weekly => Span::new().try_weeks(k).ok()?,
        Freq::Monthly => Span::new().try_months(k).ok()?,
        Freq::Yearly => Span::new().try_years(k).ok()?,
    };
    let d = rule.start.checked_add(span).ok()?;
    match rule.end {
        Some(end) if d > end => None,
        _ => Some(d),
    }
}

/// The next date this rule will post, if any.
pub fn next_due(rule: &RecurringRule) -> Option<Date> {
    if !rule.active {
        return None;
    }
    occurrence(rule, rule.posted)
}

/// Upcoming occurrences of all active rules in `from..=to`, soonest first.
pub fn upcoming(store: &Store, from: Date, to: Date) -> Vec<(Id, Date)> {
    let mut out = Vec::new();
    for r in store.rules().iter().filter(|r| r.active) {
        let mut n = r.posted;
        while let Some(d) = occurrence(r, n) {
            if d > to {
                break;
            }
            if d >= from || n == r.posted {
                out.push((r.id, d));
            }
            n += 1;
            if out.len() > 500 {
                break;
            }
        }
    }
    out.sort_by_key(|(id, d)| (*d, *id));
    out
}

/// Rules that need confirming (not auto-posted) and are due on or before `today`.
pub fn awaiting_confirmation(store: &Store, today: Date) -> Vec<(Id, Date)> {
    store
        .rules()
        .iter()
        .filter(|r| r.active && !r.auto_post)
        .filter_map(|r| next_due(r).filter(|d| *d <= today).map(|d| (r.id, d)))
        .collect()
}

/// Monthly cost of a rule, normalized to a 30.44-day month — for "your
/// subscriptions cost $X/month".
pub fn monthly_equivalent(rule: &RecurringRule) -> i64 {
    let per_year = match rule.freq {
        Freq::Daily => 365.25,
        Freq::Weekly => 52.18,
        Freq::Monthly => 12.0,
        Freq::Yearly => 1.0,
    } / rule.interval.max(1) as f64;
    (rule.amount as f64 * per_year / 12.0).round() as i64
}

fn make_txn(rule: &RecurringRule, date: Date) -> Txn {
    Txn {
        id: 0,
        account: rule.account,
        date,
        amount: rule.amount,
        payee: rule.payee.clone(),
        category: rule.category,
        note: rule.note.clone(),
        tags: Vec::new(),
        transfer: None,
        recurring: Some(rule.id),
        cleared: false,
    }
}

/// Posts every missed occurrence of auto-post rules up to and including
/// `today`. Safe to call on every launch. Returns the number posted.
pub fn post_due(store: &mut Store, today: Date) -> Result<usize> {
    let mut to_post = Vec::new();
    let mut updated = Vec::new();
    for r in store.rules().iter().filter(|r| r.active && r.auto_post) {
        let mut n = r.posted;
        while let Some(d) = occurrence(r, n) {
            if d > today {
                break;
            }
            to_post.push(make_txn(r, d));
            n += 1;
        }
        if n != r.posted {
            let mut r2 = r.clone();
            r2.posted = n;
            if occurrence(&r2, n).is_none() {
                r2.active = false;
            }
            updated.push(r2);
        }
    }
    let count = to_post.len();
    if count > 0 {
        store.raw_insert_many(to_post)?;
    }
    for r in updated {
        store.save_rule(r)?;
    }
    Ok(count)
}

/// Posts the next occurrence of one rule now (for confirm-before-post rules).
pub fn post_next(store: &mut Store, rule_id: Id) -> Result<Option<Id>> {
    let Some(rule) = store.rule(rule_id).cloned() else {
        return Ok(None);
    };
    let Some(date) = next_due(&rule) else {
        return Ok(None);
    };
    let id = store.add_txn(make_txn(&rule, date))?;
    advance(store, rule)?;
    Ok(Some(id))
}

/// Skips the next occurrence without posting it.
pub fn skip_next(store: &mut Store, rule_id: Id) -> Result<()> {
    if let Some(rule) = store.rule(rule_id).cloned() {
        advance(store, rule)?;
    }
    Ok(())
}

fn advance(store: &mut Store, mut rule: RecurringRule) -> Result<()> {
    rule.posted += 1;
    if occurrence(&rule, rule.posted).is_none() {
        rule.active = false;
    }
    store.save_rule(rule)?;
    Ok(())
}

pub fn describe(rule: &RecurringRule) -> String {
    let n = rule.interval.max(1);
    if n == 1 {
        match rule.freq {
            Freq::Daily => "Every day".into(),
            Freq::Weekly => format!("Every {}", weekday_name(rule.start)),
            Freq::Monthly => format!("Monthly on the {}", ordinal(rule.start.day() as i64)),
            Freq::Yearly => format!(
                "Yearly on {} {}",
                &MONTH_NAMES[rule.start.month() as usize - 1][..3],
                rule.start.day()
            ),
        }
    } else {
        format!("Every {n} {}", rule.freq.unit(n))
    }
}

fn weekday_name(d: Date) -> &'static str {
    use jiff::civil::Weekday::*;
    match d.weekday() {
        Monday => "Monday",
        Tuesday => "Tuesday",
        Wednesday => "Wednesday",
        Thursday => "Thursday",
        Friday => "Friday",
        Saturday => "Saturday",
        Sunday => "Sunday",
    }
}

pub fn ordinal(n: i64) -> String {
    let suffix = match (n % 10, n % 100) {
        (1, x) if x != 11 => "st",
        (2, x) if x != 12 => "nd",
        (3, x) if x != 13 => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::fixture;
    use jiff::civil::date;

    fn rule(start: Date, freq: Freq) -> RecurringRule {
        RecurringRule {
            id: 0,
            payee: "Rent".into(),
            account: 1,
            category: None,
            amount: -150_000,
            note: String::new(),
            freq,
            interval: 1,
            start,
            end: None,
            posted: 0,
            auto_post: true,
            active: true,
        }
    }

    #[test]
    fn month_end_does_not_drift() {
        let r = rule(date(2024, 1, 31), Freq::Monthly);
        let ds: Vec<_> = (0..4).map(|n| occurrence(&r, n).unwrap()).collect();
        assert_eq!(
            ds,
            vec![
                date(2024, 1, 31),
                date(2024, 2, 29),
                date(2024, 3, 31),
                date(2024, 4, 30)
            ]
        );
    }

    #[test]
    fn leap_day_yearly() {
        let r = rule(date(2024, 2, 29), Freq::Yearly);
        assert_eq!(occurrence(&r, 1), Some(date(2025, 2, 28)));
        assert_eq!(occurrence(&r, 4), Some(date(2028, 2, 29)));
    }

    #[test]
    fn end_date_and_interval() {
        let mut r = rule(date(2026, 1, 1), Freq::Weekly);
        r.interval = 2;
        r.end = Some(date(2026, 1, 29));
        assert_eq!(occurrence(&r, 2), Some(date(2026, 1, 29)));
        assert_eq!(occurrence(&r, 3), None);
    }

    #[test]
    fn posts_missed_occurrences_once() {
        let mut s = fixture();
        let mut r = rule(date(2026, 1, 15), Freq::Monthly);
        r.account = s.accounts()[0].id;
        s.save_rule(r).unwrap();
        assert_eq!(post_due(&mut s, date(2026, 4, 20)).unwrap(), 4);
        assert_eq!(post_due(&mut s, date(2026, 4, 20)).unwrap(), 0);
        assert_eq!(s.txns().len(), 4);
        assert_eq!(next_due(&s.rules()[0]), Some(date(2026, 5, 15)));
        assert_eq!(monthly_equivalent(&s.rules()[0]), -150_000);
    }

    #[test]
    fn describes() {
        assert_eq!(
            describe(&rule(date(2026, 1, 3), Freq::Monthly)),
            "Monthly on the 3rd"
        );
        assert_eq!(ordinal(11), "11th");
        assert_eq!(ordinal(22), "22nd");
    }
}
