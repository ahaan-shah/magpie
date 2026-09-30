//! Plain data types shared by the database, the in-memory store and the UI.

use crate::money::Cur;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};

pub type Id = i64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AccountKind {
    Checking,
    Savings,
    Credit,
    Cash,
    Investment,
}

impl AccountKind {
    pub const ALL: [AccountKind; 5] = [
        AccountKind::Checking,
        AccountKind::Savings,
        AccountKind::Credit,
        AccountKind::Cash,
        AccountKind::Investment,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            AccountKind::Checking => "checking",
            AccountKind::Savings => "savings",
            AccountKind::Credit => "credit",
            AccountKind::Cash => "cash",
            AccountKind::Investment => "investment",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            AccountKind::Checking => "Checking",
            AccountKind::Savings => "Savings",
            AccountKind::Credit => "Credit card",
            AccountKind::Cash => "Cash",
            AccountKind::Investment => "Investment",
        }
    }

    pub fn parse(s: &str) -> AccountKind {
        match s {
            "savings" => AccountKind::Savings,
            "credit" => AccountKind::Credit,
            "cash" => AccountKind::Cash,
            "investment" => AccountKind::Investment,
            _ => AccountKind::Checking,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Account {
    pub id: Id,
    pub name: String,
    pub kind: AccountKind,
    pub currency: Cur,
    /// Balance before the first transaction, in minor units.
    pub opening: i64,
    pub color: u32,
    pub archived: bool,
    pub sort: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CategoryKind {
    Expense,
    Income,
}

impl CategoryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CategoryKind::Expense => "expense",
            CategoryKind::Income => "income",
        }
    }
    pub fn parse(s: &str) -> CategoryKind {
        if s == "income" {
            CategoryKind::Income
        } else {
            CategoryKind::Expense
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Category {
    pub id: Id,
    pub name: String,
    pub kind: CategoryKind,
    /// 0xRRGGBB
    pub color: u32,
    /// Name of a Phosphor icon, resolved by the UI.
    pub icon: String,
    pub archived: bool,
}

/// A single ledger entry. `amount` is in the account's currency; negative is
/// money leaving the account, positive is money arriving.
#[derive(Clone, Debug, PartialEq)]
pub struct Txn {
    pub id: Id,
    pub account: Id,
    pub date: Date,
    pub amount: i64,
    pub payee: String,
    pub category: Option<Id>,
    pub note: String,
    /// Lower-case tags without the leading `#`.
    pub tags: Vec<String>,
    /// Both legs of a transfer share this group id.
    pub transfer: Option<Id>,
    pub recurring: Option<Id>,
    pub cleared: bool,
}

impl Txn {
    pub fn blank(account: Id, date: Date) -> Txn {
        Txn {
            id: 0,
            account,
            date,
            amount: 0,
            payee: String::new(),
            category: None,
            note: String::new(),
            tags: Vec::new(),
            transfer: None,
            recurring: None,
            cleared: true,
        }
    }

    pub fn is_transfer(&self) -> bool {
        self.transfer.is_some()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

impl Freq {
    pub const ALL: [Freq; 4] = [Freq::Daily, Freq::Weekly, Freq::Monthly, Freq::Yearly];
    pub fn as_str(self) -> &'static str {
        match self {
            Freq::Daily => "daily",
            Freq::Weekly => "weekly",
            Freq::Monthly => "monthly",
            Freq::Yearly => "yearly",
        }
    }
    pub fn parse(s: &str) -> Freq {
        match s {
            "daily" => Freq::Daily,
            "weekly" => Freq::Weekly,
            "yearly" => Freq::Yearly,
            _ => Freq::Monthly,
        }
    }
    pub fn unit(self, n: i64) -> &'static str {
        match (self, n == 1) {
            (Freq::Daily, true) => "day",
            (Freq::Daily, false) => "days",
            (Freq::Weekly, true) => "week",
            (Freq::Weekly, false) => "weeks",
            (Freq::Monthly, true) => "month",
            (Freq::Monthly, false) => "months",
            (Freq::Yearly, true) => "year",
            (Freq::Yearly, false) => "years",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecurringRule {
    pub id: Id,
    pub payee: String,
    pub account: Id,
    pub category: Option<Id>,
    pub amount: i64,
    pub note: String,
    pub freq: Freq,
    pub interval: i64,
    pub start: Date,
    pub end: Option<Date>,
    /// How many occurrences have been posted (or skipped) so far. The next due
    /// date is always derived from `start + posted * interval`, so monthly
    /// rules anchored on the 31st never drift.
    pub posted: i64,
    pub auto_post: bool,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Goal {
    pub id: Id,
    pub name: String,
    pub target: i64,
    pub currency: Cur,
    pub deadline: Option<Date>,
    /// When set, progress is that account's balance instead of contributions.
    pub account: Option<Id>,
    pub color: u32,
    pub icon: String,
    pub created: Date,
    pub archived: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Contribution {
    pub id: Id,
    pub goal: Id,
    pub date: Date,
    pub amount: i64,
    pub note: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BudgetPlan {
    pub category: Id,
    /// Default monthly amount in the base currency (positive).
    pub amount: i64,
    /// Carry unspent (or overspent) money into the next month.
    pub rollover: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Receipt {
    pub id: Id,
    pub txn: Id,
    pub hash: String,
    pub ext: String,
    pub name: String,
}

/// A calendar month, cheap to copy and compare.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Month {
    pub year: i16,
    pub month: i8,
}

impl Month {
    pub fn of(d: Date) -> Month {
        Month {
            year: d.year(),
            month: d.month(),
        }
    }
    pub fn first(self) -> Date {
        Date::new(self.year, self.month, 1).expect("valid month")
    }
    pub fn last(self) -> Date {
        self.first().last_of_month()
    }
    pub fn next(self) -> Month {
        if self.month == 12 {
            Month {
                year: self.year + 1,
                month: 1,
            }
        } else {
            Month {
                year: self.year,
                month: self.month + 1,
            }
        }
    }
    pub fn prev(self) -> Month {
        if self.month == 1 {
            Month {
                year: self.year - 1,
                month: 12,
            }
        } else {
            Month {
                year: self.year,
                month: self.month - 1,
            }
        }
    }
    #[allow(clippy::should_implement_trait)]
    pub fn add(self, n: i32) -> Month {
        let idx = self.year as i32 * 12 + (self.month as i32 - 1) + n;
        Month {
            year: (idx.div_euclid(12)) as i16,
            month: (idx.rem_euclid(12) + 1) as i8,
        }
    }
    pub fn days(self) -> i32 {
        self.first().days_in_month() as i32
    }
    pub fn key(self) -> String {
        format!("{:04}-{:02}", self.year, self.month)
    }
    pub fn parse(s: &str) -> Option<Month> {
        let (y, m) = s.split_once('-')?;
        let m: i8 = m.parse().ok()?;
        if !(1..=12).contains(&m) {
            return None;
        }
        Some(Month {
            year: y.parse().ok()?,
            month: m,
        })
    }
    pub fn label(self) -> String {
        format!("{} {}", MONTH_NAMES[self.month as usize - 1], self.year)
    }
    pub fn short(self) -> &'static str {
        &MONTH_NAMES[self.month as usize - 1][..3]
    }
    pub fn contains(self, d: Date) -> bool {
        d.year() == self.year && d.month() == self.month
    }
}

pub const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

pub fn today() -> Date {
    jiff::Zoned::now().date()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn month_math() {
        let m = Month { year: 2026, month: 1 };
        assert_eq!(m.prev(), Month { year: 2025, month: 12 });
        assert_eq!(m.add(-13), Month { year: 2024, month: 12 });
        assert_eq!(m.add(23), Month { year: 2027, month: 12 });
        assert_eq!(Month { year: 2024, month: 2 }.days(), 29);
        assert_eq!(Month::parse("2026-09"), Some(Month { year: 2026, month: 9 }));
        assert_eq!(Month::parse("2026-13"), None);
    }
}
