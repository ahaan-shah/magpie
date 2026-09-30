//! Savings goals: progress and a projected finish date.

use crate::analytics::balances;
use crate::model::*;
use crate::store::Store;
use jiff::ToSpan;
use jiff::civil::Date;

#[derive(Clone, Debug, PartialEq)]
pub struct GoalStatus {
    /// Saved so far, in the goal's currency.
    pub saved: i64,
    pub fraction: f32,
    /// Average saved per day over the recent window.
    pub per_day: f64,
    pub projected: Option<Date>,
    /// Needed per month from today to hit the deadline.
    pub needed_per_month: Option<i64>,
    pub on_track: Option<bool>,
}

pub fn status(store: &Store, g: &Goal, today: Date) -> GoalStatus {
    let saved = match g.account {
        Some(acc) => {
            let b = balances(store).get(&acc).copied().unwrap_or(0);
            store.convert(b, store.account_cur(acc), g.currency)
        }
        None => store.contributions(g.id).map(|c| c.amount).sum(),
    };
    let fraction = if g.target > 0 {
        (saved as f32 / g.target as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };

    // Saving rate over the last 90 days (or since the goal started).
    let window_start = (today - 90.days()).max(g.created);
    let window_days = ((today - window_start).get_days().max(1)) as f64;
    let recent: i64 = match g.account {
        Some(acc) => {
            let cur = store.account_cur(acc);
            let v: i64 = store
                .txns_between(window_start, today)
                .iter()
                .filter(|t| t.account == acc)
                .map(|t| t.amount)
                .sum();
            store.convert(v, cur, g.currency)
        }
        None => store
            .contributions(g.id)
            .filter(|c| c.date >= window_start)
            .map(|c| c.amount)
            .sum(),
    };
    let per_day = recent as f64 / window_days;
    let left = g.target - saved;
    let projected = if left <= 0 {
        Some(today)
    } else if per_day > 0.0 {
        let days = (left as f64 / per_day).ceil() as i64;
        (days < 365 * 100)
            .then(|| today.checked_add(days.days()).ok())
            .flatten()
    } else {
        None
    };
    let needed_per_month = g.deadline.filter(|d| *d > today && left > 0).map(|d| {
        let months = ((d - today).get_days() as f64 / 30.44).max(1.0);
        (left as f64 / months).ceil() as i64
    });
    let on_track = g.deadline.map(|d| left <= 0 || projected.is_some_and(|p| p <= d));
    GoalStatus {
        saved,
        fraction,
        per_day,
        projected,
        needed_per_month,
        on_track,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::fixture;
    use jiff::civil::date;

    #[test]
    fn projects_from_contributions() {
        let mut s = fixture();
        let g = Goal {
            id: 0,
            name: "Trip".into(),
            target: 100_000,
            currency: crate::Cur::USD,
            deadline: Some(date(2026, 12, 31)),
            account: None,
            color: 0,
            icon: String::new(),
            created: date(2026, 3, 1),
            archived: false,
        };
        let id = s.save_goal(g).unwrap();
        s.add_contribution(Contribution {
            id: 0,
            goal: id,
            date: date(2026, 3, 1),
            amount: 45_000,
            note: String::new(),
        })
        .unwrap();
        let st = status(&s, s.goal(id).unwrap(), date(2026, 3, 31));
        assert_eq!(st.saved, 45_000);
        assert!((st.fraction - 0.45).abs() < 1e-6);
        assert_eq!(st.per_day, 1500.0);
        assert_eq!(st.projected, Some(date(2026, 3, 31) + 37.days()));
        assert_eq!(st.on_track, Some(true));
    }
}
