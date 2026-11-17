//! Excel workbooks with several sheets: listing them, reading one, and the
//! sheet changes the user makes (rename, hide, delete, reorder, duplicate),
//! which wait until the workbook is saved, like cell edits.
//!
//! Saving writes the whole workbook again: the sheet a tab shows from its
//! current result, every other sheet with the values stored in the file.
//! ParVu writes values only, so formatting, Excel formulas and charts in the
//! workbook are not kept (the UI says so before saving).

use std::path::Path;

use calamine::{Data, Range, Reader, SheetType, SheetVisible, open_workbook_auto};
use duckdb::Connection;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// A worksheet as stored in the file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SheetInfo {
    pub name: String,
    pub hidden: bool,
}

fn excel_err(e: impl std::fmt::Display) -> CoreError {
    CoreError::Excel(e.to_string())
}

/// The worksheets of a workbook, in order (chart and macro sheets left out).
pub fn sheets(path: &Path) -> Result<Vec<SheetInfo>> {
    let wb = open_workbook_auto(path).map_err(excel_err)?;
    Ok(wb
        .sheets_metadata()
        .iter()
        .filter(|s| s.typ == SheetType::WorkSheet)
        .map(|s| SheetInfo { name: s.name.clone(), hidden: s.visible != SheetVisible::Visible })
        .collect())
}

/// The sheet a workbook opens on: the first visible worksheet.
pub fn first_sheet(path: &Path) -> Result<String> {
    let all = sheets(path)?;
    all.iter()
        .find(|s| !s.hidden)
        .or(all.first())
        .map(|s| s.name.clone())
        .ok_or_else(|| CoreError::Excel("The workbook has no worksheets".into()))
}

/// The cells of one sheet.
pub fn read_sheet(path: &Path, sheet: &str) -> Result<Range<Data>> {
    let mut wb = open_workbook_auto(path).map_err(excel_err)?;
    wb.worksheet_range(sheet).map_err(|e| CoreError::Excel(format!("Sheet \"{sheet}\": {e}")))
}

// ---------------------------------------------------------------- the plan

/// A sheet of the workbook as it will be saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanSheet {
    /// Its name after saving.
    pub name: String,
    /// The sheet of the file it comes from.
    pub source: String,
    pub hidden: bool,
    /// Deleted when the workbook is saved; can be restored until then.
    pub deleted: bool,
}

/// The sheets of a workbook with the changes not saved yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkbookPlan {
    pub sheets: Vec<PlanSheet>,
}

/// A change to the sheets of a workbook. Sheets are named by their current
/// (possibly renamed) name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum SheetOp {
    Rename { sheet: String, name: String },
    Hide { sheet: String },
    Show { sheet: String },
    Delete { sheet: String },
    Restore { sheet: String },
    /// Move by `by` places among the sheets (negative: to the left).
    Move { sheet: String, by: i32 },
    /// A copy with the saved values, right after it.
    Duplicate { sheet: String },
}

/// Excel's rules for sheet names.
pub fn check_sheet_name(name: &str) -> Result<()> {
    let bad = |m: &str| Err(CoreError::invalid(m.to_string()));
    if name.trim().is_empty() {
        return bad("Enter a sheet name");
    }
    if name.chars().count() > 31 {
        return bad("Sheet names are at most 31 characters long");
    }
    if let Some(c) = name.chars().find(|c| "[]:*?/\\".contains(*c)) {
        return Err(CoreError::invalid(format!("Sheet names can't contain {c}")));
    }
    if name.starts_with('\'') || name.ends_with('\'') {
        return bad("Sheet names can't start or end with an apostrophe");
    }
    if name.eq_ignore_ascii_case("History") {
        return bad("Excel reserves the sheet name History");
    }
    Ok(())
}

impl WorkbookPlan {
    /// The workbook as stored, with nothing changed.
    pub fn from_file(path: &Path) -> Result<Self> {
        Ok(Self {
            sheets: sheets(path)?
                .into_iter()
                .map(|s| PlanSheet { source: s.name.clone(), name: s.name, hidden: s.hidden, deleted: false })
                .collect(),
        })
    }

    /// Whether anything differs from `stored`.
    pub fn differs_from(&self, stored: &WorkbookPlan) -> bool {
        self != stored
    }

    fn index(&self, sheet: &str) -> Result<usize> {
        self.sheets
            .iter()
            .position(|s| s.name == sheet)
            .ok_or_else(|| CoreError::invalid(format!("No sheet \"{sheet}\" in this workbook")))
    }

    fn name_taken(&self, name: &str, except: Option<usize>) -> bool {
        self.sheets
            .iter()
            .enumerate()
            .any(|(i, s)| Some(i) != except && !s.deleted && s.name.to_lowercase() == name.to_lowercase())
    }

    /// Visible sheets that will be saved.
    fn visible(&self) -> usize {
        self.sheets.iter().filter(|s| !s.deleted && !s.hidden).count()
    }

    /// Apply `op`. `shown` lists the sheets open in a tab (by source): they
    /// can't be deleted.
    pub fn apply(&mut self, op: &SheetOp, shown: &[String]) -> Result<()> {
        let before = self.clone();
        let r = self.apply_inner(op, shown);
        if r.is_ok() && self.visible() == 0 {
            *self = before;
            return Err(CoreError::invalid("A workbook needs at least one visible sheet"));
        }
        if r.is_err() {
            *self = before;
        }
        r
    }

    fn apply_inner(&mut self, op: &SheetOp, shown: &[String]) -> Result<()> {
        match op {
            SheetOp::Rename { sheet, name } => {
                let i = self.index(sheet)?;
                let name = name.trim().to_string();
                check_sheet_name(&name)?;
                if self.name_taken(&name, Some(i)) {
                    return Err(CoreError::invalid(format!("There is already a sheet named \"{name}\"")));
                }
                self.sheets[i].name = name;
            }
            SheetOp::Hide { sheet } => {
                let i = self.index(sheet)?;
                self.sheets[i].hidden = true;
            }
            SheetOp::Show { sheet } => {
                let i = self.index(sheet)?;
                self.sheets[i].hidden = false;
            }
            SheetOp::Delete { sheet } => {
                let i = self.index(sheet)?;
                let s = &self.sheets[i];
                // Only the first plan sheet of a source is "the" open one.
                let is_original = self.sheets.iter().position(|x| x.source == s.source) == Some(i);
                if is_original && shown.contains(&s.source) {
                    return Err(CoreError::invalid(format!(
                        "\"{sheet}\" is open in a tab: show another sheet there, or close the tab, first"
                    )));
                }
                self.sheets[i].deleted = true;
            }
            SheetOp::Restore { sheet } => {
                let i = self.index(sheet)?;
                if self.name_taken(&self.sheets[i].name.clone(), Some(i)) {
                    return Err(CoreError::invalid(format!("Rename the other sheet named \"{sheet}\" first")));
                }
                self.sheets[i].deleted = false;
            }
            SheetOp::Move { sheet, by } => {
                let i = self.index(sheet)?;
                let j = (i as i64 + i64::from(*by)).clamp(0, self.sheets.len() as i64 - 1) as usize;
                let s = self.sheets.remove(i);
                self.sheets.insert(j, s);
            }
            SheetOp::Duplicate { sheet } => {
                let i = self.index(sheet)?;
                let src = self.sheets[i].clone();
                let mut n = 2;
                let name = loop {
                    let suffix = format!(" ({n})");
                    let stem: String = src.name.chars().take(31 - suffix.chars().count()).collect();
                    let candidate = format!("{stem}{suffix}");
                    if !self.name_taken(&candidate, None) {
                        break candidate;
                    }
                    n += 1;
                };
                self.sheets.insert(i + 1, PlanSheet { name, deleted: false, ..src });
            }
        }
        Ok(())
    }

    /// The name a stored sheet will have after saving (`None`: deleted).
    pub fn saved_name(&self, source: &str) -> Option<&str> {
        self.sheets.iter().find(|s| s.source == source && !s.deleted).map(|s| s.name.as_str())
    }
}

// ---------------------------------------------------------------- writing

/// Write `plan` to `out` (via a temporary file): the sheet `own` (a source
/// name) from `query`, the others with the values stored in `from`.
/// Returns the rows written from `query`.
pub fn write_workbook(
    conn: &Connection,
    from: &Path,
    out: &Path,
    plan: &WorkbookPlan,
    own: &str,
    query: &str,
) -> Result<u64> {
    let tmp = crate::export::temp_path_for(out);
    let result = write_to(conn, from, &tmp, plan, own, query);
    match result {
        Ok(rows) => {
            std::fs::rename(&tmp, out)?;
            Ok(rows)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

fn write_to(conn: &Connection, from: &Path, tmp: &Path, plan: &WorkbookPlan, own: &str, query: &str) -> Result<u64> {
    let mut wb = rust_xlsxwriter::Workbook::new();
    let mut rows = 0;
    let mut own_written = false;
    let mut source: Option<calamine::Sheets<std::io::BufReader<std::fs::File>>> = None;
    for s in plan.sheets.iter().filter(|s| !s.deleted) {
        let ws = wb.add_worksheet_with_constant_memory().map_err(crate::export::xlsx_err)?;
        ws.set_name(&s.name).map_err(crate::export::xlsx_err)?;
        if s.hidden {
            ws.set_hidden(true);
        }
        if s.source == own && !own_written {
            rows = crate::export::write_query_sheet(ws, conn, query)?;
            own_written = true;
        } else {
            let book = match source.as_mut() {
                Some(b) => b,
                None => source.insert(open_workbook_auto(from).map_err(excel_err)?),
            };
            let range = book.worksheet_range(&s.source).map_err(excel_err)?;
            copy_range(ws, &range)?;
        }
    }
    wb.save(tmp).map_err(crate::export::xlsx_err)?;
    Ok(rows)
}

/// The values of a stored sheet, cell by cell.
fn copy_range(ws: &mut rust_xlsxwriter::Worksheet, range: &Range<Data>) -> Result<()> {
    use rust_xlsxwriter::Format;
    let date_fmt = Format::new().set_num_format("yyyy-mm-dd");
    let datetime_fmt = Format::new().set_num_format("yyyy-mm-dd hh:mm:ss");
    let (r0, c0) = range.start().unwrap_or((0, 0));
    let x = crate::export::xlsx_err;
    for (r, row) in range.rows().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            let (rr, cc) = (r0 + r as u32, (c0 as usize + c) as u16);
            match cell {
                Data::Empty | Data::Error(_) => {}
                Data::Int(i) => {
                    ws.write_number(rr, cc, *i as f64).map_err(x)?;
                }
                Data::Float(f) => {
                    ws.write_number(rr, cc, *f).map_err(x)?;
                }
                Data::Bool(b) => {
                    ws.write_boolean(rr, cc, *b).map_err(x)?;
                }
                Data::String(s) | Data::DateTimeIso(s) | Data::DurationIso(s) => {
                    ws.write_string(rr, cc, s).map_err(x)?;
                }
                Data::DateTime(dt) => match dt.as_datetime() {
                    Some(t) if t.time() == chrono::NaiveTime::MIN => {
                        ws.write_date_with_format(rr, cc, t.date(), &date_fmt).map_err(x)?;
                    }
                    Some(t) => {
                        ws.write_datetime_with_format(rr, cc, t, &datetime_fmt).map_err(x)?;
                    }
                    None => {
                        ws.write_number(rr, cc, dt.as_f64()).map_err(x)?;
                    }
                },
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(names: &[&str]) -> WorkbookPlan {
        WorkbookPlan {
            sheets: names
                .iter()
                .map(|n| PlanSheet { name: n.to_string(), source: n.to_string(), hidden: false, deleted: false })
                .collect(),
        }
    }

    #[test]
    fn rename_follows_excel_rules() {
        let mut p = plan(&["A", "B"]);
        let r = |p: &mut WorkbookPlan, n: &str| p.apply(&SheetOp::Rename { sheet: "A".into(), name: n.into() }, &[]);
        assert!(r(&mut p, "b").is_err(), "names are unique ignoring case");
        assert!(r(&mut p, "a/b").is_err());
        assert!(r(&mut p, &"x".repeat(32)).is_err());
        assert!(r(&mut p, "'q").is_err());
        assert!(r(&mut p, "  ").is_err());
        r(&mut p, "Sales 2024").unwrap();
        assert_eq!(p.sheets[0].name, "Sales 2024");
        assert_eq!(p.saved_name("A"), Some("Sales 2024"));
    }

    #[test]
    fn keeps_one_visible_sheet() {
        let mut p = plan(&["A", "B"]);
        p.apply(&SheetOp::Hide { sheet: "A".into() }, &[]).unwrap();
        assert!(p.apply(&SheetOp::Hide { sheet: "B".into() }, &[]).is_err());
        assert!(p.apply(&SheetOp::Delete { sheet: "B".into() }, &[]).is_err());
        assert!(!p.sheets[1].hidden && !p.sheets[1].deleted, "a refused change changes nothing");
        p.apply(&SheetOp::Show { sheet: "A".into() }, &[]).unwrap();
        p.apply(&SheetOp::Delete { sheet: "B".into() }, &[]).unwrap();
        assert_eq!(p.saved_name("B"), None);
        p.apply(&SheetOp::Restore { sheet: "B".into() }, &[]).unwrap();
        assert_eq!(p.saved_name("B"), Some("B"));
    }

    #[test]
    fn open_sheets_cant_be_deleted() {
        let mut p = plan(&["A", "B"]);
        let err = p.apply(&SheetOp::Delete { sheet: "A".into() }, &["A".into()]).unwrap_err().to_string();
        assert!(err.contains("open in a tab"), "{err}");
        // A copy of an open sheet can.
        p.apply(&SheetOp::Duplicate { sheet: "A".into() }, &[]).unwrap();
        assert_eq!(p.sheets[1].name, "A (2)");
        p.apply(&SheetOp::Delete { sheet: "A (2)".into() }, &["A".into()]).unwrap();
    }

    #[test]
    fn moves_and_duplicates() {
        let mut p = plan(&["A", "B", "C"]);
        p.apply(&SheetOp::Move { sheet: "C".into(), by: -5 }, &[]).unwrap();
        assert_eq!(p.sheets.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["C", "A", "B"]);
        p.apply(&SheetOp::Duplicate { sheet: "A".into() }, &[]).unwrap();
        p.apply(&SheetOp::Duplicate { sheet: "A".into() }, &[]).unwrap();
        assert_eq!(p.sheets.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["C", "A", "A (3)", "A (2)", "B"]);
        let long = "x".repeat(31);
        let mut q = plan(&[&long]);
        q.apply(&SheetOp::Duplicate { sheet: long.clone() }, &[]).unwrap();
        assert_eq!(q.sheets[1].name.chars().count(), 31);
    }
}
