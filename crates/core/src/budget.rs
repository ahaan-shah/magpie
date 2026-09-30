//! Envelope-style monthly budgets per category, with optional rollover.

use crate::analytics::{category_spent, category_spent_map};
use crate::model::*;
use crate::store::Store;
use jiff::civil::Date;

#[derive(Clone, Debug, PartialEq)]
pub struct BudgetLine {
    pub category: Id,
    /// Budgeted for this month (override or the default plan).
    pub planned: i64,
    /// Unspent money carried in from previous months when rollover is on.
    pub carry: i64,
    pub spent: i64,
    pub rollover: bool,
}

impl BudgetLine {
    pub fn available(&self) -> i64 {
        self.planned + self.carry
    }
    pub fn remaining(&self) -> i64 {
        self.available() - self.spent
    }
    /// Fraction of the envelope used (can exceed 1).
    pub fn used(&self) -> f32 {
        let a = self.available();
        if a <= 0 {
            return if self.spent > 0 { 1.5 } else { 0.0 };
        }
        self.spent as f32 / a as f32
    }
}

/// The budgeted amount for a category in a month, if it has a plan.
pub fn planned(store: &Store, cat: Id, m: Month) -> Option<i64> {
    let plan = store.budget_plan(cat)?;
    Some(store.budget_override(cat, m).unwrap_or(plan.amount))
}

/// How far through month `m` we are on `today`: 0..=1 (1 for past months).
pub fn month_progress(m: Month, today: Date) -> f32 {
    let (first, last) = (m.first(), m.last());
    if today < first {
        0.0
    } else if today > last {
        1.0
    } else {
        today.day() as f32 / m.days() as f32
    }
}

const MAX_ROLLOVER_MONTHS: i32 = 24;

fn carry(store: &Store, cat: Id, m: Month) -> i64 {
    let Some(first) = store.txns().first().map(|t| Month::of(t.date)) else {
        return 0;
    };
    let mut start = m.add(-MAX_ROLLOVER_MONTHS);
    if start < first {
        start = first;
    }
    let mut c = 0;
    let mut k = start;
    while k < m {
        // Unspent money rolls forward; overspending is absorbed in its own
        // month rather than haunting every month after it.
        if let Some(p) = planned(store, cat, k) {
            c = (c + p - category_spent(store, cat, k)).max(0);
        }
        k = k.next();
    }
    c
}

/// Every budgeted category for month `m`, in category order.
pub fn month_budget(store: &Store, m: Month) -> Vec<BudgetLine> {
    let spent = category_spent_map(store, m);
    let mut lines: Vec<BudgetLine> = store
        .categories()
        .iter()
        .filter_map(|c| {
            let plan = store.budget_plan(c.id)?;
            let planned = planned(store, c.id, m)?;
            Some(BudgetLine {
                category: c.id,
                planned,
                carry: if plan.rollover { carry(store, c.id, m) } else { 0 },
                spent: spent.get(&c.id).copied().unwrap_or(0),
                rollover: plan.rollover,
            })
        })
        .collect();
    lines.sort_by_key(|l| std::cmp::Reverse(l.available()));
    lines
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BudgetSummary {
    pub available: i64,
    pub spent: i64,
    pub over_count: usize,
}

impl BudgetSummary {
    pub fn remaining(&self) -> i64 {
        self.available - self.spent
    }
}

pub fn summarize(lines: &[BudgetLine]) -> BudgetSummary {
    BudgetSummary {
        available: lines.iter().map(BudgetLine::available).sum(),
        spent: lines.iter().map(|l| l.spent).sum(),
        over_count: lines.iter().filter(|l| l.remaining() < 0).count(),
    }
}

/// Average monthly spend over the `n` full months before `m`, rounded to a
/// whole unit — used by "set from 3-month average".
pub fn average_spend(store: &Store, cat: Id, m: Month, n: i32) -> i64 {
    let total: i64 = (1..=n).map(|k| category_spent(store, cat, m.add(-k))).sum();
    let avg = total / n as i64;
    let scale = store.base().scale();
    ((avg + scale - 1) / scale) * scale
}

/// Remaining money per remaining day (including today) in the month.
pub fn per_day_left(line: &BudgetLine, m: Month, today: Date) -> Option<i64> {
    if !m.contains(today) {
        return None;
    }
    let days_left = (m.days() - today.day() as i32 + 1).max(1) as i64;
    Some(line.remaining().max(0) / days_left)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::{fixture, txn};
    use jiff::civil::date;

    #[test]
    fn rollover_carries_unspent() {
        let mut s = fixture();
        let food = s.find_category("Food").unwrap().id;
        s.save_budget_plan(BudgetPlan {
            category: food,
            amount: 10_000,
            rollover: true,
        })
        .unwrap();
        s.add_txn(txn(&s, date(2026, 1, 5), -6_000, "Food")).unwrap();
        s.add_txn(txn(&s, date(2026, 2, 5), -12_000, "Food")).unwrap();
        s.add_txn(txn(&s, date(2026, 3, 5), -1_000, "Food")).unwrap();
        let mar = month_budget(&s, Month { year: 2026, month: 3 });
        // Jan +4000, Feb -2000 => carry 2000
        assert_eq!(mar[0].carry, 2_000);
        assert_eq!(mar[0].available(), 12_000);
        assert_eq!(mar[0].remaining(), 11_000);

        s.set_budget_override(food, Month { year: 2026, month: 2 }, Some(20_000))
            .unwrap();
        let mar = month_budget(&s, Month { year: 2026, month: 3 });
        assert_eq!(mar[0].carry, 4_000 + 8_000);
    }

    #[test]
    fn no_rollover_and_average() {
        let mut s = fixture();
        let food = s.find_category("Food").unwrap().id;
        s.save_budget_plan(BudgetPlan {
            category: food,
            amount: 5_000,
            rollover: false,
        })
        .unwrap();
        for m in 1..=3 {
            s.add_txn(txn(&s, date(2026, m, 5), -3_050, "Food")).unwrap();
        }
        let apr = Month { year: 2026, month: 4 };
        assert_eq!(month_budget(&s, apr)[0].carry, 0);
        assert_eq!(average_spend(&s, food, apr, 3), 3_100);
        assert!((month_progress(apr, date(2026, 4, 15)) - 0.5).abs() < 1e-6);
    }
}
