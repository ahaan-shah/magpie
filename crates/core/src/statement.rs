//! Bank statements in: whatever a bank exports, read into rows, mapped to
//! columns, and turned into transactions.
//!
//! Formats: CSV/TSV/TXT (comma, semicolon, tab or pipe separated; UTF-8,
//! UTF-16 or Windows-1252), Excel (.xlsx, .xls), OpenDocument (.ods) and
//! OFX/QFX. Statements are messy, so the reader copes with preamble lines
//! above the header (account details), footer rows (totals), separate debit
//! and credit columns or a DR/CR indicator, European number formats
//! ("1.234,56"), "12.50 DR" style amounts and dates like "12 Mar 2026".

use crate::model::*;
use crate::money::{self, Cur};
use crate::store::Store;
use crate::{Error, Result};
use jiff::civil::Date;
use std::collections::HashMap;
use std::path::Path;

/// File types the importer understands, for file pickers.
pub const EXTENSIONS: &[&str] = &["csv", "tsv", "txt", "xlsx", "xls", "xlsm", "ods", "ofx", "qfx"];

/// How to read a statement's columns.
#[derive(Clone, Debug, PartialEq)]
pub struct CsvMapping {
    /// Rows above the header (or above the first transaction) to ignore.
    pub skip: usize,
    pub has_header: bool,
    pub date: usize,
    pub payee: Option<usize>,
    /// A single signed amount column…
    pub amount: Option<usize>,
    /// …or separate debit (outflow) / credit (inflow) columns.
    pub debit: Option<usize>,
    pub credit: Option<usize>,
    /// A column saying DR/CR (debit/credit) for an unsigned amount.
    pub direction: Option<usize>,
    pub category: Option<usize>,
    pub note: Option<usize>,
    pub date_format: DateFormat,
    /// Flip signs for banks that export spending as positive numbers.
    pub invert: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateFormat {
    Ymd,
    Dmy,
    Mdy,
}

impl DateFormat {
    pub const ALL: [DateFormat; 3] = [DateFormat::Ymd, DateFormat::Dmy, DateFormat::Mdy];
    pub fn label(self) -> &'static str {
        match self {
            DateFormat::Ymd => "YYYY-MM-DD",
            DateFormat::Dmy => "DD/MM/YYYY",
            DateFormat::Mdy => "MM/DD/YYYY",
        }
    }
}

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

fn month_name(s: &str) -> Option<i32> {
    let s = s.to_ascii_lowercase();
    if s.len() < 3 || !s.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    MONTHS.iter().position(|m| s.starts_with(m)).map(|i| i as i32 + 1)
}

/// Parses a date in the given numeric order. Also accepts month names in
/// any order ("12 Mar 2026", "Mar 12, 2026", "12-MAR-26") and ignores a
/// trailing time ("2026-03-12 10:22", "2026-03-12T10:22:01Z").
pub fn parse_date(s: &str, f: DateFormat) -> Option<Date> {
    let s = s.trim();
    // Drop an ISO time part.
    let s = match s.find('T') {
        Some(i) if i >= 8 && s[..i].chars().all(|c| c.is_ascii_digit() || "-/.".contains(c)) => &s[..i],
        _ => s,
    };
    let parts: Vec<&str> = s
        .split(['-', '/', '.', ' ', ','])
        .filter(|p| !p.is_empty() && !p.contains(':'))
        .take(3)
        .collect();
    if parts.len() < 3 {
        return None;
    }
    let (y, m, d) = if let Some(mi) = parts.iter().position(|p| month_name(p).is_some()) {
        let m = month_name(parts[mi])?;
        let nums: Vec<i32> = parts
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != mi)
            .map(|(_, p)| p.parse().ok())
            .collect::<Option<_>>()?;
        // "2026 Mar 12" has the year first; otherwise the day comes first
        // ("12 Mar 2026", "Mar 12 2026").
        if nums[0] > 31 {
            (nums[0], m, nums[1])
        } else {
            (nums[1], m, nums[0])
        }
    } else {
        let n: Vec<i32> = parts.iter().map(|p| p.parse().ok()).collect::<Option<_>>()?;
        match f {
            DateFormat::Ymd => (n[0], n[1], n[2]),
            DateFormat::Dmy => (n[2], n[1], n[0]),
            DateFormat::Mdy => (n[2], n[0], n[1]),
        }
    };
    let y = if (0..100).contains(&y) { 2000 + y } else { y };
    if !(1900..=2200).contains(&y) {
        return None;
    }
    Date::new(y as i16, m as i8, d as i8).ok()
}

/// Parses an amount the way banks write them: "1,234.56", "1.234,56",
/// "(12.00)", "12.00-", "12.00 DR" / "CR", "−12", "₹ 1,23,456.78".
pub fn parse_amount(s: &str, cur: Cur) -> Option<i64> {
    let mut t = s.trim().to_string();
    if t.is_empty() {
        return None;
    }
    let mut neg = false;
    let upper = t.to_ascii_uppercase();
    for (suffix, is_debit) in [("DR.", true), ("DR", true), ("CR.", false), ("CR", false)] {
        if upper.ends_with(suffix) && upper.len() > suffix.len() {
            neg = is_debit;
            t.truncate(t.len() - suffix.len());
            break;
        }
    }
    let trimmed = t.trim_end();
    if let Some(rest) = trimmed.strip_suffix('-')
        && rest.chars().any(|c| c.is_ascii_digit())
    {
        neg = !neg;
        t = rest.to_string();
    }
    // Decimal comma: "1.234,56", or "12,5" / "12,50" with no other separator.
    let (dot, comma) = (t.rfind('.'), t.rfind(','));
    match (dot, comma) {
        (Some(d), Some(c)) if c > d => t = t.replace('.', "").replace(',', "."),
        (None, Some(c)) => {
            let decimals = t[c + 1..].chars().take_while(|ch| ch.is_ascii_digit()).count();
            if t.matches(',').count() == 1 && (decimals == 1 || decimals == 2) {
                t.replace_range(c..c + 1, ".");
            }
        }
        _ => {}
    }
    let v = money::parse(&t, cur)?;
    Some(if neg { -v.abs() } else { v })
}

// ------------------------------------------------------------------ reading

/// Reads any supported statement file into rows of trimmed cells.
pub fn read_table(path: &Path) -> Result<Vec<Vec<String>>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let rows = match ext.as_str() {
        "xlsx" | "xls" | "xlsm" | "xlsb" | "ods" => read_spreadsheet(path)?,
        _ => {
            let bytes = std::fs::read(path)?;
            let text = decode(&bytes);
            if ext == "ofx" || ext == "qfx" || looks_like_ofx(&text) {
                read_ofx(&text)?
            } else {
                read_delimited(&text)?
            }
        }
    };
    let rows: Vec<Vec<String>> = rows.into_iter().filter(|r| r.iter().any(|c| !c.is_empty())).collect();
    if rows.is_empty() {
        return Err(Error::Msg("that file doesn't contain any rows".into()));
    }
    Ok(rows)
}

/// Text in whatever encoding the bank used: UTF-8 (with or without BOM),
/// UTF-16 with a BOM, or Windows-1252 as the usual fallback.
fn decode(bytes: &[u8]) -> String {
    if let Some((enc, bom)) = encoding_rs::Encoding::for_bom(bytes) {
        return enc.decode_without_bom_handling(&bytes[bom..]).0.into_owned();
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::WINDOWS_1252
            .decode_without_bom_handling(bytes)
            .0
            .into_owned(),
    }
}

/// Picks the separator that splits the most lines into the same number of
/// columns.
fn sniff_delimiter(text: &str) -> u8 {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).take(40).collect();
    let mut best = (b',', 0usize);
    for d in *b",;\t|" {
        let mut counts: HashMap<usize, usize> = HashMap::new();
        for l in &lines {
            let mut n = 0;
            let mut quoted = false;
            for ch in l.bytes() {
                if ch == b'"' {
                    quoted = !quoted;
                } else if ch == d && !quoted {
                    n += 1;
                }
            }
            if n > 0 {
                *counts.entry(n).or_default() += 1;
            }
        }
        // Score: how many lines share the most common column count.
        let score = counts.values().copied().max().unwrap_or(0);
        if score > best.1 {
            best = (d, score);
        }
    }
    best.0
}

fn read_delimited(text: &str) -> Result<Vec<Vec<String>>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(sniff_delimiter(text))
        .from_reader(text.as_bytes());
    let mut rows = Vec::new();
    for rec in rdr.records() {
        rows.push(rec?.iter().map(|s| s.trim().to_string()).collect());
    }
    Ok(rows)
}

fn read_spreadsheet(path: &Path) -> Result<Vec<Vec<String>>> {
    use calamine::{Data, Reader, open_workbook_auto};
    let mut wb = open_workbook_auto(path).map_err(|e| Error::Msg(format!("couldn't read that spreadsheet: {e}")))?;
    // The first sheet with any data; statements rarely use more than one.
    for name in wb.sheet_names() {
        let Ok(range) = wb.worksheet_range(&name) else { continue };
        if range.is_empty() {
            continue;
        }
        let rows = range
            .rows()
            .map(|r| {
                r.iter()
                    .map(|c| match c {
                        Data::Empty => String::new(),
                        Data::String(s) => s.trim().to_string(),
                        Data::Float(f) => fmt_number(*f),
                        Data::Int(i) => i.to_string(),
                        Data::Bool(b) => b.to_string(),
                        Data::DateTime(d) => excel_date(d.as_f64()),
                        Data::DateTimeIso(s) => s.trim().to_string(),
                        Data::DurationIso(s) => s.trim().to_string(),
                        Data::Error(_) => String::new(),
                    })
                    .collect()
            })
            .collect();
        return Ok(rows);
    }
    Ok(Vec::new())
}

fn fmt_number(f: f64) -> String {
    if f.fract() == 0.0 && f.abs() < 1e15 {
        format!("{}", f as i64)
    } else {
        // Round to cents-ish precision without float noise.
        let s = format!("{f:.8}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// An Excel serial date (days since 1899-12-30) as YYYY-MM-DD.
fn excel_date(serial: f64) -> String {
    let base = Date::constant(1899, 12, 30);
    base.checked_add(jiff::Span::new().days(serial.floor() as i64))
        .map(|d| d.to_string())
        .unwrap_or_default()
}

fn looks_like_ofx(text: &str) -> bool {
    let head: String = text.chars().take(2000).collect::<String>().to_ascii_uppercase();
    head.contains("OFXHEADER") || head.contains("<OFX>")
}

/// OFX/QFX (SGML or XML): one row per <STMTTRN>, as Date, Amount, Payee, Memo.
fn read_ofx(text: &str) -> Result<Vec<Vec<String>>> {
    let upper = text.to_ascii_uppercase();
    let mut rows = vec![vec!["Date".to_string(), "Amount".into(), "Payee".into(), "Memo".into()]];
    let mut at = 0;
    while let Some(start) = upper[at..].find("<STMTTRN>") {
        let start = at + start + "<STMTTRN>".len();
        let end = upper[start..].find("</STMTTRN>").map(|e| start + e).unwrap_or_else(|| {
            upper[start..]
                .find("<STMTTRN>")
                .map(|e| start + e)
                .unwrap_or(upper.len())
        });
        let block = &text[start..end];
        let field = |tag: &str| -> String {
            let open = format!("<{tag}>");
            let Some(i) = block.to_ascii_uppercase().find(&open) else {
                return String::new();
            };
            let rest = &block[i + open.len()..];
            rest[..rest.find('<').unwrap_or(rest.len())].trim().to_string()
        };
        let date = field("DTPOSTED");
        let date = if date.len() >= 8 && date[..8].chars().all(|c| c.is_ascii_digit()) {
            format!("{}-{}-{}", &date[..4], &date[4..6], &date[6..8])
        } else {
            date
        };
        let mut payee = field("NAME");
        if payee.is_empty() {
            payee = field("PAYEE");
        }
        rows.push(vec![date, field("TRNAMT"), unescape(&payee), unescape(&field("MEMO"))]);
        at = end;
    }
    if rows.len() == 1 {
        return Err(Error::Msg("no transactions found in that OFX file".into()));
    }
    Ok(rows)
}

fn unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

// ------------------------------------------------------------------ mapping

pub struct CsvPreview {
    pub headers: Vec<String>,
    /// Data rows only (preamble and header removed), up to 200.
    pub rows: Vec<Vec<String>>,
    pub guess: CsvMapping,
}

fn has_date_cell(row: &[String]) -> bool {
    row.iter()
        .any(|c| DateFormat::ALL.iter().any(|f| parse_date(c, *f).is_some()))
}

const DATE_KEYS: &[&str] = &["date", "posted", "time", "dt", "datum", "fecha", "buchung"];

/// Reads a statement's first rows and guesses where the data starts and
/// which column is which, from header names and cell contents.
pub fn preview(path: &Path) -> Result<CsvPreview> {
    let table = read_table(path)?;
    let look = &table[..table.len().min(60)];
    let lower = |c: &String| c.to_lowercase();
    // A header row: several labels, one of them date-like, and no dates.
    let header_at = look.iter().position(|r| {
        r.iter().filter(|c| !c.is_empty()).count() >= 2
            && r.iter().any(|c| {
                let c = lower(c);
                c.len() < 40 && DATE_KEYS.iter().any(|k| c.contains(k))
            })
            && !has_date_cell(r)
    });
    let first_data = look
        .iter()
        .position(|r| r.iter().filter(|c| !c.is_empty()).count() >= 2 && has_date_cell(r));
    // Any language: a row of labels (no dates, no numbers) right above the
    // first transaction is its header.
    let labels_above = |d: usize| {
        d > 0 && {
            let r = &look[d - 1];
            r.iter().filter(|c| !c.is_empty()).count() >= 2
                && !has_date_cell(r)
                && r.iter().all(|c| c.is_empty() || parse_amount(c, Cur::USD).is_none())
        }
    };
    let (skip, has_header) = match (header_at, first_data) {
        (Some(h), Some(d)) if h < d => (h, true),
        (_, Some(d)) if labels_above(d) => (d - 1, true),
        (_, Some(d)) => (d, false),
        (Some(h), None) => (h, true),
        (None, None) => (0, false),
    };
    let data: Vec<Vec<String>> = table
        .iter()
        .skip(skip + has_header as usize)
        .take(200)
        .cloned()
        .collect();
    let width = data
        .iter()
        .map(Vec::len)
        .max()
        .unwrap_or(0)
        .max(if has_header { table[skip].len() } else { 0 });
    let headers: Vec<String> = (0..width)
        .map(|i| {
            has_header
                .then(|| table[skip].get(i).cloned().filter(|h| !h.is_empty()))
                .flatten()
                .unwrap_or_else(|| format!("Column {}", i + 1))
        })
        .collect();

    let hl: Vec<String> = headers.iter().map(|h| h.to_lowercase()).collect();
    let find = |keys: &[&str], not: &[&str]| {
        hl.iter()
            .position(|h| keys.iter().any(|k| h.contains(k)) && !not.iter().any(|n| h.contains(n)))
    };
    // Columns by content, for files without headers.
    let col_share = |i: usize, pred: &dyn Fn(&str) -> bool| {
        let vals: Vec<&String> = data.iter().filter_map(|r| r.get(i)).filter(|c| !c.is_empty()).collect();
        if vals.is_empty() {
            0.0
        } else {
            vals.iter().filter(|c| pred(c)).count() as f32 / vals.len() as f32
        }
    };
    let is_date = |c: &str| DateFormat::ALL.iter().any(|f| parse_date(c, *f).is_some());
    let is_amount = |c: &str| parse_amount(c, Cur::USD).is_some() && !is_date(c);

    let date = find(&["transaction date", "txn date", "posting date", "booking date"], &[])
        .or_else(|| find(DATE_KEYS, &["value", "update"]))
        .or_else(|| (0..width).find(|i| col_share(*i, &is_date) > 0.8))
        .unwrap_or(0);
    let debit = find(
        &["debit", "withdrawal", "paid out", "money out", "outflow", "spent"],
        &["credit"],
    );
    let credit = find(
        &["credit", "deposit", "paid in", "money in", "inflow", "received"],
        &["debit", "card"],
    );
    let amount = find(
        &["amount", "amt", "sum", "betrag", "montant", "importe"],
        &["balance", "withdrawal", "deposit", "debit", "credit"],
    );
    let direction = (0..width).find(|&i| {
        let h = &hl[i];
        (h.contains("dr/cr")
            || h.contains("cr/dr")
            || h.contains("type")
            || h.contains("debit/credit")
            || h == "dr / cr")
            && col_share(i, &|c| {
                matches!(
                    c.to_ascii_uppercase().trim_end_matches('.'),
                    "DR" | "CR" | "D" | "C" | "DEBIT" | "CREDIT"
                )
            }) > 0.8
    });
    let payee = find(
        &[
            "payee",
            "description",
            "narration",
            "particulars",
            "merchant",
            "details",
            "remarks",
            "name",
            "beneficiary",
            "counterparty",
        ],
        &["account name", "file"],
    )
    .or_else(|| {
        // The column with the longest text, for headerless files.
        (0..width)
            .filter(|i| *i != date && col_share(*i, &is_amount) < 0.5)
            .max_by_key(|i| data.iter().filter_map(|r| r.get(*i)).map(String::len).sum::<usize>())
    });
    let category = find(&["category"], &[]);
    let note = find(&["memo", "note", "reference", "ref no", "chq"], &[]).filter(|c| Some(*c) != payee);

    let (amount, debit, credit) = match (debit, credit) {
        (Some(d), Some(c)) if d != c => (None, Some(d), Some(c)),
        _ => {
            let amount = amount.or_else(|| {
                (0..width)
                    .rev()
                    .find(|i| *i != date && Some(*i) != payee && col_share(*i, &is_amount) > 0.8)
            });
            (amount, None, None)
        }
    };
    let direction = direction.filter(|_| amount.is_some());

    let sample: Vec<&str> = data
        .iter()
        .filter_map(|r| r.get(date).map(String::as_str))
        .take(80)
        .collect();
    // The order that reads the most dates; if several do ("01/09/26"), the
    // one that packs them closest together, since a statement covers a
    // short stretch of time (Sep 1–2 beats Jan 9–Feb 9).
    let score = |f: DateFormat| {
        let dates: Vec<Date> = sample.iter().filter_map(|s| parse_date(s, f)).collect();
        let span = match (dates.iter().min(), dates.iter().max()) {
            (Some(a), Some(b)) => (*b - *a).get_days() as i64,
            _ => i64::MAX,
        };
        (dates.len(), -span)
    };
    let date_format = [DateFormat::Dmy, DateFormat::Ymd, DateFormat::Mdy]
        .into_iter()
        .max_by_key(|f| score(*f))
        .unwrap_or(DateFormat::Ymd);
    Ok(CsvPreview {
        headers,
        rows: data,
        guess: CsvMapping {
            skip,
            has_header,
            date,
            payee,
            amount,
            debit,
            credit,
            direction,
            category,
            note,
            date_format,
            invert: false,
        },
    })
}

/// The signed amount of one row under `map`, or None if it has none.
pub fn row_amount(row: &[String], map: &CsvMapping, cur: Cur) -> Option<i64> {
    let get = |i: Option<usize>| i.and_then(|i| row.get(i)).map(String::as_str).unwrap_or("");
    let mut amount = if map.amount.is_some() {
        let v = parse_amount(get(map.amount), cur)?;
        match get(map.direction)
            .trim()
            .trim_end_matches('.')
            .to_ascii_uppercase()
            .as_str()
        {
            "DR" | "D" | "DEBIT" => -v.abs(),
            "CR" | "C" | "CREDIT" => v.abs(),
            _ => v,
        }
    } else {
        let d = parse_amount(get(map.debit), cur).unwrap_or(0).abs();
        let c = parse_amount(get(map.credit), cur).unwrap_or(0).abs();
        if d == 0 && c == 0 {
            return None;
        }
        c - d
    };
    if map.invert {
        amount = -amount;
    }
    Some(amount)
}

pub struct ImportResult {
    pub txns: Vec<Txn>,
    /// Rows that aren't transactions (blank, totals, unreadable).
    pub skipped: usize,
    /// Rows already in the account (same date, amount and payee).
    pub duplicates: usize,
    /// Category names that didn't exist yet.
    pub new_categories: Vec<String>,
}

/// Parses the whole statement with `map` into transactions for `account`.
/// Transactions already in that account are left out, so importing
/// overlapping statements is safe. Nothing is written; call
/// [`crate::io::commit_import`] with the result.
pub fn read(store: &Store, path: &Path, map: &CsvMapping, account: Id) -> Result<ImportResult> {
    let table = read_table(path)?;
    let cur = store.account_cur(account);
    let mut out = ImportResult {
        txns: Vec::new(),
        skipped: 0,
        duplicates: 0,
        new_categories: Vec::new(),
    };
    let mut existing: HashMap<(Date, i64, String), usize> = HashMap::new();
    for t in store.txns().iter().filter(|t| t.account == account) {
        *existing.entry((t.date, t.amount, t.payee.to_lowercase())).or_default() += 1;
    }
    for row in table.iter().skip(map.skip + map.has_header as usize) {
        let get = |i: Option<usize>| i.and_then(|i| row.get(i)).cloned().unwrap_or_default();
        let Some(date) = parse_date(&get(Some(map.date)), map.date_format) else {
            out.skipped += 1;
            continue;
        };
        let Some(amount) = row_amount(row, map, cur) else {
            out.skipped += 1;
            continue;
        };
        let payee = get(map.payee);
        if let Some(n) = existing.get_mut(&(date, amount, payee.to_lowercase()))
            && *n > 0
        {
            *n -= 1;
            out.duplicates += 1;
            continue;
        }
        let cat_name = get(map.category);
        let category = if cat_name.is_empty() {
            None
        } else {
            match store.find_category(&cat_name) {
                Some(c) => Some(c.id),
                None => {
                    if !out.new_categories.iter().any(|n| n.eq_ignore_ascii_case(&cat_name)) {
                        out.new_categories.push(cat_name.clone());
                    }
                    None
                }
            }
        };
        let mut t = Txn::blank(account, date);
        t.amount = amount;
        t.payee = payee;
        t.note = get(map.note);
        t.category = category;
        if category.is_none() && !cat_name.is_empty() {
            // Remember the wanted category in a tag so commit can map it.
            t.tags.push(format!("__cat:{cat_name}"));
        }
        out.txns.push(t);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i16, m: i8, day: i8) -> Date {
        Date::new(y, m, day).unwrap()
    }

    #[test]
    fn dates_in_the_wild() {
        use DateFormat::*;
        assert_eq!(parse_date("2026-09-30", Ymd), Some(d(2026, 9, 30)));
        assert_eq!(parse_date("30/09/2026", Dmy), Some(d(2026, 9, 30)));
        assert_eq!(parse_date("09/30/26", Mdy), Some(d(2026, 9, 30)));
        assert_eq!(parse_date("13/13/2026", Mdy), None);
        assert_eq!(parse_date("12 Mar 2026", Ymd), Some(d(2026, 3, 12)));
        assert_eq!(parse_date("12-MAR-26", Dmy), Some(d(2026, 3, 12)));
        assert_eq!(parse_date("Mar 12, 2026", Mdy), Some(d(2026, 3, 12)));
        assert_eq!(parse_date("2026-03-12T10:22:01Z", Ymd), Some(d(2026, 3, 12)));
        assert_eq!(parse_date("12/03/2026 10:22", Dmy), Some(d(2026, 3, 12)));
        assert_eq!(parse_date("Opening balance", Dmy), None);
        assert_eq!(parse_date("1.234,56", Dmy), None);
    }

    #[test]
    fn amounts_in_the_wild() {
        let c = Cur::USD;
        assert_eq!(parse_amount("1,234.56", c), Some(123_456));
        assert_eq!(parse_amount("1.234,56", c), Some(123_456));
        assert_eq!(parse_amount("-12,50", c), Some(-1250));
        assert_eq!(parse_amount("(12.00)", c), Some(-1200));
        assert_eq!(parse_amount("12.00-", c), Some(-1200));
        assert_eq!(parse_amount("12.00 DR", c), Some(-1200));
        assert_eq!(parse_amount("12.00 Cr", c), Some(1200));
        assert_eq!(parse_amount("₹ 1,23,456.78", c), Some(12_345_678));
        assert_eq!(parse_amount("1,234", c), Some(123_400));
        assert_eq!(parse_amount("", c), None);
        assert_eq!(parse_amount("n/a", c), None);
    }

    fn write(dir: &tempfile::TempDir, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let p = dir.path().join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn indian_bank_statement_with_preamble() {
        let dir = tempfile::tempdir().unwrap();
        let p = write(
            &dir,
            "statement.csv",
            b"HDFC BANK Ltd.,,,,,,\n\
              Account No : 5010xxxx,,,,,,\n\
              Statement From : 01/09/2026 To : 30/09/2026,,,,,,\n\
              ,,,,,,\n\
              Date,Narration,Chq./Ref.No.,Value Dt,Withdrawal Amt.,Deposit Amt.,Closing Balance\n\
              01/09/26,UPI-SWIGGY-swiggy@icici,0000123,01/09/26,450.00,,14550.00\n\
              02/09/26,SALARY SEPT,0000124,02/09/26,,85000.00,99550.00\n\
              ,,,,,,\n\
              STATEMENT SUMMARY :-,,,,,,\n",
        );
        let pv = preview(&p).unwrap();
        let g = &pv.guess;
        assert_eq!((g.skip, g.has_header, g.date), (3, true, 0));
        assert_eq!(
            (g.payee, g.debit, g.credit, g.amount),
            (Some(1), Some(4), Some(5), None)
        );
        assert_eq!(g.date_format, DateFormat::Dmy);
        assert_eq!(pv.rows.len(), 3);
        let row = &pv.rows[0];
        assert_eq!(row_amount(row, g, Cur::USD), Some(-45_000));
    }

    #[test]
    fn european_semicolon_latin1() {
        let dir = tempfile::tempdir().unwrap();
        // "Café" in Windows-1252, semicolons, decimal commas.
        let mut bytes = b"Buchungstag;Empf\xe4nger;Verwendungszweck;Betrag\n".to_vec();
        bytes.extend_from_slice(
            b"30.09.2026;Caf\xe9 M\xfcller;Kaffee;-4,50\n01.10.2026;Arbeitgeber GmbH;Gehalt;2.500,00\n",
        );
        let p = write(&dir, "umsaetze.csv", &bytes);
        let pv = preview(&p).unwrap();
        let g = &pv.guess;
        assert!(g.has_header);
        assert_eq!(g.amount, Some(3));
        assert_eq!(pv.rows[0][1], "Café Müller");
        assert_eq!(row_amount(&pv.rows[0], g, Cur::EUR), Some(-450));
        assert_eq!(row_amount(&pv.rows[1], g, Cur::EUR), Some(250_000));
        assert_eq!(parse_date(&pv.rows[0][0], g.date_format), Some(d(2026, 9, 30)));
    }

    #[test]
    fn amount_with_dr_cr_column() {
        let dir = tempfile::tempdir().unwrap();
        let p = write(
            &dir,
            "s.csv",
            b"Txn Date,Description,Amount,Dr/Cr,Balance\n12 Mar 2026,Coffee,4.50,DR,100\n13 Mar 2026,Refund,10.00,CR,110\n",
        );
        let pv = preview(&p).unwrap();
        let g = &pv.guess;
        assert_eq!((g.amount, g.direction, g.payee), (Some(2), Some(3), Some(1)));
        assert_eq!(row_amount(&pv.rows[0], g, Cur::USD), Some(-450));
        assert_eq!(row_amount(&pv.rows[1], g, Cur::USD), Some(1000));
    }

    #[test]
    fn ofx_statement() {
        let dir = tempfile::tempdir().unwrap();
        let p = write(
            &dir,
            "s.qfx",
            b"OFXHEADER:100\nDATA:OFXSGML\n\n<OFX><BANKMSGSRSV1><STMTTRNRS><STMTRS><BANKTRANLIST>\n\
              <STMTTRN><TRNTYPE>DEBIT<DTPOSTED>20260930120000[-5:EST]<TRNAMT>-4.50<FITID>1<NAME>BLUE BOTTLE &amp; CO<MEMO>coffee\n</STMTTRN>\n\
              <STMTTRN><TRNTYPE>CREDIT<DTPOSTED>20260929<TRNAMT>3200.00<FITID>2<NAME>ACME PAYROLL</STMTTRN>\n\
              </BANKTRANLIST></STMTRS></STMTTRNRS></BANKMSGSRSV1></OFX>\n",
        );
        let pv = preview(&p).unwrap();
        let g = &pv.guess;
        assert_eq!((g.date, g.amount, g.payee, g.note), (0, Some(1), Some(2), Some(3)));
        assert_eq!(pv.rows[0], vec!["2026-09-30", "-4.50", "BLUE BOTTLE & CO", "coffee"]);
        assert_eq!(row_amount(&pv.rows[1], g, Cur::USD), Some(320_000));
    }

    #[test]
    fn spreadsheet_statement() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("s.xlsx");
        let mut wb = rust_xlsxwriter::Workbook::new();
        let ws = wb.add_worksheet();
        let fmt = rust_xlsxwriter::Format::new().set_num_format("dd/mm/yyyy");
        ws.write(0, 0, "Statement of account").unwrap();
        for (c, h) in ["Date", "Description", "Debit", "Credit"].iter().enumerate() {
            ws.write(2, c as u16, *h).unwrap();
        }
        let date = rust_xlsxwriter::ExcelDateTime::from_ymd(2026, 9, 30).unwrap();
        ws.write_datetime_with_format(3, 0, &date, &fmt).unwrap();
        ws.write(3, 1, "Groceries").unwrap();
        ws.write(3, 2, 52.3).unwrap();
        wb.save(&p).unwrap();
        let pv = preview(&p).unwrap();
        let g = &pv.guess;
        assert_eq!((g.skip, g.has_header, g.debit, g.credit), (1, true, Some(2), Some(3)));
        assert_eq!(pv.rows[0][0], "2026-09-30");
        assert_eq!(row_amount(&pv.rows[0], g, Cur::USD), Some(-5230));
    }
}
