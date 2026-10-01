//! Aggregations over the store. All results are in the base currency and
//! exclude transfers (moving money between your own accounts is neither
//! income nor spending). Everything is a single linear pass over a
//! date-sliced range, so even 100k transactions aggregate in well under a
//! millisecond or two.

use crate::model::*;
use crate::store::Store;
use jiff::civil::Date;
use std::collections::HashMap;

/// Splits a transaction into `(income, expense)` contributions, both usually
/// positive. Refunds in an expense category reduce spending rather than
/// counting as income; uncategorized entries go by sign.
pub fn flow(store: &Store, t: &Txn) -> (i64, i64) {
    if t.is_transfer() {
        return (0, 0);
    }
    let amt = store.txn_base(t);
    match t.category.and_then(|c| store.category(c)).map(|c| c.kind) {
        Some(CategoryKind::Income) => (amt, 0),
        Some(CategoryKind::Expense) => (0, -amt),
        None if amt >= 0 => (amt, 0),
        None => (0, -amt),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Totals {
    pub income: i64,
    pub expense: i64,
    pub count: usize,
}

impl Totals {
    pub fn net(&self) -> i64 {
        self.income - self.expense
    }
    /// Share of income kept, 0..=1 (can be negative when overspending).
    pub fn savings_rate(&self) -> Option<f64> {
        (self.income > 0).then(|| self.net() as f64 / self.income as f64)
    }
}

pub fn totals(store: &Store, from: Date, to: Date) -> Totals {
    let mut out = Totals::default();
    for t in store.txns_between(from, to) {
        let (i, e) = flow(store, t);
        out.income += i;
        out.expense += e;
        if !t.is_transfer() {
            out.count += 1;
        }
    }
    out
}

pub fn month_totals(store: &Store, m: Month) -> Totals {
    totals(store, m.first(), m.last())
}

/// Income and expense per month for the `n` months ending at `last`.
pub fn cashflow(store: &Store, last: Month, n: usize) -> Vec<(Month, Totals)> {
    (0..n as i32)
        .rev()
        .map(|k| last.add(-k))
        .map(|m| (m, month_totals(store, m)))
        .collect()
}

/// Spending per category (expense categories plus uncategorized outflow),
/// largest first. Categories with net refunds (negative spend) are dropped.
pub fn spending_by_category(store: &Store, from: Date, to: Date) -> Vec<(Option<Id>, i64)> {
    let mut map: HashMap<Option<Id>, i64> = HashMap::new();
    for t in store.txns_between(from, to) {
        let (_, e) = flow(store, t);
        if e != 0 {
            *map.entry(t.category).or_default() += e;
        }
    }
    let mut v: Vec<_> = map.into_iter().filter(|(_, v)| *v > 0).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v
}

/// Income per category, largest first.
pub fn income_by_category(store: &Store, from: Date, to: Date) -> Vec<(Option<Id>, i64)> {
    let mut map: HashMap<Option<Id>, i64> = HashMap::new();
    for t in store.txns_between(from, to) {
        let (i, _) = flow(store, t);
        if i != 0 {
            *map.entry(t.category).or_default() += i;
        }
    }
    let mut v: Vec<_> = map.into_iter().filter(|(_, v)| *v > 0).collect();
    v.sort_by_key(|x| std::cmp::Reverse(x.1));
    v
}

/// Spend for one category in a month (base currency).
pub fn category_spent(store: &Store, cat: Id, m: Month) -> i64 {
    store
        .txns_in(m)
        .iter()
        .filter(|t| t.category == Some(cat))
        .map(|t| flow(store, t).1)
        .sum()
}

/// All category spends for a month in one pass.
pub fn category_spent_map(store: &Store, m: Month) -> HashMap<Id, i64> {
    let mut map = HashMap::new();
    for t in store.txns_in(m) {
        if let Some(c) = t.category {
            let e = flow(store, t).1;
            if e != 0 {
                *map.entry(c).or_default() += e;
            }
        }
    }
    map
}

/// Balance of every account in its own currency, including opening balance.
pub fn balances(store: &Store) -> HashMap<Id, i64> {
    let mut map: HashMap<Id, i64> = store.accounts().iter().map(|a| (a.id, a.opening)).collect();
    for t in store.txns() {
        *map.entry(t.account).or_default() += t.amount;
    }
    map
}

/// Balance of every account as of the end of `date`.
pub fn balances_at(store: &Store, date: Date) -> HashMap<Id, i64> {
    let mut map: HashMap<Id, i64> = store.accounts().iter().map(|a| (a.id, a.opening)).collect();
    let end = store.txns().partition_point(|t| t.date <= date);
    for t in &store.txns()[..end] {
        *map.entry(t.account).or_default() += t.amount;
    }
    map
}

/// Net worth across non-archived accounts, in base currency.
pub fn net_worth(store: &Store) -> i64 {
    let b = balances(store);
    store
        .accounts()
        .iter()
        .filter(|a| !a.archived)
        .map(|a| store.to_base(b.get(&a.id).copied().unwrap_or(0), a.currency))
        .sum()
}

/// End-of-month net worth for the `n` months ending at `last`. One pass.
pub fn net_worth_series(store: &Store, last: Month, n: usize) -> Vec<(Month, i64)> {
    let months: Vec<Month> = (0..n as i32).rev().map(|k| last.add(-k)).collect();
    let ends: Vec<Date> = months.iter().map(|m| m.last()).collect();
    months.into_iter().zip(net_worth_at(store, &ends)).collect()
}

/// Net worth at the end of each date in `ends` (ascending). One pass.
pub fn net_worth_at(store: &Store, ends: &[Date]) -> Vec<i64> {
    let mut bal: HashMap<Id, i64> = store.accounts().iter().map(|a| (a.id, a.opening)).collect();
    let txns = store.txns();
    let mut i = 0;
    let mut out = Vec::with_capacity(ends.len());
    for &end in ends {
        while i < txns.len() && txns[i].date <= end {
            *bal.entry(txns[i].account).or_default() += txns[i].amount;
            i += 1;
        }
        let total = store
            .accounts()
            .iter()
            .filter(|a| !a.archived)
            .map(|a| store.to_base(bal.get(&a.id).copied().unwrap_or(0), a.currency))
            .sum();
        out.push(total);
    }
    out
}

/// End-of-day balance for one account for each day in `from..=to` (native
/// currency). Used for the account sparklines.
pub fn account_series(store: &Store, account: Id, from: Date, to: Date) -> Vec<i64> {
    let opening = store.account(account).map(|a| a.opening).unwrap_or(0);
    let txns = store.txns();
    let start = txns.partition_point(|t| t.date < from);
    let mut bal = opening
        + txns[..start]
            .iter()
            .filter(|t| t.account == account)
            .map(|t| t.amount)
            .sum::<i64>();
    let mut out = Vec::new();
    let mut i = start;
    let mut d = from;
    while d <= to {
        while i < txns.len() && txns[i].date <= d {
            if txns[i].account == account {
                bal += txns[i].amount;
            }
            i += 1;
        }
        out.push(bal);
        match d.tomorrow() {
            Ok(n) => d = n,
            Err(_) => break,
        }
    }
    out
}

/// Spending per day in `from..=to` (index 0 = `from`), for heatmaps and
/// budget pacing.
pub fn daily_spend(store: &Store, from: Date, to: Date) -> Vec<i64> {
    let days = (to - from).get_days().max(0) as usize + 1;
    let mut out = vec![0i64; days];
    for t in store.txns_between(from, to) {
        let e = flow(store, t).1;
        if e != 0 {
            let idx = (t.date - from).get_days() as usize;
            if idx < days {
                out[idx] += e;
            }
        }
    }
    out
}

/// Biggest payees by spend.
pub fn top_payees(store: &Store, from: Date, to: Date, n: usize) -> Vec<(String, i64, usize)> {
    let mut map: HashMap<String, (String, i64, usize)> = HashMap::new();
    for t in store.txns_between(from, to) {
        let e = flow(store, t).1;
        if e > 0 && !t.payee.is_empty() {
            let entry = map
                .entry(t.payee.to_lowercase())
                .or_insert_with(|| (t.payee.clone(), 0, 0));
            entry.1 += e;
            entry.2 += 1;
        }
    }
    let mut v: Vec<_> = map.into_values().collect();
    v.sort_by_key(|x| std::cmp::Reverse(x.1));
    v.truncate(n);
    v
}

/// Monthly spend per category for a stacked chart: returns the months and,
/// for each of the top `k` categories (others folded into `None`), a series.
pub type Series = Vec<(Option<Id>, Vec<i64>)>;

pub fn category_trend(store: &Store, last: Month, n: usize, k: usize) -> (Vec<Month>, Series) {
    let months: Vec<Month> = (0..n as i32).rev().map(|i| last.add(-i)).collect();
    let spans: Vec<(Date, Date)> = months.iter().map(|m| (m.first(), m.last())).collect();
    (months, category_trend_spans(store, &spans, k))
}

/// Spend per category for each `(from, to)` span (ascending, inclusive), for
/// the top `k` categories over the whole range plus everything else as `None`.
pub fn category_trend_spans(store: &Store, spans: &[(Date, Date)], k: usize) -> Series {
    let (Some(first), Some(last)) = (spans.first(), spans.last()) else {
        return Vec::new();
    };
    let n = spans.len();
    let totals = spending_by_category(store, first.0, last.1);
    let top: Vec<Option<Id>> = totals
        .iter()
        .filter(|(c, _)| c.is_some())
        .take(k)
        .map(|(c, _)| *c)
        .collect();
    let mut series: Series = top.iter().map(|c| (*c, vec![0; n])).collect();
    series.push((None, vec![0; n]));
    let other = series.len() - 1;
    for (si_span, (from, to)) in spans.iter().enumerate() {
        for t in store.txns_between(*from, *to) {
            let e = flow(store, t).1;
            if e == 0 {
                continue;
            }
            let si = top.iter().position(|c| *c == t.category).unwrap_or(other);
            series[si].1[si_span] += e;
        }
    }
    if series[other].1.iter().all(|v| *v == 0) {
        series.pop();
    }
    series
}

/// What the user usually does with a payee: last category and account, and
/// how often it's been used. Drives autocomplete and category suggestions.
#[derive(Clone, Debug)]
pub struct PayeeInfo {
    pub name: String,
    pub category: Option<Id>,
    pub account: Id,
    pub last_amount: i64,
    pub count: usize,
}

pub fn payee_index(store: &Store) -> Vec<PayeeInfo> {
    let mut map: HashMap<String, PayeeInfo> = HashMap::new();
    for t in store.txns() {
        if t.payee.is_empty() || t.is_transfer() {
            continue;
        }
        let e = map.entry(t.payee.to_lowercase()).or_insert_with(|| PayeeInfo {
            name: t.payee.clone(),
            category: None,
            account: t.account,
            last_amount: 0,
            count: 0,
        });
        // Later transactions win: txns are sorted oldest first.
        e.name = t.payee.clone();
        e.category = t.category.or(e.category);
        e.account = t.account;
        e.last_amount = t.amount;
        e.count += 1;
    }
    let mut v: Vec<_> = map.into_values().collect();
    v.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
    v
}

/// The "month in review" card.
#[derive(Clone, Debug, Default)]
pub struct Review {
    pub month: Option<Month>,
    pub totals: Totals,
    pub prev: Totals,
    /// Category whose spend changed the most vs. the previous month.
    pub biggest_change: Option<(Option<Id>, i64)>,
    pub top_payee: Option<(String, i64)>,
    /// Days in the month with zero spending.
    pub no_spend_days: usize,
}

pub fn review(store: &Store, m: Month) -> Review {
    let cur = spending_by_category(store, m.first(), m.last());
    let prev = spending_by_category(store, m.prev().first(), m.prev().last());
    let prev_map: HashMap<_, _> = prev.into_iter().collect();
    let mut changes: HashMap<Option<Id>, i64> = HashMap::new();
    for (c, v) in &cur {
        changes.insert(*c, v - prev_map.get(c).copied().unwrap_or(0));
    }
    for (c, v) in &prev_map {
        changes.entry(*c).or_insert(-v);
    }
    let biggest_change = changes
        .into_iter()
        .max_by_key(|(_, v)| v.abs())
        .filter(|(_, v)| *v != 0);
    let days = daily_spend(store, m.first(), m.last());
    let today = today();
    let upto = if m.contains(today) {
        today.day() as usize
    } else {
        days.len()
    };
    Review {
        month: Some(m),
        totals: month_totals(store, m),
        prev: month_totals(store, m.prev()),
        biggest_change,
        top_payee: top_payees(store, m.first(), m.last(), 1)
            .into_iter()
            .next()
            .map(|(n, v, _)| (n, v)),
        no_spend_days: days.iter().take(upto).filter(|v| **v == 0).count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::{fixture, txn};
    use jiff::civil::date;

    #[test]
    fn totals_and_refunds() {
        let mut s = fixture();
        s.add_txn(txn(&s, date(2026, 5, 1), -2000, "Food")).unwrap();
        s.add_txn(txn(&s, date(2026, 5, 2), 500, "Food")).unwrap(); // refund
        s.add_txn(txn(&s, date(2026, 5, 3), 100_000, "Salary")).unwrap();
        s.add_txn(txn(&s, date(2026, 5, 4), -300, "Nope")).unwrap(); // uncategorized
        let t = month_totals(&s, Month { year: 2026, month: 5 });
        assert_eq!(t.income, 100_000);
        assert_eq!(t.expense, 1800);
        assert_eq!(t.net(), 98_200);
        let cats = spending_by_category(&s, date(2026, 5, 1), date(2026, 5, 31));
        assert_eq!(cats[0].1, 1500);
        assert_eq!(cats[1], (None, 300));
    }

    #[test]
    fn balances_and_series() {
        let mut s = fixture();
        s.add_txn(txn(&s, date(2026, 1, 15), -10_000, "Food")).unwrap();
        s.add_txn(txn(&s, date(2026, 2, 15), 50_000, "Salary")).unwrap();
        let id = s.accounts()[0].id;
        assert_eq!(balances(&s)[&id], 100_000 - 10_000 + 50_000);
        let series = net_worth_series(&s, Month { year: 2026, month: 2 }, 3);
        assert_eq!(
            series.iter().map(|x| x.1).collect::<Vec<_>>(),
            vec![100_000, 90_000, 140_000]
        );
        let days = account_series(&s, id, date(2026, 1, 14), date(2026, 1, 16));
        assert_eq!(days, vec![100_000, 90_000, 90_000]);
    }

    #[test]
    fn transfers_are_neutral() {
        let mut s = fixture();
        let other = s
            .save_account(Account {
                id: 0,
                name: "B".into(),
                kind: AccountKind::Savings,
                currency: crate::Cur::USD,
                opening: 0,
                color: 0,
                archived: false,
                sort: 0,
            })
            .unwrap();
        let a = s.accounts()[0].id;
        s.add_transfer(a, other, date(2026, 1, 1), 5000, None, "").unwrap();
        let t = month_totals(&s, Month { year: 2026, month: 1 });
        assert_eq!((t.income, t.expense), (0, 0));
        assert_eq!(net_worth(&s), 100_000);
    }
}
