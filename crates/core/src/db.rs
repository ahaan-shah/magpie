//! SQLite persistence. Every write goes straight to disk (WAL mode keeps
//! that sub-millisecond); reads only happen once at startup when the
//! [`crate::store::Store`] loads everything into memory.

use crate::model::*;
use crate::money::Cur;
use jiff::civil::Date;
use rusqlite::{Connection, OptionalExtension, Row, params};
use std::path::Path;

pub type Result<T> = std::result::Result<T, crate::Error>;

const MIGRATIONS: &[&str] = &[
    // v1
    r#"
    CREATE TABLE accounts (
        id        INTEGER PRIMARY KEY,
        name      TEXT NOT NULL,
        kind      TEXT NOT NULL,
        currency  TEXT NOT NULL,
        opening   INTEGER NOT NULL DEFAULT 0,
        color     INTEGER NOT NULL DEFAULT 0,
        archived  INTEGER NOT NULL DEFAULT 0,
        sort      INTEGER NOT NULL DEFAULT 0
    );
    CREATE TABLE categories (
        id        INTEGER PRIMARY KEY,
        name      TEXT NOT NULL,
        kind      TEXT NOT NULL,
        color     INTEGER NOT NULL,
        icon      TEXT NOT NULL DEFAULT '',
        archived  INTEGER NOT NULL DEFAULT 0
    );
    CREATE TABLE transactions (
        id         INTEGER PRIMARY KEY,
        account_id INTEGER NOT NULL REFERENCES accounts(id),
        date       TEXT NOT NULL,
        amount     INTEGER NOT NULL,
        payee      TEXT NOT NULL DEFAULT '',
        category_id INTEGER REFERENCES categories(id) ON DELETE SET NULL,
        note       TEXT NOT NULL DEFAULT '',
        tags       TEXT NOT NULL DEFAULT '',
        transfer_id INTEGER,
        recurring_id INTEGER,
        cleared    INTEGER NOT NULL DEFAULT 1
    );
    CREATE INDEX tx_date ON transactions(date);
    CREATE INDEX tx_account ON transactions(account_id, date);
    CREATE INDEX tx_category ON transactions(category_id, date);
    CREATE TABLE budget_plans (
        category_id INTEGER PRIMARY KEY REFERENCES categories(id) ON DELETE CASCADE,
        amount      INTEGER NOT NULL,
        rollover    INTEGER NOT NULL DEFAULT 0
    );
    CREATE TABLE budget_overrides (
        category_id INTEGER NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
        month       TEXT NOT NULL,
        amount      INTEGER NOT NULL,
        PRIMARY KEY (category_id, month)
    );
    CREATE TABLE recurring (
        id        INTEGER PRIMARY KEY,
        payee     TEXT NOT NULL,
        account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        category_id INTEGER REFERENCES categories(id) ON DELETE SET NULL,
        amount    INTEGER NOT NULL,
        note      TEXT NOT NULL DEFAULT '',
        freq      TEXT NOT NULL,
        interval  INTEGER NOT NULL DEFAULT 1,
        start     TEXT NOT NULL,
        end_date  TEXT,
        posted    INTEGER NOT NULL DEFAULT 0,
        auto_post INTEGER NOT NULL DEFAULT 1,
        active    INTEGER NOT NULL DEFAULT 1
    );
    CREATE TABLE goals (
        id        INTEGER PRIMARY KEY,
        name      TEXT NOT NULL,
        target    INTEGER NOT NULL,
        currency  TEXT NOT NULL,
        deadline  TEXT,
        account_id INTEGER REFERENCES accounts(id) ON DELETE SET NULL,
        color     INTEGER NOT NULL,
        icon      TEXT NOT NULL DEFAULT '',
        created   TEXT NOT NULL,
        archived  INTEGER NOT NULL DEFAULT 0
    );
    CREATE TABLE contributions (
        id      INTEGER PRIMARY KEY,
        goal_id INTEGER NOT NULL REFERENCES goals(id) ON DELETE CASCADE,
        date    TEXT NOT NULL,
        amount  INTEGER NOT NULL,
        note    TEXT NOT NULL DEFAULT ''
    );
    CREATE TABLE receipts (
        id      INTEGER PRIMARY KEY,
        txn_id  INTEGER NOT NULL REFERENCES transactions(id) ON DELETE CASCADE,
        hash    TEXT NOT NULL,
        ext     TEXT NOT NULL,
        name    TEXT NOT NULL DEFAULT ''
    );
    CREATE INDEX receipts_txn ON receipts(txn_id);
    CREATE TABLE fx_rates (
        currency TEXT PRIMARY KEY,
        per_eur  REAL NOT NULL,
        manual   INTEGER NOT NULL DEFAULT 0
    );
    CREATE TABLE settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
    "#,
];

pub struct Db {
    conn: Connection,
}

fn date_str(d: Date) -> String {
    d.to_string()
}

fn parse_date(s: &str) -> Date {
    s.parse().unwrap_or(Date::constant(1970, 1, 1))
}

fn cur(s: &str) -> Cur {
    Cur::new(s).unwrap_or_default()
}

pub(crate) fn join_tags(tags: &[String]) -> String {
    tags.join(" ")
}

pub(crate) fn split_tags(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_owned).collect()
}

impl Db {
    pub fn open(path: &Path) -> Result<Db> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Db> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Db> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA temp_store = MEMORY;",
        )?;
        let db = Db { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        let version: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
            let tx = self.conn.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", i as i64 + 1)?;
            tx.commit()?;
        }
        Ok(())
    }

    pub fn is_empty(&self) -> Result<bool> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM accounts", [], |r| r.get(0))?;
        Ok(n == 0)
    }

    /// Runs `f` inside one SQLite transaction — used for bulk imports.
    pub fn batch<T>(&self, f: impl FnOnce(&Db) -> Result<T>) -> Result<T> {
        let tx = self.conn.unchecked_transaction()?;
        let out = f(self)?;
        tx.commit()?;
        Ok(out)
    }

    /// Writes a consistent copy of the database to `path`.
    pub fn backup_to(&self, path: &Path) -> Result<()> {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        self.conn
            .execute("VACUUM INTO ?1", [path.to_string_lossy()])?;
        Ok(())
    }

    // ---------- settings ----------

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    // ---------- accounts ----------

    pub fn accounts(&self) -> Result<Vec<Account>> {
        let mut st = self.conn.prepare(
            "SELECT id, name, kind, currency, opening, color, archived, sort FROM accounts ORDER BY sort, id",
        )?;
        let rows = st.query_map([], |r| {
            Ok(Account {
                id: r.get(0)?,
                name: r.get(1)?,
                kind: AccountKind::parse(&r.get::<_, String>(2)?),
                currency: cur(&r.get::<_, String>(3)?),
                opening: r.get(4)?,
                color: r.get(5)?,
                archived: r.get(6)?,
                sort: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn save_account(&self, a: &Account) -> Result<Id> {
        if a.id == 0 {
            self.conn.execute(
                "INSERT INTO accounts(name, kind, currency, opening, color, archived, sort) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![a.name, a.kind.as_str(), a.currency.code(), a.opening, a.color, a.archived, a.sort],
            )?;
            Ok(self.conn.last_insert_rowid())
        } else {
            self.conn.execute(
                "UPDATE accounts SET name=?2, kind=?3, currency=?4, opening=?5, color=?6, archived=?7, sort=?8 WHERE id=?1",
                params![a.id, a.name, a.kind.as_str(), a.currency.code(), a.opening, a.color, a.archived, a.sort],
            )?;
            Ok(a.id)
        }
    }

    pub fn delete_account(&self, id: Id) -> Result<()> {
        self.conn
            .execute("DELETE FROM transactions WHERE account_id = ?1", [id])?;
        self.conn
            .execute("DELETE FROM accounts WHERE id = ?1", [id])?;
        Ok(())
    }

    // ---------- categories ----------

    pub fn categories(&self) -> Result<Vec<Category>> {
        let mut st = self
            .conn
            .prepare("SELECT id, name, kind, color, icon, archived FROM categories ORDER BY id")?;
        let rows = st.query_map([], |r| {
            Ok(Category {
                id: r.get(0)?,
                name: r.get(1)?,
                kind: CategoryKind::parse(&r.get::<_, String>(2)?),
                color: r.get(3)?,
                icon: r.get(4)?,
                archived: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn save_category(&self, c: &Category) -> Result<Id> {
        if c.id == 0 {
            self.conn.execute(
                "INSERT INTO categories(name, kind, color, icon, archived) VALUES (?1,?2,?3,?4,?5)",
                params![c.name, c.kind.as_str(), c.color, c.icon, c.archived],
            )?;
            Ok(self.conn.last_insert_rowid())
        } else {
            self.conn.execute(
                "UPDATE categories SET name=?2, kind=?3, color=?4, icon=?5, archived=?6 WHERE id=?1",
                params![c.id, c.name, c.kind.as_str(), c.color, c.icon, c.archived],
            )?;
            Ok(c.id)
        }
    }

    pub fn delete_category(&self, id: Id) -> Result<()> {
        self.conn
            .execute("DELETE FROM categories WHERE id = ?1", [id])?;
        Ok(())
    }

    // ---------- transactions ----------

    fn txn_row(r: &Row) -> rusqlite::Result<Txn> {
        Ok(Txn {
            id: r.get(0)?,
            account: r.get(1)?,
            date: parse_date(&r.get::<_, String>(2)?),
            amount: r.get(3)?,
            payee: r.get(4)?,
            category: r.get(5)?,
            note: r.get(6)?,
            tags: split_tags(&r.get::<_, String>(7)?),
            transfer: r.get(8)?,
            recurring: r.get(9)?,
            cleared: r.get(10)?,
        })
    }

    pub fn txns(&self) -> Result<Vec<Txn>> {
        let mut st = self.conn.prepare(
            "SELECT id, account_id, date, amount, payee, category_id, note, tags, transfer_id, recurring_id, cleared
             FROM transactions ORDER BY date, id",
        )?;
        let rows = st.query_map([], Self::txn_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn insert_txn(&self, t: &Txn) -> Result<Id> {
        let mut st = self.conn.prepare_cached(
            "INSERT INTO transactions(id, account_id, date, amount, payee, category_id, note, tags, transfer_id, recurring_id, cleared)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        )?;
        let id = if t.id == 0 { None } else { Some(t.id) };
        st.execute(params![
            id,
            t.account,
            date_str(t.date),
            t.amount,
            t.payee,
            t.category,
            t.note,
            join_tags(&t.tags),
            t.transfer,
            t.recurring,
            t.cleared
        ])?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_txn(&self, t: &Txn) -> Result<()> {
        let mut st = self.conn.prepare_cached(
            "UPDATE transactions SET account_id=?2, date=?3, amount=?4, payee=?5, category_id=?6, note=?7, tags=?8,
             transfer_id=?9, recurring_id=?10, cleared=?11 WHERE id=?1",
        )?;
        st.execute(params![
            t.id,
            t.account,
            date_str(t.date),
            t.amount,
            t.payee,
            t.category,
            t.note,
            join_tags(&t.tags),
            t.transfer,
            t.recurring,
            t.cleared
        ])?;
        Ok(())
    }

    pub fn delete_txn(&self, id: Id) -> Result<()> {
        self.conn
            .prepare_cached("DELETE FROM transactions WHERE id = ?1")?
            .execute([id])?;
        Ok(())
    }

    // ---------- budgets ----------

    pub fn budget_plans(&self) -> Result<Vec<BudgetPlan>> {
        let mut st = self
            .conn
            .prepare("SELECT category_id, amount, rollover FROM budget_plans")?;
        let rows = st.query_map([], |r| {
            Ok(BudgetPlan {
                category: r.get(0)?,
                amount: r.get(1)?,
                rollover: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn save_budget_plan(&self, p: &BudgetPlan) -> Result<()> {
        self.conn.execute(
            "INSERT INTO budget_plans(category_id, amount, rollover) VALUES (?1,?2,?3)
             ON CONFLICT(category_id) DO UPDATE SET amount=excluded.amount, rollover=excluded.rollover",
            params![p.category, p.amount, p.rollover],
        )?;
        Ok(())
    }

    pub fn delete_budget_plan(&self, category: Id) -> Result<()> {
        self.conn.execute(
            "DELETE FROM budget_plans WHERE category_id = ?1",
            [category],
        )?;
        self.conn.execute(
            "DELETE FROM budget_overrides WHERE category_id = ?1",
            [category],
        )?;
        Ok(())
    }

    pub fn budget_overrides(&self) -> Result<Vec<(Id, Month, i64)>> {
        let mut st = self
            .conn
            .prepare("SELECT category_id, month, amount FROM budget_overrides")?;
        let rows = st.query_map([], |r| {
            Ok((
                r.get::<_, Id>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (c, m, a) = row?;
            if let Some(m) = Month::parse(&m) {
                out.push((c, m, a));
            }
        }
        Ok(out)
    }

    pub fn set_budget_override(
        &self,
        category: Id,
        month: Month,
        amount: Option<i64>,
    ) -> Result<()> {
        match amount {
            Some(a) => self.conn.execute(
                "INSERT INTO budget_overrides(category_id, month, amount) VALUES (?1,?2,?3)
                 ON CONFLICT(category_id, month) DO UPDATE SET amount=excluded.amount",
                params![category, month.key(), a],
            )?,
            None => self.conn.execute(
                "DELETE FROM budget_overrides WHERE category_id=?1 AND month=?2",
                params![category, month.key()],
            )?,
        };
        Ok(())
    }

    // ---------- recurring ----------

    pub fn rules(&self) -> Result<Vec<RecurringRule>> {
        let mut st = self.conn.prepare(
            "SELECT id, payee, account_id, category_id, amount, note, freq, interval, start, end_date, posted, auto_post, active
             FROM recurring ORDER BY id",
        )?;
        let rows = st.query_map([], |r| {
            Ok(RecurringRule {
                id: r.get(0)?,
                payee: r.get(1)?,
                account: r.get(2)?,
                category: r.get(3)?,
                amount: r.get(4)?,
                note: r.get(5)?,
                freq: Freq::parse(&r.get::<_, String>(6)?),
                interval: r.get(7)?,
                start: parse_date(&r.get::<_, String>(8)?),
                end: r.get::<_, Option<String>>(9)?.map(|s| parse_date(&s)),
                posted: r.get(10)?,
                auto_post: r.get(11)?,
                active: r.get(12)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn save_rule(&self, x: &RecurringRule) -> Result<Id> {
        let end = x.end.map(date_str);
        if x.id == 0 {
            self.conn.execute(
                "INSERT INTO recurring(payee, account_id, category_id, amount, note, freq, interval, start, end_date, posted, auto_post, active)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
                params![x.payee, x.account, x.category, x.amount, x.note, x.freq.as_str(), x.interval,
                        date_str(x.start), end, x.posted, x.auto_post, x.active],
            )?;
            Ok(self.conn.last_insert_rowid())
        } else {
            self.conn.execute(
                "UPDATE recurring SET payee=?2, account_id=?3, category_id=?4, amount=?5, note=?6, freq=?7, interval=?8,
                 start=?9, end_date=?10, posted=?11, auto_post=?12, active=?13 WHERE id=?1",
                params![x.id, x.payee, x.account, x.category, x.amount, x.note, x.freq.as_str(), x.interval,
                        date_str(x.start), end, x.posted, x.auto_post, x.active],
            )?;
            Ok(x.id)
        }
    }

    pub fn delete_rule(&self, id: Id) -> Result<()> {
        self.conn
            .execute("DELETE FROM recurring WHERE id = ?1", [id])?;
        Ok(())
    }

    // ---------- goals ----------

    pub fn goals(&self) -> Result<Vec<Goal>> {
        let mut st = self.conn.prepare(
            "SELECT id, name, target, currency, deadline, account_id, color, icon, created, archived FROM goals ORDER BY id",
        )?;
        let rows = st.query_map([], |r| {
            Ok(Goal {
                id: r.get(0)?,
                name: r.get(1)?,
                target: r.get(2)?,
                currency: cur(&r.get::<_, String>(3)?),
                deadline: r.get::<_, Option<String>>(4)?.map(|s| parse_date(&s)),
                account: r.get(5)?,
                color: r.get(6)?,
                icon: r.get(7)?,
                created: parse_date(&r.get::<_, String>(8)?),
                archived: r.get(9)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn save_goal(&self, g: &Goal) -> Result<Id> {
        let dl = g.deadline.map(date_str);
        if g.id == 0 {
            self.conn.execute(
                "INSERT INTO goals(name, target, currency, deadline, account_id, color, icon, created, archived)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![g.name, g.target, g.currency.code(), dl, g.account, g.color, g.icon, date_str(g.created), g.archived],
            )?;
            Ok(self.conn.last_insert_rowid())
        } else {
            self.conn.execute(
                "UPDATE goals SET name=?2, target=?3, currency=?4, deadline=?5, account_id=?6, color=?7, icon=?8,
                 created=?9, archived=?10 WHERE id=?1",
                params![g.id, g.name, g.target, g.currency.code(), dl, g.account, g.color, g.icon, date_str(g.created), g.archived],
            )?;
            Ok(g.id)
        }
    }

    pub fn delete_goal(&self, id: Id) -> Result<()> {
        self.conn.execute("DELETE FROM goals WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn contributions(&self) -> Result<Vec<Contribution>> {
        let mut st = self.conn.prepare(
            "SELECT id, goal_id, date, amount, note FROM contributions ORDER BY date, id",
        )?;
        let rows = st.query_map([], |r| {
            Ok(Contribution {
                id: r.get(0)?,
                goal: r.get(1)?,
                date: parse_date(&r.get::<_, String>(2)?),
                amount: r.get(3)?,
                note: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn insert_contribution(&self, c: &Contribution) -> Result<Id> {
        self.conn.execute(
            "INSERT INTO contributions(goal_id, date, amount, note) VALUES (?1,?2,?3,?4)",
            params![c.goal, date_str(c.date), c.amount, c.note],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn delete_contribution(&self, id: Id) -> Result<()> {
        self.conn
            .execute("DELETE FROM contributions WHERE id = ?1", [id])?;
        Ok(())
    }

    // ---------- receipts ----------

    pub fn receipts(&self) -> Result<Vec<Receipt>> {
        let mut st = self
            .conn
            .prepare("SELECT id, txn_id, hash, ext, name FROM receipts ORDER BY id")?;
        let rows = st.query_map([], |r| {
            Ok(Receipt {
                id: r.get(0)?,
                txn: r.get(1)?,
                hash: r.get(2)?,
                ext: r.get(3)?,
                name: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn insert_receipt(&self, x: &Receipt) -> Result<Id> {
        self.conn.execute(
            "INSERT INTO receipts(txn_id, hash, ext, name) VALUES (?1,?2,?3,?4)",
            params![x.txn, x.hash, x.ext, x.name],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn delete_receipt(&self, id: Id) -> Result<()> {
        self.conn
            .execute("DELETE FROM receipts WHERE id = ?1", [id])?;
        Ok(())
    }

    // ---------- fx ----------

    pub fn rates(&self) -> Result<Vec<(Cur, f64, bool)>> {
        let mut st = self
            .conn
            .prepare("SELECT currency, per_eur, manual FROM fx_rates")?;
        let rows = st.query_map([], |r| {
            Ok((
                cur(&r.get::<_, String>(0)?),
                r.get::<_, f64>(1)?,
                r.get::<_, bool>(2)?,
            ))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn set_rate(&self, c: Cur, per_eur: f64, manual: bool) -> Result<()> {
        self.conn.execute(
            "INSERT INTO fx_rates(currency, per_eur, manual) VALUES (?1,?2,?3)
             ON CONFLICT(currency) DO UPDATE SET per_eur=excluded.per_eur, manual=excluded.manual",
            params![c.code(), per_eur, manual],
        )?;
        Ok(())
    }
}
