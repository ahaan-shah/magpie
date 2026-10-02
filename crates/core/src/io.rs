//! Import and export: CSV / XLSX / JSON out; bank statements (see
//! [`crate::statement`]) and Pear's `transactions.json` in.

use crate::Result;
use crate::model::*;
use crate::money;
use crate::store::Store;
use serde::Serialize;
use std::path::{Path, PathBuf};

// ------------------------------------------------------------------ export

#[derive(Serialize)]
pub struct ExportRow {
    pub date: String,
    pub account: String,
    pub payee: String,
    pub category: String,
    pub amount: String,
    pub currency: String,
    pub note: String,
    pub tags: String,
}

pub const EXPORT_HEADER: [&str; 8] = [
    "date", "account", "payee", "category", "amount", "currency", "note", "tags",
];

pub fn export_rows(store: &Store, txns: &[Txn]) -> Vec<ExportRow> {
    txns.iter()
        .map(|t| {
            let cur = store.account_cur(t.account);
            ExportRow {
                date: t.date.to_string(),
                account: store.account_name(t.account).to_string(),
                payee: t.payee.clone(),
                category: if t.is_transfer() {
                    "Transfer".into()
                } else {
                    store.category_name(t.category).into()
                },
                amount: money::to_input(t.amount, cur),
                currency: cur.code().to_string(),
                note: t.note.clone(),
                tags: t.tags.join(" "),
            }
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Csv,
    Xlsx,
    Json,
}

impl Format {
    pub fn ext(self) -> &'static str {
        match self {
            Format::Csv => "csv",
            Format::Xlsx => "xlsx",
            Format::Json => "json",
        }
    }
}

/// A non-clobbering dated file name in `dir`, e.g. `magpie-2026-09-30 (2).csv`.
pub fn export_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let base = format!("{stem}-{}", today());
    let mut p = dir.join(format!("{base}.{ext}"));
    let mut n = 2;
    while p.exists() {
        p = dir.join(format!("{base} ({n}).{ext}"));
        n += 1;
    }
    p
}

pub fn export(store: &Store, txns: &[Txn], format: Format, path: &Path) -> Result<()> {
    let rows = export_rows(store, txns);
    match format {
        Format::Csv => {
            let mut w = csv::Writer::from_path(path)?;
            w.write_record(EXPORT_HEADER)?;
            for r in &rows {
                w.serialize(r)?;
            }
            w.flush()?;
        }
        Format::Json => {
            std::fs::write(path, serde_json::to_vec_pretty(&rows)?)?;
        }
        Format::Xlsx => {
            use rust_xlsxwriter::{Format as XFormat, Workbook};
            let mut wb = Workbook::new();
            let ws = wb.add_worksheet().set_name("Transactions")?;
            let bold = XFormat::new().set_bold();
            let money_fmt = XFormat::new().set_num_format("#,##0.00");
            for (c, h) in EXPORT_HEADER.iter().enumerate() {
                ws.write_string_with_format(0, c as u16, *h, &bold)?;
            }
            for (i, (r, t)) in rows.iter().zip(txns).enumerate() {
                let row = i as u32 + 1;
                let cur = store.account_cur(t.account);
                ws.write_string(row, 0, &r.date)?;
                ws.write_string(row, 1, &r.account)?;
                ws.write_string(row, 2, &r.payee)?;
                ws.write_string(row, 3, &r.category)?;
                ws.write_number_with_format(row, 4, cur.to_major(t.amount), &money_fmt)?;
                ws.write_string(row, 5, &r.currency)?;
                ws.write_string(row, 6, &r.note)?;
                ws.write_string(row, 7, &r.tags)?;
            }
            ws.set_freeze_panes(1, 0)?;
            for (c, w) in [12, 18, 28, 18, 14, 9, 36, 18].iter().enumerate() {
                ws.set_column_width(c as u16, *w)?;
            }
            wb.save(path)?;
        }
    }
    Ok(())
}

/// Net amount per category per month (income positive, spending negative),
/// one row per category with any activity — a pivot table for spreadsheets.
pub fn export_summary(store: &Store, months: &[Month], path: &Path) -> Result<()> {
    let mut w = csv::Writer::from_path(path)?;
    let mut header = vec!["category".to_string()];
    header.extend(months.iter().map(|m| m.key()));
    w.write_record(&header)?;
    let base = store.base();
    for c in store.categories() {
        let vals: Vec<i64> = months
            .iter()
            .map(|m| {
                store
                    .txns_in(*m)
                    .iter()
                    .filter(|t| t.category == Some(c.id))
                    .map(|t| {
                        let (i, e) = crate::analytics::flow(store, t);
                        i - e
                    })
                    .sum()
            })
            .collect();
        if vals.iter().any(|v| *v != 0) {
            let mut rec = vec![c.name.clone()];
            rec.extend(vals.iter().map(|v| money::to_input(*v, base)));
            w.write_record(&rec)?;
        }
    }
    w.flush()?;
    Ok(())
}

// ------------------------------------------------------------------ statement import

pub use crate::statement::{CsvMapping, CsvPreview, DateFormat, ImportResult, parse_date};

/// Writes imported transactions, creating any missing categories and
/// filling blanks from payee history. Returns how many were imported.
pub fn commit_import(store: &mut Store, mut result: ImportResult, label: &str) -> Result<usize> {
    let palette = crate::demo::PALETTE;
    for (i, name) in result.new_categories.iter().enumerate() {
        // A new category whose every row is money coming in is an income one.
        let tag = format!("__cat:{name}");
        let mut rows = result.txns.iter().filter(|t| t.tags.contains(&tag)).peekable();
        let income = rows.peek().is_some() && rows.all(|t| t.amount > 0);
        store.save_category(Category {
            id: 0,
            name: name.clone(),
            kind: if income {
                CategoryKind::Income
            } else {
                CategoryKind::Expense
            },
            color: palette[(store.categories().len() + i) % palette.len()],
            icon: "tag".into(),
            archived: false,
        })?;
    }
    let payees = crate::analytics::payee_index(store);
    for t in &mut result.txns {
        if let Some(pos) = t.tags.iter().position(|x| x.starts_with("__cat:")) {
            let name = t.tags.remove(pos);
            t.category = store.find_category(&name["__cat:".len()..]).map(|c| c.id);
        }
        if t.category.is_none() && !t.payee.is_empty() {
            let key = t.payee.to_lowercase();
            t.category = payees
                .iter()
                .find(|p| p.name.to_lowercase() == key)
                .and_then(|p| p.category);
        }
    }
    store.add_txns(result.txns, label)
}

// ------------------------------------------------------------------ Pear

/// The default location of Pear's data, if it exists.
pub fn pear_path() -> Option<PathBuf> {
    let p = directories::BaseDirs::new()?
        .config_dir()
        .join("pear")
        .join("transactions.json");
    p.exists().then_some(p)
}

/// Imports Pear's `transactions.json` (amount > 0 was spending there) into
/// `account`, creating categories as needed.
pub fn import_pear(store: &mut Store, path: &Path, account: Id) -> Result<usize> {
    #[derive(serde::Deserialize)]
    struct PearTxn {
        date: String,
        category: String,
        amount: f64,
        #[serde(default)]
        note: String,
    }
    let items: Vec<PearTxn> = serde_json::from_slice(&std::fs::read(path)?)?;
    let cur = store.account_cur(account);
    let mut result = ImportResult {
        txns: Vec::new(),
        skipped: 0,
        duplicates: 0,
        new_categories: Vec::new(),
    };
    for p in items {
        let Some(date) = parse_date(&p.date, DateFormat::Ymd) else {
            result.skipped += 1;
            continue;
        };
        let mut t = Txn::blank(account, date);
        t.amount = -cur.from_major(p.amount);
        t.payee = if p.note.is_empty() {
            p.category.clone()
        } else {
            p.note.clone()
        };
        if !p.category.is_empty() {
            match store.find_category(&p.category) {
                Some(c) => t.category = Some(c.id),
                None => {
                    if !result
                        .new_categories
                        .iter()
                        .any(|n| n.eq_ignore_ascii_case(&p.category))
                    {
                        result.new_categories.push(p.category.clone());
                    }
                    t.tags.push(format!("__cat:{}", p.category));
                }
            }
        }
        result.txns.push(t);
    }
    commit_import(store, result, "Import from Pear")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::fixture;
    use jiff::civil::date;

    #[test]
    fn date_formats() {
        assert_eq!(parse_date("2026-09-30", DateFormat::Ymd), Some(date(2026, 9, 30)));
        assert_eq!(parse_date("30/09/2026", DateFormat::Dmy), Some(date(2026, 9, 30)));
        assert_eq!(parse_date("09/30/26", DateFormat::Mdy), Some(date(2026, 9, 30)));
        assert_eq!(parse_date("13/13/2026", DateFormat::Mdy), None);
    }

    #[test]
    fn csv_roundtrip_with_guessing() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("bank.csv");
        std::fs::write(
            &p,
            "Posted Date,Description,Debit,Credit,Category\n\
             30/09/2026,Blue Bottle,4.50,,Food\n\
             29/09/2026,ACME PAYROLL,,3200.00,Salary\n\
             garbage,row,,,\n\
             28/09/2026,Hardware store,19.99,,Home\n",
        )
        .unwrap();
        let mut s = fixture();
        let pv = crate::statement::preview(&p).unwrap();
        assert!(pv.guess.has_header);
        assert_eq!(pv.guess.date_format, DateFormat::Dmy);
        assert_eq!(
            (pv.guess.debit, pv.guess.credit, pv.guess.payee),
            (Some(2), Some(3), Some(1))
        );
        let acc = s.accounts()[0].id;
        let res = crate::statement::read(&s, &p, &pv.guess, acc).unwrap();
        assert_eq!(res.skipped, 1);
        assert_eq!(res.new_categories, vec!["Home"]);
        assert_eq!(commit_import(&mut s, res, "Import").unwrap(), 3);
        let home = s.find_category("Home").unwrap().id;
        let hw = s.txns().iter().find(|t| t.payee == "Hardware store").unwrap();
        assert_eq!((hw.amount, hw.category, hw.tags.len()), (-1999, Some(home), 0));

        let out = dir.path().join("out.csv");
        let txns = s.txns().to_vec();
        export(&s, &txns, Format::Csv, &out).unwrap();
        let text = std::fs::read_to_string(&out).unwrap();
        assert!(text.starts_with("date,account,payee"));
        assert!(text.contains("2026-09-29,Checking,ACME PAYROLL,Salary,3200.00,USD"));
        export(&s, &txns, Format::Xlsx, &dir.path().join("out.xlsx")).unwrap();
        export(&s, &txns, Format::Json, &dir.path().join("out.json")).unwrap();
    }

    #[test]
    fn imports_pear_json() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("transactions.json");
        std::fs::write(
            &p,
            r#"[{"date":"2026-07-01","category":"Food","amount":240.0,"note":"Groceries","id":"a"},
                {"date":"2026-07-02","category":"Pay","amount":-5000.5,"note":"","id":"b"}]"#,
        )
        .unwrap();
        let mut s = fixture();
        let acc = s.accounts()[0].id;
        assert_eq!(import_pear(&mut s, &p, acc).unwrap(), 2);
        let food = s.find_category("Food").unwrap().id;
        assert_eq!(s.txns()[0].amount, -24_000);
        assert_eq!(s.txns()[0].category, Some(food));
        assert_eq!(s.txns()[1].amount, 500_050);
        assert_eq!(s.txns()[1].payee, "Pay");
    }
}
