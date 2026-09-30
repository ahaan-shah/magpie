//! The in-memory source of truth. Everything is loaded once at startup;
//! every mutation writes through to SQLite and bumps [`Store::version`], which
//! the UI uses to know when to rebuild its cached view-models.

use crate::Result;
use crate::db::Db;
use crate::fx::Rates;
use crate::model::*;
use crate::money::Cur;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub base: Cur,
    pub theme: String,
    pub fx_auto: bool,
    /// Date (YYYY-MM-DD) the ECB rates were last refreshed, if ever.
    pub fx_updated: Option<String>,
    pub onboarded: bool,
    pub default_account: Option<Id>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            base: Cur::USD,
            theme: "Midnight".into(),
            fx_auto: true,
            fx_updated: None,
            onboarded: false,
            default_account: None,
        }
    }
}

/// One user-level change that can be undone.
#[derive(Clone, Debug)]
enum Op {
    Insert(Vec<Txn>),
    Delete(Vec<Txn>, Vec<Receipt>),
    Update(Vec<(Txn, Txn)>),
}

#[derive(Clone, Debug)]
struct UndoEntry {
    label: String,
    op: Op,
}

pub struct Store {
    db: Db,
    dir: Option<PathBuf>,
    accounts: Vec<Account>,
    categories: Vec<Category>,
    /// Sorted by `(date, id)`.
    txns: Vec<Txn>,
    plans: Vec<BudgetPlan>,
    overrides: HashMap<(Id, Month), i64>,
    rules: Vec<RecurringRule>,
    goals: Vec<Goal>,
    contributions: Vec<Contribution>,
    receipts: Vec<Receipt>,
    pub rates: Rates,
    settings: Settings,
    version: u64,
    undo: Vec<UndoEntry>,
    redo: Vec<UndoEntry>,
}

fn sort_key(t: &Txn) -> (jiff::civil::Date, Id) {
    (t.date, t.id)
}

impl Store {
    /// Opens (or creates) the database in `dir`.
    pub fn open(dir: &Path) -> Result<Store> {
        std::fs::create_dir_all(dir)?;
        let db = Db::open(&dir.join("magpie.db"))?;
        let mut s = Self::load(db)?;
        s.dir = Some(dir.to_path_buf());
        Ok(s)
    }

    pub fn in_memory() -> Result<Store> {
        Self::load(Db::open_in_memory()?)
    }

    fn load(db: Db) -> Result<Store> {
        let mut settings = Settings::default();
        if let Some(v) = db.setting("base")? {
            settings.base = Cur::new(&v).unwrap_or_default();
        }
        if let Some(v) = db.setting("theme")? {
            settings.theme = v;
        }
        if let Some(v) = db.setting("fx_auto")? {
            settings.fx_auto = v == "1";
        }
        settings.fx_updated = db.setting("fx_updated")?;
        settings.onboarded = db.setting("onboarded")?.as_deref() == Some("1");
        settings.default_account = db.setting("default_account")?.and_then(|v| v.parse().ok());

        let mut rates = Rates::defaults();
        for (c, r, manual) in db.rates()? {
            rates.set(c, r, manual);
        }

        let overrides = db
            .budget_overrides()?
            .into_iter()
            .map(|(c, m, a)| ((c, m), a))
            .collect();

        Ok(Store {
            accounts: db.accounts()?,
            categories: db.categories()?,
            txns: db.txns()?,
            plans: db.budget_plans()?,
            overrides,
            rules: db.rules()?,
            goals: db.goals()?,
            contributions: db.contributions()?,
            receipts: db.receipts()?,
            rates,
            settings,
            version: 1,
            undo: Vec::new(),
            redo: Vec::new(),
            db,
            dir: None,
        })
    }

    /// Monotonic counter bumped on every change.
    pub fn version(&self) -> u64 {
        self.version
    }

    fn touch(&mut self) {
        self.version += 1;
    }

    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    pub fn db(&self) -> &Db {
        &self.db
    }

    pub fn is_empty(&self) -> bool {
        self.accounts.is_empty()
    }

    pub fn backup_to(&self, path: &Path) -> Result<()> {
        self.db.backup_to(path)
    }

    // ---------- settings ----------

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn base(&self) -> Cur {
        self.settings.base
    }

    pub fn update_settings(&mut self, f: impl FnOnce(&mut Settings)) -> Result<()> {
        let mut s = self.settings.clone();
        f(&mut s);
        let db = &self.db;
        db.set_setting("base", s.base.code())?;
        db.set_setting("theme", &s.theme)?;
        db.set_setting("fx_auto", if s.fx_auto { "1" } else { "0" })?;
        if let Some(u) = &s.fx_updated {
            db.set_setting("fx_updated", u)?;
        }
        db.set_setting("onboarded", if s.onboarded { "1" } else { "0" })?;
        if let Some(a) = s.default_account {
            db.set_setting("default_account", &a.to_string())?;
        }
        self.settings = s;
        self.touch();
        Ok(())
    }

    // ---------- fx ----------

    pub fn convert(&self, amount: i64, from: Cur, to: Cur) -> i64 {
        self.rates.convert(amount, from, to)
    }

    pub fn to_base(&self, amount: i64, from: Cur) -> i64 {
        self.rates.convert(amount, from, self.settings.base)
    }

    /// Amount of a transaction in the base currency.
    pub fn txn_base(&self, t: &Txn) -> i64 {
        let c = self.account_cur(t.account);
        if c == self.settings.base {
            t.amount
        } else {
            self.to_base(t.amount, c)
        }
    }

    pub fn set_rate(&mut self, c: Cur, per_eur: f64, manual: bool) -> Result<()> {
        self.db.set_rate(c, per_eur, manual)?;
        self.rates.set(c, per_eur, manual);
        self.touch();
        Ok(())
    }

    /// Applies freshly fetched ECB rates, leaving manual overrides alone.
    pub fn apply_fetched_rates(&mut self, rates: &[(Cur, f64)], date: &str) -> Result<()> {
        self.db.batch(|db| {
            for &(c, r) in rates {
                if !self.rates.is_manual(c) {
                    db.set_rate(c, r, false)?;
                }
            }
            Ok(())
        })?;
        for &(c, r) in rates {
            if !self.rates.is_manual(c) {
                self.rates.set(c, r, false);
            }
        }
        let d = date.to_string();
        self.update_settings(|s| s.fx_updated = Some(d))
    }

    // ---------- accounts ----------

    pub fn accounts(&self) -> &[Account] {
        &self.accounts
    }

    pub fn active_accounts(&self) -> impl Iterator<Item = &Account> {
        self.accounts.iter().filter(|a| !a.archived)
    }

    pub fn account(&self, id: Id) -> Option<&Account> {
        self.accounts.iter().find(|a| a.id == id)
    }

    pub fn account_cur(&self, id: Id) -> Cur {
        self.account(id).map(|a| a.currency).unwrap_or(self.settings.base)
    }

    pub fn account_name(&self, id: Id) -> &str {
        self.account(id).map(|a| a.name.as_str()).unwrap_or("—")
    }

    pub fn default_account(&self) -> Option<Id> {
        self.settings
            .default_account
            .filter(|id| self.account(*id).is_some_and(|a| !a.archived))
            .or_else(|| self.active_accounts().next().map(|a| a.id))
    }

    pub fn save_account(&mut self, mut a: Account) -> Result<Id> {
        if a.id == 0 {
            a.sort = self.accounts.iter().map(|x| x.sort).max().unwrap_or(0) + 1;
        }
        let id = self.db.save_account(&a)?;
        a.id = id;
        match self.accounts.iter_mut().find(|x| x.id == id) {
            Some(slot) => *slot = a,
            None => self.accounts.push(a),
        }
        self.touch();
        Ok(id)
    }

    /// Deletes an account and all of its transactions.
    pub fn delete_account(&mut self, id: Id) -> Result<()> {
        self.db.delete_account(id)?;
        self.accounts.retain(|a| a.id != id);
        let removed: Vec<Id> = self.txns.iter().filter(|t| t.account == id).map(|t| t.id).collect();
        self.txns.retain(|t| t.account != id);
        self.receipts.retain(|r| !removed.contains(&r.txn));
        self.rules.retain(|r| r.account != id);
        self.undo.clear();
        self.redo.clear();
        self.touch();
        Ok(())
    }

    // ---------- categories ----------

    pub fn categories(&self) -> &[Category] {
        &self.categories
    }

    pub fn category(&self, id: Id) -> Option<&Category> {
        self.categories.iter().find(|c| c.id == id)
    }

    pub fn category_name(&self, id: Option<Id>) -> &str {
        id.and_then(|id| self.category(id))
            .map(|c| c.name.as_str())
            .unwrap_or("Uncategorized")
    }

    pub fn find_category(&self, name: &str) -> Option<&Category> {
        let n = name.trim();
        self.categories.iter().find(|c| c.name.eq_ignore_ascii_case(n))
    }

    pub fn save_category(&mut self, mut c: Category) -> Result<Id> {
        let id = self.db.save_category(&c)?;
        c.id = id;
        match self.categories.iter_mut().find(|x| x.id == id) {
            Some(slot) => *slot = c,
            None => self.categories.push(c),
        }
        self.touch();
        Ok(id)
    }

    /// Deletes a category; its transactions become uncategorized.
    pub fn delete_category(&mut self, id: Id) -> Result<()> {
        self.db.delete_category(id)?;
        self.categories.retain(|c| c.id != id);
        for t in &mut self.txns {
            if t.category == Some(id) {
                t.category = None;
            }
        }
        for r in &mut self.rules {
            if r.category == Some(id) {
                r.category = None;
            }
        }
        self.plans.retain(|p| p.category != id);
        self.overrides.retain(|(c, _), _| *c != id);
        self.undo.clear();
        self.redo.clear();
        self.touch();
        Ok(())
    }

    // ---------- transactions ----------

    /// All transactions, oldest first.
    pub fn txns(&self) -> &[Txn] {
        &self.txns
    }

    /// Transactions with `from <= date <= to`, oldest first. O(log n).
    pub fn txns_between(&self, from: jiff::civil::Date, to: jiff::civil::Date) -> &[Txn] {
        &self.txns[self.range_of(from, to)]
    }

    /// Index range into [`Store::txns`] for `from <= date <= to`.
    pub fn range_of(&self, from: jiff::civil::Date, to: jiff::civil::Date) -> std::ops::Range<usize> {
        let a = self.txns.partition_point(|t| t.date < from);
        let b = self.txns.partition_point(|t| t.date <= to);
        a..b.max(a)
    }

    pub fn txns_in(&self, m: Month) -> &[Txn] {
        self.txns_between(m.first(), m.last())
    }

    pub fn txn(&self, id: Id) -> Option<&Txn> {
        self.txns.iter().find(|t| t.id == id)
    }

    fn place(&mut self, t: Txn) {
        let k = sort_key(&t);
        let pos = self.txns.partition_point(|x| sort_key(x) < k);
        self.txns.insert(pos, t);
    }

    fn remove(&mut self, id: Id) -> Option<Txn> {
        let pos = self.txns.iter().position(|t| t.id == id)?;
        Some(self.txns.remove(pos))
    }

    fn raw_insert(&mut self, mut t: Txn) -> Result<Txn> {
        t.id = self.db.insert_txn(&t)?;
        self.place(t.clone());
        Ok(t)
    }

    fn raw_update(&mut self, t: Txn) -> Result<()> {
        self.db.update_txn(&t)?;
        self.remove(t.id);
        self.place(t);
        Ok(())
    }

    fn raw_delete(&mut self, id: Id) -> Result<(Option<Txn>, Vec<Receipt>)> {
        self.db.delete_txn(id)?;
        let receipts: Vec<Receipt> = self.receipts.iter().filter(|r| r.txn == id).cloned().collect();
        self.receipts.retain(|r| r.txn != id);
        Ok((self.remove(id), receipts))
    }

    fn record(&mut self, label: impl Into<String>, op: Op) {
        self.undo.push(UndoEntry {
            label: label.into(),
            op,
        });
        if self.undo.len() > 200 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn add_txn(&mut self, t: Txn) -> Result<Id> {
        let t = self.raw_insert(t)?;
        let id = t.id;
        self.record("Add transaction", Op::Insert(vec![t]));
        self.touch();
        Ok(id)
    }

    /// Adds many transactions in one SQLite transaction (imports, demo data).
    pub fn add_txns(&mut self, txns: Vec<Txn>, label: &str) -> Result<usize> {
        let n = txns.len();
        let mut inserted = Vec::with_capacity(n);
        self.db.batch(|db| {
            for mut t in txns {
                t.id = db.insert_txn(&t)?;
                inserted.push(t);
            }
            Ok(())
        })?;
        self.txns.extend(inserted.iter().cloned());
        self.txns.sort_by_key(sort_key);
        self.record(label, Op::Insert(inserted));
        self.touch();
        Ok(n)
    }

    pub fn update_txn(&mut self, t: Txn) -> Result<()> {
        let Some(before) = self.txn(t.id).cloned() else {
            return Ok(());
        };
        if before == t {
            return Ok(());
        }
        let mut pairs = vec![(before.clone(), t.clone())];
        self.raw_update(t.clone())?;
        // Keep the other leg of a transfer in sync (date, note and amount).
        if let Some(group) = t.transfer {
            let other = self
                .txns
                .iter()
                .find(|x| x.transfer == Some(group) && x.id != t.id)
                .cloned();
            if let Some(o) = other {
                let mut n = o.clone();
                n.date = t.date;
                n.note = t.note.clone();
                if before.amount != t.amount {
                    let (fc, tc) = (self.account_cur(t.account), self.account_cur(o.account));
                    n.amount = -self.convert(t.amount, fc, tc);
                }
                if n != o {
                    self.raw_update(n.clone())?;
                    pairs.push((o, n));
                }
            }
        }
        self.record("Edit transaction", Op::Update(pairs));
        self.touch();
        Ok(())
    }

    /// Applies `f` to every listed transaction as one undoable step.
    pub fn bulk_update(&mut self, ids: &[Id], label: &str, f: impl Fn(&mut Txn)) -> Result<()> {
        let mut pairs = Vec::new();
        for &id in ids {
            if let Some(before) = self.txn(id).cloned() {
                let mut after = before.clone();
                f(&mut after);
                if after != before {
                    pairs.push((before, after));
                }
            }
        }
        if pairs.is_empty() {
            return Ok(());
        }
        self.db.batch(|db| {
            for (_, a) in &pairs {
                db.update_txn(a)?;
            }
            Ok(())
        })?;
        for (_, a) in &pairs {
            self.remove(a.id);
            self.place(a.clone());
        }
        self.record(label, Op::Update(pairs));
        self.touch();
        Ok(())
    }

    /// Deletes transactions (and the other leg of any transfer).
    pub fn delete_txns(&mut self, ids: &[Id]) -> Result<usize> {
        let mut all: Vec<Id> = ids.to_vec();
        for &id in ids {
            if let Some(g) = self.txn(id).and_then(|t| t.transfer) {
                all.extend(self.txns.iter().filter(|t| t.transfer == Some(g)).map(|t| t.id));
            }
        }
        all.sort_unstable();
        all.dedup();
        let mut removed = Vec::new();
        let mut receipts = Vec::new();
        for id in all {
            let (t, r) = self.raw_delete(id)?;
            if let Some(t) = t {
                removed.push(t);
            }
            receipts.extend(r);
        }
        let n = removed.len();
        let label = if n == 1 {
            "Delete transaction".to_string()
        } else {
            format!("Delete {n} transactions")
        };
        self.record(label, Op::Delete(removed, receipts));
        self.touch();
        Ok(n)
    }

    /// Creates both legs of a transfer. `to_amount` is in the destination
    /// account's currency (pass `None` to convert automatically).
    pub fn add_transfer(
        &mut self,
        from: Id,
        to: Id,
        date: jiff::civil::Date,
        amount: i64,
        to_amount: Option<i64>,
        note: &str,
    ) -> Result<Id> {
        let amount = amount.abs();
        let (fc, tc) = (self.account_cur(from), self.account_cur(to));
        let to_amount = to_amount.map(i64::abs).unwrap_or_else(|| self.convert(amount, fc, tc));
        let payee_out = format!("Transfer to {}", self.account_name(to));
        let payee_in = format!("Transfer from {}", self.account_name(from));

        let mut out = Txn::blank(from, date);
        out.amount = -amount;
        out.payee = payee_out;
        out.note = note.to_string();
        let mut out = self.raw_insert(out)?;
        out.transfer = Some(out.id);
        self.raw_update(out.clone())?;

        let mut inc = Txn::blank(to, date);
        inc.amount = to_amount;
        inc.payee = payee_in;
        inc.note = note.to_string();
        inc.transfer = Some(out.id);
        let inc = self.raw_insert(inc)?;

        let id = out.id;
        self.record("Transfer", Op::Insert(vec![out, inc]));
        self.touch();
        Ok(id)
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|e| e.label.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|e| e.label.as_str())
    }

    fn apply_inverse(&mut self, op: &Op) -> Result<Op> {
        Ok(match op {
            Op::Insert(ts) => {
                let mut rs = Vec::new();
                for t in ts {
                    rs.extend(self.raw_delete(t.id)?.1);
                }
                Op::Delete(ts.clone(), rs)
            }
            Op::Delete(ts, rs) => {
                for t in ts {
                    self.raw_insert(t.clone())?;
                }
                for r in rs {
                    self.db.insert_receipt(r)?;
                    self.receipts.push(r.clone());
                }
                Op::Insert(ts.clone())
            }
            Op::Update(pairs) => {
                for (before, _) in pairs {
                    self.raw_update(before.clone())?;
                }
                Op::Update(pairs.iter().map(|(b, a)| (a.clone(), b.clone())).collect())
            }
        })
    }

    /// Undoes the last change; returns its label.
    pub fn undo(&mut self) -> Result<Option<String>> {
        let Some(e) = self.undo.pop() else {
            return Ok(None);
        };
        let inv = self.apply_inverse(&e.op)?;
        self.redo.push(UndoEntry {
            label: e.label.clone(),
            op: inv,
        });
        self.touch();
        Ok(Some(e.label))
    }

    pub fn redo(&mut self) -> Result<Option<String>> {
        let Some(e) = self.redo.pop() else {
            return Ok(None);
        };
        let inv = self.apply_inverse(&e.op)?;
        self.undo.push(UndoEntry {
            label: e.label.clone(),
            op: inv,
        });
        self.touch();
        Ok(Some(e.label))
    }

    // ---------- budgets ----------

    pub fn budget_plans(&self) -> &[BudgetPlan] {
        &self.plans
    }

    pub fn budget_plan(&self, category: Id) -> Option<&BudgetPlan> {
        self.plans.iter().find(|p| p.category == category)
    }

    pub fn budget_override(&self, category: Id, m: Month) -> Option<i64> {
        self.overrides.get(&(category, m)).copied()
    }

    pub fn save_budget_plan(&mut self, p: BudgetPlan) -> Result<()> {
        self.db.save_budget_plan(&p)?;
        match self.plans.iter_mut().find(|x| x.category == p.category) {
            Some(slot) => *slot = p,
            None => self.plans.push(p),
        }
        self.touch();
        Ok(())
    }

    pub fn delete_budget_plan(&mut self, category: Id) -> Result<()> {
        self.db.delete_budget_plan(category)?;
        self.plans.retain(|p| p.category != category);
        self.overrides.retain(|(c, _), _| *c != category);
        self.touch();
        Ok(())
    }

    pub fn set_budget_override(&mut self, category: Id, m: Month, amount: Option<i64>) -> Result<()> {
        self.db.set_budget_override(category, m, amount)?;
        match amount {
            Some(a) => self.overrides.insert((category, m), a),
            None => self.overrides.remove(&(category, m)),
        };
        self.touch();
        Ok(())
    }

    // ---------- recurring ----------

    pub fn rules(&self) -> &[RecurringRule] {
        &self.rules
    }

    pub fn rule(&self, id: Id) -> Option<&RecurringRule> {
        self.rules.iter().find(|r| r.id == id)
    }

    pub fn save_rule(&mut self, mut r: RecurringRule) -> Result<Id> {
        r.interval = r.interval.max(1);
        let id = self.db.save_rule(&r)?;
        r.id = id;
        match self.rules.iter_mut().find(|x| x.id == id) {
            Some(slot) => *slot = r,
            None => self.rules.push(r),
        }
        self.touch();
        Ok(id)
    }

    pub fn delete_rule(&mut self, id: Id) -> Result<()> {
        self.db.delete_rule(id)?;
        self.rules.retain(|r| r.id != id);
        self.touch();
        Ok(())
    }

    // ---------- goals ----------

    pub fn goals(&self) -> &[Goal] {
        &self.goals
    }

    pub fn goal(&self, id: Id) -> Option<&Goal> {
        self.goals.iter().find(|g| g.id == id)
    }

    pub fn save_goal(&mut self, mut g: Goal) -> Result<Id> {
        let id = self.db.save_goal(&g)?;
        g.id = id;
        match self.goals.iter_mut().find(|x| x.id == id) {
            Some(slot) => *slot = g,
            None => self.goals.push(g),
        }
        self.touch();
        Ok(id)
    }

    pub fn delete_goal(&mut self, id: Id) -> Result<()> {
        self.db.delete_goal(id)?;
        self.goals.retain(|g| g.id != id);
        self.contributions.retain(|c| c.goal != id);
        self.touch();
        Ok(())
    }

    pub fn contributions(&self, goal: Id) -> impl Iterator<Item = &Contribution> {
        self.contributions.iter().filter(move |c| c.goal == goal)
    }

    pub fn add_contribution(&mut self, mut c: Contribution) -> Result<Id> {
        c.id = self.db.insert_contribution(&c)?;
        let id = c.id;
        self.contributions.push(c);
        self.contributions.sort_by_key(|c| (c.date, c.id));
        self.touch();
        Ok(id)
    }

    pub fn delete_contribution(&mut self, id: Id) -> Result<()> {
        self.db.delete_contribution(id)?;
        self.contributions.retain(|c| c.id != id);
        self.touch();
        Ok(())
    }

    // ---------- receipts ----------

    pub fn receipts_for(&self, txn: Id) -> impl Iterator<Item = &Receipt> {
        self.receipts.iter().filter(move |r| r.txn == txn)
    }

    pub fn has_receipt(&self, txn: Id) -> bool {
        self.receipts.iter().any(|r| r.txn == txn)
    }

    pub(crate) fn push_receipt(&mut self, mut r: Receipt) -> Result<Id> {
        r.id = self.db.insert_receipt(&r)?;
        let id = r.id;
        self.receipts.push(r);
        self.touch();
        Ok(id)
    }

    pub fn delete_receipt(&mut self, id: Id) -> Result<()> {
        self.db.delete_receipt(id)?;
        self.receipts.retain(|r| r.id != id);
        self.touch();
        Ok(())
    }

    pub(crate) fn all_receipts(&self) -> &[Receipt] {
        &self.receipts
    }

    pub(crate) fn raw_insert_many(&mut self, txns: Vec<Txn>) -> Result<Vec<Txn>> {
        let mut out = Vec::with_capacity(txns.len());
        self.db.batch(|db| {
            for mut t in txns {
                t.id = db.insert_txn(&t)?;
                out.push(t);
            }
            Ok(())
        })?;
        for t in &out {
            self.place(t.clone());
        }
        self.touch();
        Ok(out)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use jiff::civil::date;

    pub fn fixture() -> Store {
        let mut s = Store::in_memory().unwrap();
        s.save_account(Account {
            id: 0,
            name: "Checking".into(),
            kind: AccountKind::Checking,
            currency: Cur::USD,
            opening: 100_000,
            color: 0x4f8cff,
            archived: false,
            sort: 0,
        })
        .unwrap();
        s.save_category(Category {
            id: 0,
            name: "Food".into(),
            kind: CategoryKind::Expense,
            color: 0xff8844,
            icon: "fork-knife".into(),
            archived: false,
        })
        .unwrap();
        s.save_category(Category {
            id: 0,
            name: "Salary".into(),
            kind: CategoryKind::Income,
            color: 0x44cc88,
            icon: "briefcase".into(),
            archived: false,
        })
        .unwrap();
        s
    }

    pub fn txn(s: &Store, d: jiff::civil::Date, amount: i64, cat: &str) -> Txn {
        let mut t = Txn::blank(s.accounts()[0].id, d);
        t.amount = amount;
        t.category = s.find_category(cat).map(|c| c.id);
        t.payee = cat.to_string();
        t
    }

    #[test]
    fn keeps_sorted_and_undoes() {
        let mut s = fixture();
        let a = s.add_txn(txn(&s, date(2026, 3, 5), -500, "Food")).unwrap();
        s.add_txn(txn(&s, date(2026, 3, 1), -300, "Food")).unwrap();
        s.add_txn(txn(&s, date(2026, 3, 9), 9000, "Salary")).unwrap();
        let dates: Vec<_> = s.txns().iter().map(|t| t.date.day()).collect();
        assert_eq!(dates, vec![1, 5, 9]);

        let mut t = s.txn(a).unwrap().clone();
        t.date = date(2026, 3, 20);
        s.update_txn(t).unwrap();
        assert_eq!(s.txns().last().unwrap().id, a);

        s.delete_txns(&[a]).unwrap();
        assert_eq!(s.txns().len(), 2);
        assert_eq!(s.undo().unwrap().as_deref(), Some("Delete transaction"));
        assert_eq!(s.txns().len(), 3);
        s.undo().unwrap(); // the edit
        assert_eq!(s.txn(a).unwrap().date, date(2026, 3, 5));
        s.redo().unwrap();
        assert_eq!(s.txn(a).unwrap().date, date(2026, 3, 20));
    }

    #[test]
    fn range_queries() {
        let mut s = fixture();
        for d in 1..=28 {
            s.add_txn(txn(&s, date(2026, 2, d), -100, "Food")).unwrap();
        }
        s.add_txn(txn(&s, date(2026, 3, 1), -100, "Food")).unwrap();
        assert_eq!(s.txns_in(Month { year: 2026, month: 2 }).len(), 28);
        assert_eq!(s.txns_between(date(2026, 2, 10), date(2026, 2, 12)).len(), 3);
        assert_eq!(s.txns_between(date(2027, 1, 1), date(2026, 1, 1)).len(), 0);
    }

    #[test]
    fn transfer_legs_stay_linked() {
        let mut s = fixture();
        let savings = s
            .save_account(Account {
                id: 0,
                name: "Savings".into(),
                kind: AccountKind::Savings,
                currency: Cur::USD,
                opening: 0,
                color: 0,
                archived: false,
                sort: 0,
            })
            .unwrap();
        let from = s.accounts()[0].id;
        let id = s
            .add_transfer(from, savings, date(2026, 1, 2), 25_000, None, "")
            .unwrap();
        assert_eq!(s.txns().len(), 2);
        let mut t = s.txn(id).unwrap().clone();
        t.amount = -30_000;
        s.update_txn(t).unwrap();
        let other = s.txns().iter().find(|t| t.account == savings).unwrap();
        assert_eq!(other.amount, 30_000);
        s.delete_txns(&[id]).unwrap();
        assert!(s.txns().is_empty());
    }
}
