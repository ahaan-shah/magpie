//! First-run defaults and a realistic demo dataset.

use crate::Result;
use crate::model::*;
use crate::money::Cur;
use crate::store::Store;
use jiff::ToSpan;
use jiff::civil::Date;

/// Category colours: distinct in hue and readable on both light and dark
/// backgrounds.
pub const PALETTE: [u32; 16] = [
    0x5B8DEF, 0xF2994A, 0x27AE60, 0xEB5757, 0x9B51E0, 0x2DB5B5, 0xF2C94C, 0xE56BAF, 0x6FCF97, 0x56CCF2, 0xBB6BD9,
    0xF08A5D, 0x8D99AE, 0xC0A062, 0x4FB3A9, 0xD96C75,
];

const DEFAULT_CATEGORIES: &[(&str, CategoryKind, &str)] = &[
    ("Groceries", CategoryKind::Expense, "shopping-cart"),
    ("Dining", CategoryKind::Expense, "fork-knife"),
    ("Transport", CategoryKind::Expense, "car"),
    ("Housing", CategoryKind::Expense, "house"),
    ("Utilities", CategoryKind::Expense, "lightning"),
    ("Subscriptions", CategoryKind::Expense, "repeat"),
    ("Shopping", CategoryKind::Expense, "bag"),
    ("Health", CategoryKind::Expense, "heartbeat"),
    ("Entertainment", CategoryKind::Expense, "film-strip"),
    ("Travel", CategoryKind::Expense, "airplane"),
    ("Education", CategoryKind::Expense, "book"),
    ("Gifts", CategoryKind::Expense, "gift"),
    ("Personal care", CategoryKind::Expense, "sparkle"),
    ("Fees", CategoryKind::Expense, "receipt"),
    ("Salary", CategoryKind::Income, "briefcase"),
    ("Freelance", CategoryKind::Income, "laptop"),
    ("Interest", CategoryKind::Income, "bank"),
    ("Other income", CategoryKind::Income, "coins"),
];

/// Creates the default categories and a first account if the store is empty.
pub fn seed_defaults(store: &mut Store, base: Cur) -> Result<()> {
    store.update_settings(|s| s.base = base)?;
    if store.categories().is_empty() {
        for (i, (name, kind, icon)) in DEFAULT_CATEGORIES.iter().enumerate() {
            store.save_category(Category {
                id: 0,
                name: (*name).into(),
                kind: *kind,
                color: PALETTE[i % PALETTE.len()],
                icon: (*icon).into(),
                archived: false,
            })?;
        }
    }
    if store.accounts().is_empty() {
        store.save_account(Account {
            id: 0,
            name: "Everyday".into(),
            kind: AccountKind::Checking,
            currency: base,
            opening: 0,
            color: PALETTE[0],
            icon: String::new(),
            style: Default::default(),
            archived: false,
            sort: 0,
        })?;
    }
    Ok(())
}

/// Tiny deterministic PRNG (xorshift64*), so demo data is identical on
/// every machine and we don't need a `rand` dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn f(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.f()
    }
    fn chance(&mut self, p: f64) -> bool {
        self.f() < p
    }
    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[(self.next() % xs.len() as u64) as usize]
    }
}

/// Fills an empty store with ~`months` of believable personal finances in
/// `base`, plus `extra` random transactions (for load testing).
pub fn generate(store: &mut Store, base: Cur, months: i32, extra: usize) -> Result<()> {
    seed_defaults(store, base)?;
    let today = today();
    let start = Month::of(today).add(-months + 1).first();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let usd = |store: &Store, dollars: f64| store.convert(Cur::USD.from_major(dollars), Cur::USD, base);
    let cat = |store: &Store, n: &str| store.find_category(n).map(|c| c.id);

    let travel_cur = if base == Cur::EUR {
        Cur::new("GBP").unwrap()
    } else {
        Cur::EUR
    };
    let checking = store.accounts()[0].id;
    let acct = |store: &mut Store, name: &str, kind, cur: Cur, opening: f64, color| -> Result<Id> {
        let opening = store.convert(Cur::USD.from_major(opening), Cur::USD, cur);
        store.save_account(Account {
            id: 0,
            name: name.into(),
            kind,
            currency: cur,
            opening,
            color,
            icon: String::new(),
            style: Default::default(),
            archived: false,
            sort: 0,
        })
    };
    {
        let mut a = store.account(checking).unwrap().clone();
        a.opening = usd(store, 2_400.0);
        store.save_account(a)?;
    }
    let savings = acct(
        store,
        "High-yield savings",
        AccountKind::Savings,
        base,
        8_000.0,
        PALETTE[2],
    )?;
    let card = acct(store, "Visa card", AccountKind::Credit, base, 0.0, PALETTE[3])?;
    let cash = acct(store, "Wallet", AccountKind::Cash, base, 120.0, PALETTE[6])?;
    let travel = acct(
        store,
        &format!("Travel ({travel_cur})"),
        AccountKind::Checking,
        travel_cur,
        300.0,
        PALETTE[4],
    )?;
    let brokerage = acct(store, "Brokerage", AccountKind::Investment, base, 12_000.0, PALETTE[5])?;
    store.update_settings(|s| s.default_account = Some(card))?;

    // Recurring rules — posted below by the recurring engine.
    let rules: &[(&str, Id, &str, f64, Freq, i8)] = &[
        ("Acme Corp", checking, "Salary", 5_200.0, Freq::Monthly, 28),
        (
            "Maple Street Apartments",
            checking,
            "Housing",
            -1_850.0,
            Freq::Monthly,
            1,
        ),
        ("City Power & Water", checking, "Utilities", -140.0, Freq::Monthly, 12),
        ("Fiber Internet", card, "Utilities", -65.0, Freq::Monthly, 18),
        ("Netflix", card, "Subscriptions", -15.49, Freq::Monthly, 7),
        ("Spotify", card, "Subscriptions", -11.99, Freq::Monthly, 21),
        ("Iron Temple Gym", card, "Health", -45.0, Freq::Monthly, 3),
        ("iCloud+", card, "Subscriptions", -2.99, Freq::Monthly, 14),
        ("Phone plan", card, "Utilities", -35.0, Freq::Monthly, 9),
    ];
    for (payee, account, c, amt, freq, day) in rules {
        let d = Date::new(start.year(), start.month(), *day).unwrap_or(start);
        store.save_rule(RecurringRule {
            id: 0,
            payee: (*payee).into(),
            account: *account,
            category: cat(store, c),
            amount: usd(store, *amt),
            note: String::new(),
            freq: *freq,
            interval: 1,
            start: d,
            end: None,
            posted: 0,
            auto_post: true,
            active: true,
        })?;
    }
    let yearly_start = today - 20.days();
    store.save_rule(RecurringRule {
        id: 0,
        payee: "Amazon Prime".into(),
        account: card,
        category: cat(store, "Subscriptions"),
        amount: usd(store, -139.0),
        note: String::new(),
        freq: Freq::Yearly,
        interval: 1,
        start: Date::new(yearly_start.year() - 1, yearly_start.month(), yearly_start.day()).unwrap_or(yearly_start),
        end: None,
        posted: 0,
        auto_post: true,
        active: true,
    })?;
    store.save_rule(RecurringRule {
        id: 0,
        payee: "Car insurance".into(),
        account: checking,
        category: cat(store, "Transport"),
        amount: usd(store, -620.0),
        note: "6-month premium".into(),
        freq: Freq::Monthly,
        interval: 6,
        start: (today + 9.days()) - 12.months(),
        end: None,
        posted: 2,
        auto_post: false,
        active: true,
    })?;
    crate::recurring::post_due(store, today)?;

    let groceries = ["Trader Joe's", "Whole Foods", "Safeway", "Costco", "Farmers market"];
    let dining = [
        "Blue Bottle",
        "Chipotle",
        "Sushi Zen",
        "Pizzeria Delfina",
        "Tacos El Gordo",
        "Sweetgreen",
        "The Local Pub",
        "Pho 88",
    ];
    let transport = ["Uber", "Lyft", "Shell", "Chevron", "Metro card", "Parking"];
    let shopping = ["Amazon", "Target", "IKEA", "Uniqlo", "Apple", "Best Buy", "REI"];
    let fun = ["AMC Theatres", "Steam", "Concert tickets", "Bowling", "Museum"];
    let care = ["Barber", "Sephora", "Pharmacy"];

    let mut txns = Vec::new();
    let add =
        |txns: &mut Vec<Txn>, account, date: Date, amount: i64, payee: &str, category: Option<Id>, tags: &[&str]| {
            let mut t = Txn::blank(account, date);
            t.amount = amount;
            t.payee = payee.into();
            t.category = category;
            t.tags = tags.iter().map(|s| s.to_string()).collect();
            txns.push(t);
        };
    let (c_groc, c_din, c_tr, c_shop, c_fun, c_care, c_health, c_gift, c_trav, c_free, c_int, c_edu) = (
        cat(store, "Groceries"),
        cat(store, "Dining"),
        cat(store, "Transport"),
        cat(store, "Shopping"),
        cat(store, "Entertainment"),
        cat(store, "Personal care"),
        cat(store, "Health"),
        cat(store, "Gifts"),
        cat(store, "Travel"),
        cat(store, "Freelance"),
        cat(store, "Interest"),
        cat(store, "Education"),
    );
    let mut d = start;
    let mut trip_months = std::collections::HashSet::new();
    while d <= today {
        let wd = d.weekday().to_monday_zero_offset();
        let season = 1.0 + 0.25 * ((d.month() as f64 - 1.0) / 12.0 * std::f64::consts::TAU).cos() * 0.4;
        if rng.chance(if wd >= 5 { 0.45 } else { 0.18 }) {
            let v = usd(store, -rng.range(18.0, 140.0) * season);
            add(&mut txns, card, d, v, rng.pick(&groceries), c_groc, &[]);
        }
        if rng.chance(if wd >= 4 { 0.55 } else { 0.28 }) {
            let v = usd(store, -rng.range(6.0, 68.0));
            let acc = if rng.chance(0.15) { cash } else { card };
            let tags: &[&str] = if rng.chance(0.2) { &["date-night"] } else { &[] };
            add(&mut txns, acc, d, v, rng.pick(&dining), c_din, tags);
        }
        if rng.chance(0.3) {
            let v = usd(store, -rng.range(4.0, 55.0));
            add(&mut txns, card, d, v, rng.pick(&transport), c_tr, &[]);
        }
        if rng.chance(0.08) {
            let v = usd(store, -rng.range(15.0, 240.0));
            add(&mut txns, card, d, v, rng.pick(&shopping), c_shop, &[]);
        }
        if rng.chance(0.06) {
            let v = usd(store, -rng.range(12.0, 90.0));
            add(&mut txns, card, d, v, rng.pick(&fun), c_fun, &[]);
        }
        if rng.chance(0.03) {
            let v = usd(store, -rng.range(15.0, 80.0));
            add(&mut txns, card, d, v, rng.pick(&care), c_care, &[]);
        }
        if rng.chance(0.015) {
            let v = usd(store, -rng.range(20.0, 180.0));
            add(&mut txns, checking, d, v, "Dr. Patel's office", c_health, &["medical"]);
        }
        if d.month() == 12 && d.day() > 5 && d.day() < 22 && rng.chance(0.25) {
            let v = usd(store, -rng.range(25.0, 120.0));
            add(&mut txns, card, d, v, rng.pick(&shopping), c_gift, &["holidays"]);
        }
        if rng.chance(0.012) {
            let v = usd(store, -rng.range(20.0, 60.0));
            add(&mut txns, card, d, v, "Coursera", c_edu, &[]);
        }
        if d.day() == 1 {
            add(
                &mut txns,
                savings,
                d,
                usd(store, rng.range(18.0, 32.0)),
                "Interest payment",
                c_int,
                &[],
            );
        }
        if d.day() == 10 && rng.chance(0.55) {
            add(
                &mut txns,
                checking,
                d,
                usd(store, rng.range(400.0, 1600.0)),
                "Freelance client",
                c_free,
                &["side-gig"],
            );
        }
        // Summer and winter trips on the travel account.
        let m = Month::of(d);
        if (d.month() == 7 || d.month() == 12) && d.day() >= 14 && d.day() <= 21 {
            if trip_months.insert(m) {
                let v = store.convert(Cur::USD.from_major(-rng.range(250.0, 600.0)), Cur::USD, travel_cur);
                add(&mut txns, travel, d, v, "Hotel Lumière", c_trav, &["vacation"]);
            }
            for _ in 0..2 {
                let v = store.convert(Cur::USD.from_major(-rng.range(8.0, 70.0)), Cur::USD, travel_cur);
                add(&mut txns, travel, d, v, "Café de Flore", c_din, &["vacation"]);
            }
        }
        d = d.tomorrow().unwrap();
    }
    // Extra noise for load testing.
    let span_days = (today - start).get_days().max(1) as i64;
    for _ in 0..extra {
        let d = start + ((rng.next() % span_days as u64) as i64).days();
        let v = usd(store, -rng.range(2.0, 90.0));
        add(&mut txns, card, d, v, rng.pick(&dining), c_din, &[]);
    }
    store.add_txns(txns, "Demo data")?;

    // Monthly transfers: savings, card payoff, travel top-ups, investing.
    let mut m = Month::of(start);
    while m.first() <= today {
        let pay_day = Date::new(m.year, m.month, 2).unwrap();
        if pay_day <= today {
            store.add_transfer(checking, savings, pay_day, usd(store, 600.0), None, "Monthly savings")?;
            store.add_transfer(checking, brokerage, pay_day, usd(store, 400.0), None, "Index fund")?;
            let card_bal = crate::analytics::balances_at(store, pay_day)[&card];
            if card_bal < 0 {
                store.add_transfer(checking, card, pay_day, -card_bal, None, "Card payment")?;
            }
            if m.month == 6 || m.month == 11 {
                store.add_transfer(checking, travel, pay_day, usd(store, 900.0), None, "Trip budget")?;
            }
            if rng.chance(0.4) {
                store.add_transfer(checking, cash, pay_day, usd(store, 100.0), None, "ATM")?;
            }
        }
        m = m.next();
    }
    // Brokerage growth as income-less gains (uncategorized positive).
    let mut m = Month::of(start);
    let mut gains = Vec::new();
    while m.last() < today {
        let mut t = Txn::blank(brokerage, m.last());
        t.amount = usd(store, rng.range(-300.0, 650.0));
        t.payee = "Market gain/loss".into();
        t.category = cat(store, "Other income");
        gains.push(t);
        m = m.next();
    }
    store.add_txns(gains, "Demo data")?;

    for (name, amount, rollover) in [
        ("Groceries", 600.0, false),
        ("Dining", 450.0, true),
        ("Transport", 320.0, false),
        ("Shopping", 250.0, false),
        ("Entertainment", 120.0, false),
        ("Subscriptions", 60.0, false),
        ("Utilities", 260.0, false),
        ("Housing", 1_850.0, false),
        ("Personal care", 60.0, false),
        ("Travel", 160.0, true),
    ] {
        if let Some(c) = cat(store, name) {
            store.save_budget_plan(BudgetPlan {
                category: c,
                amount: usd(store, amount),
                rollover,
            })?;
        }
    }

    let g1 = Goal {
        id: 0,
        name: "Emergency fund".into(),
        target: usd(store, 25_000.0),
        currency: base,
        deadline: Some(today + 14.months()),
        account: Some(savings),
        color: PALETTE[2],
        icon: "lifebuoy".into(),
        created: start,
        archived: false,
    };
    store.save_goal(g1)?;
    let trip = store.save_goal(Goal {
        id: 0,
        name: "Japan trip".into(),
        target: usd(store, 4_500.0),
        currency: base,
        deadline: Some(today + 7.months()),
        account: None,
        color: PALETTE[7],
        icon: "airplane-tilt".into(),
        created: today - 5.months(),
        archived: false,
    })?;
    for k in (0..5).rev() {
        store.add_contribution(Contribution {
            id: 0,
            goal: trip,
            date: today - (k * 30 + 3).days(),
            amount: usd(store, rng.range(250.0, 450.0)),
            note: String::new(),
        })?;
    }
    store
        .save_goal(Goal {
            id: 0,
            name: "New laptop".into(),
            target: usd(store, 2_200.0),
            currency: base,
            deadline: None,
            account: None,
            color: PALETTE[9],
            icon: "laptop".into(),
            created: today - 40.days(),
            archived: false,
        })
        .and_then(|id| {
            store.add_contribution(Contribution {
                id: 0,
                goal: id,
                date: today - 12.days(),
                amount: usd(store, 350.0),
                note: String::new(),
            })
        })?;
    // A fresh workspace has nothing new to announce.
    store.update_settings(|s| {
        s.onboarded = true;
        s.seen_version = Some(env!("CARGO_PKG_VERSION").into());
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_generates_consistent_data() {
        let mut s = Store::in_memory().unwrap();
        generate(&mut s, Cur::USD, 24, 0).unwrap();
        assert!(s.txns().len() > 1000, "got {}", s.txns().len());
        assert!(
            s.txns()
                .windows(2)
                .all(|w| (w[0].date, w[0].id) <= (w[1].date, w[1].id))
        );
        let nw = crate::analytics::net_worth(&s);
        assert!(nw > 0, "net worth {nw}");
        assert!(!crate::budget::month_budget(&s, Month::of(today())).is_empty());
    }
}
