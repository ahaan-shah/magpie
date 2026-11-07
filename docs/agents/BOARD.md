# Work board

Sessions working on Magpie claim work here so they don't collide (see
"Working with other sessions" in CLAUDE.md). One line per active task; delete
your line when the work is merged.

| Topic | Branch | Areas / files | Status |
|---|---|---|---|
| Last month in Reports, Reports donut, one-month budgets, payees with several categories | agent/month-budgets | reports.rs, dashboard.rs, budgets.rs, budget.rs, forms.rs (PayeeAsk, payee suggestions), store.rs, db.rs (**migration v4**) | in progress |

## Reserved

- v2 is used (0.2.1, account card looks). v3 is used (0.2.5, payee default categories). v4 claimed by agent/month-budgets (one-month budgets, extra payee categories). Next free: **v5**.
- Next database migration index: **check `MIGRATIONS` in `crates/core/src/db.rs`** and claim it here before writing one.
- Release owner: whoever the maintainer asks; nobody else bumps versions or tags.
