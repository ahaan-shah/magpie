//! Load-time benchmark: `cargo run --release -p magpie-core --example bench [N]`
//! Generates a database with N extra transactions, then measures how long a
//! cold `Store::open` and the dashboard aggregates take.

use magpie_core::{Cur, Month, Store, analytics, budget, demo, today};
use std::time::Instant;

fn main() {
    let extra: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(100_000);
    let dir = std::env::temp_dir().join("magpie-bench");
    let _ = std::fs::remove_dir_all(&dir);
    {
        let mut s = Store::open(&dir).unwrap();
        demo::generate(&mut s, Cur::USD, 24, extra).unwrap();
        println!("generated {} transactions", s.txns().len());
    }
    let t = Instant::now();
    let s = Store::open(&dir).unwrap();
    println!("cold open + load:      {:>8.1?}", t.elapsed());
    let t = Instant::now();
    let n = s.db().txns().unwrap().len();
    println!("  of which txns query:  {:>8.1?} ({n} rows)", t.elapsed());
    let m = Month::of(today());
    let t = Instant::now();
    let nw = analytics::net_worth(&s);
    let series = analytics::net_worth_series(&s, m, 12);
    let flow = analytics::cashflow(&s, m, 12);
    let cats = analytics::spending_by_category(&s, m.first(), m.last());
    let b = budget::month_budget(&s, m);
    println!("dashboard aggregates:  {:>8.1?}", t.elapsed());
    let t = Instant::now();
    let payees = analytics::payee_index(&s);
    println!("payee index:           {:>8.1?}", t.elapsed());
    std::hint::black_box((nw, series, flow, cats, b, payees));
    if std::env::var_os("KEEP").is_none() {
        let _ = std::fs::remove_dir_all(&dir);
    }
}
