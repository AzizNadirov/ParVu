//! Excel's functions compiled by the expression language and run in
//! DuckDB: every case is an Excel formula and the value Excel gives.

use parvu_core::duckdb::Connection;
use parvu_dsl::{Catalog, FunctionRegistry};

fn setup() -> (Connection, Catalog) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE t AS SELECT * FROM (VALUES
            ('hELLO wORLD', 'AB-12', 2.345, 3, DATE '2024-01-31', TRUE, ' 12,345 ', 'North', 10.0),
            ('x', 'CD-7', -2.5, 8, DATE '2024-03-04', FALSE, 'abc', 'South', 20.0),
            ('y', NULL, 7.0, 1, DATE '2024-12-30', TRUE, '', 'North', 30.0)
         ) v(name, code, price, qty, d, flag, txt, region, amount);
         CREATE TABLE prices AS SELECT * FROM (VALUES ('AB-12', 9.5), ('CD-7', 4.0)) v(id, price);",
    )
    .unwrap();
    let mut catalog = Catalog::new();
    for table in ["t", "prices"] {
        let cols: Vec<(String, String)> = conn
            .prepare(&format!(
                "SELECT column_name, data_type FROM information_schema.columns WHERE table_name = '{table}' ORDER BY ordinal_position"
            ))
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        catalog.register_table(table, cols);
    }
    (conn, catalog)
}

/// The formula's value on row `row` (0-based) as text; NULL as "NULL".
fn eval(conn: &Connection, catalog: &Catalog, formula: &str, row: usize) -> String {
    let reg = FunctionRegistry::default();
    let f = parvu_dsl::compile_formula(formula, catalog, &reg, "t").unwrap_or_else(|e| panic!("{formula}: {e}"));
    // Totals are filled in the way the core does it.
    let mut sql = f.sql.clone();
    for (i, total) in f.totals.iter().enumerate() {
        let (v, ty): (Option<String>, String) = conn
            .query_row(&format!("SELECT CAST(({total}) AS VARCHAR), typeof(({total})) FROM t"), [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap_or_else(|e| panic!("{formula} total {total}: {e}"));
        let lit = v.map(|v| format!("CAST('{v}' AS {ty})")).unwrap_or_else(|| "NULL".into());
        sql = sql.replace(&parvu_dsl::total_marker(i), &lit);
    }
    let q = format!("SELECT CAST(({sql}) AS VARCHAR) FROM (SELECT *, row_number() OVER () AS __n FROM t) AS t ORDER BY __n LIMIT 1 OFFSET {row}");
    let v: Option<String> = conn.query_row(&q, [], |r| r.get(0)).unwrap_or_else(|e| panic!("{formula}\n{q}\n{e}"));
    v.unwrap_or_else(|| "NULL".into())
}

fn check(cases: &[(&str, &str)]) {
    let (conn, catalog) = setup();
    let mut failed = Vec::new();
    for (formula, want) in cases {
        let got = eval(&conn, &catalog, formula, 0);
        if got != *want {
            failed.push(format!("{formula}: got {got}, want {want}"));
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}

#[test]
fn math_and_information() {
    check(&[
        ("ROUNDUP([price], 2)", "2.35"),
        ("ROUNDUP(-2.341, 1)", "-2.4"),
        ("ROUNDDOWN([price], 1)", "2.3"),
        ("ROUNDDOWN([price] + 1, 0)", "3.0"),
        ("TRUNC(-2.7)", "-2.0"),
        ("MROUND(7, 5)", "5.0"),
        ("INT(-2.5)", "-3"),
        ("ROUND(CEILING(2.31, 0.05), 2)", "2.35"),
        ("LOG(8, 2)", "3.0"),
        ("LOG(1000)", "3.0"),
        ("QUOTIENT(7, 2)", "3.0"),
        ("PRODUCT(2, 3, [qty])", "18"),
        ("ROUND(PI(), 4)", "3.1416"),
        ("RANDBETWEEN(5, 5)", "5"),
        ("ISBLANK(NA())", "true"),
        ("ISBLANK([name])", "false"),
        ("ISNUMBER([price])", "true"),
        ("ISNUMBER([code])", "false"),
        ("ISTEXT([code])", "true"),
        ("ISEVEN(4)", "true"),
        ("ISODD([qty])", "true"),
    ]);
}

#[test]
fn logical() {
    check(&[
        ("IFERROR(1 / 0, -1)", "-1.0"),
        ("IFERROR(VALUE('abc'), 0)", "0.0"),
        ("IFERROR([qty] * 2, 0)", "6"),
        ("IFNA(NA(), 5)", "5"),
        ("NOT([flag])", "false"),
        ("IFS([qty] > 5, 'big', [qty] > 2, 'mid', TRUE, 'small')", "mid"),
        ("SWITCH([qty], 1, 'one', 3, 'three', 'other')", "three"),
        ("SWITCH([qty], 1, 'one', 2, 'two')", "NULL"),
        ("CHOOSE(2, 'a', 'b', 'c')", "b"),
        ("AND([flag], [qty] > 2)", "true"),
        ("AND(TRUE, FALSE, TRUE)", "false"),
        ("OR([qty] > 5, [flag])", "true"),
        ("XOR(TRUE, TRUE)", "false"),
        ("XOR(TRUE, FALSE, FALSE)", "true"),
    ]);
}

#[test]
fn text() {
    check(&[
        ("MID([code], 4, 2)", "12"),
        ("FIND('-', [code])", "3"),
        ("FIND('b', [code])", "NULL"),
        ("FIND('B', 'ABAB', 3)", "4"),
        ("SEARCH('b', [code])", "2"),
        ("SUBSTITUTE([code], '-', '/')", "AB/12"),
        ("PROPER([name])", "Hello World"),
        ("CONCATENATE([code], NA(), '!')", "AB-12!"),
        ("TEXTJOIN('-', TRUE, 'a', '', NA(), 'b')", "a-b"),
        ("TEXTJOIN('-', FALSE, 'a', '', 'b')", "a--b"),
        ("TEXTJOIN('|', TRUE, [name:code])", "hELLO wORLD|AB-12"),
        ("VALUE([txt])", "12345.0"),
        ("REPT('ab', 3)", "ababab"),
        ("EXACT('a', 'A')", "false"),
        ("TEXTBEFORE('me@site.org', '@')", "me"),
        ("TEXTAFTER('me@site.org', '@')", "site.org"),
        ("TEXTAFTER('nothing', '@')", "NULL"),
    ]);
}

#[test]
fn dates() {
    check(&[
        ("DATE(2024, 13, 1)", "2025-01-01"),
        ("DATE(2024, 2, 30)", "2024-03-01"),
        ("TIME(9, 30, 5)", "09:30:05"),
        ("WEEKDAY(DATE(2024, 3, 4))", "2"),
        ("WEEKDAY(DATE(2024, 3, 4), 2)", "1"),
        ("WEEKDAY(DATE(2024, 3, 4), 3)", "0"),
        ("WEEKNUM(DATE(2024, 1, 7))", "2"),
        ("WEEKNUM(DATE(2024, 1, 7), 2)", "1"),
        ("ISOWEEKNUM(DATE(2024, 12, 30))", "1"),
        ("EDATE([d], 1)", "2024-02-29"),
        ("EDATE([d], -2)", "2023-11-30"),
        ("EOMONTH(DATE(2024, 1, 15), 1)", "2024-02-29"),
        ("DATEDIF(DATE(2020, 3, 6), DATE(2024, 3, 5), 'Y')", "3"),
        ("DATEDIF(DATE(2020, 3, 6), DATE(2024, 3, 5), 'M')", "47"),
        ("DATEDIF(DATE(2020, 3, 6), DATE(2024, 3, 5), 'D')", "1460"),
        ("DAYS(DATE(2024, 3, 15), DATE(2024, 3, 1))", "14"),
        ("NETWORKDAYS(DATE(2024, 3, 1), DATE(2024, 3, 31))", "21"),
        ("NETWORKDAYS(DATE(2024, 3, 8), DATE(2024, 3, 4))", "-5"),
        ("YEARFRAC(DATE(2024, 1, 1), DATE(2024, 7, 1))", "0.5"),
        ("DATEVALUE('2024-03-05')", "2024-03-05"),
        ("SECOND(DATEVALUE('2024-03-05'))", "0"),
    ]);
}

/// Cases evaluated on another row than the first.
fn check_row(row: usize, cases: &[(&str, &str)]) {
    let (conn, catalog) = setup();
    let failed: Vec<String> = cases
        .iter()
        .filter_map(|(f, want)| {
            let got = eval(&conn, &catalog, f, row);
            (got != *want).then(|| format!("{f}: got {got}, want {want}"))
        })
        .collect();
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}

#[test]
fn statistics_are_column_totals() {
    check(&[
        ("AVERAGE([amount])", "20.0"),
        ("[amount] / AVERAGE([amount])", "0.5"),
        ("COUNTA([code])", "2"),
        ("COUNTBLANK([code])", "1"),
        ("MEDIAN([qty])", "3.0"),
        ("MODE([region])", "North"),
        ("ROUND(STDEV.P([amount]), 4)", "8.165"),
        ("VAR.S([amount])", "100.0"),
        ("STDEV([amount])", "10.0"),
        ("LARGE([amount], 2)", "20.0"),
        ("SMALL([qty], 1)", "1"),
        ("PERCENTILE([amount], 0.5)", "20.0"),
        ("QUARTILE([amount], 1)", "15.0"),
        ("ROUND(CORREL([qty], [amount]), 4)", "-0.2774"),
        ("SUMPRODUCT([qty], [amount])", "220.0"),
    ]);
}

#[test]
fn conditional_totals() {
    check(&[
        ("COUNTIF([region], 'North')", "2"),
        ("COUNTIF([region], 'north')", "2"),
        ("COUNTIF([qty], '>2')", "2"),
        ("COUNTIF([qty], 8)", "1"),
        ("COUNTIF([region], 'S*')", "1"),
        ("COUNTIF([code], '')", "1"),
        ("COUNTIF([code], '<>')", "2"),
        ("COUNTIF([region], '<>North')", "1"),
        ("COUNTIF([d], '>=2024-03-01')", "2"),
        ("COUNTIFS([region], 'North', [qty], '>2')", "1"),
        ("SUMIF([region], 'North', [amount])", "40.0"),
        ("SUMIF([qty], '>2')", "11"),
        ("SUMIFS([amount], [region], 'North', [qty], '<5')", "40.0"),
        ("AVERAGEIF([region], 'North', [amount])", "20.0"),
        ("MAXIFS([amount], [region], 'North')", "30.0"),
        ("MINIFS([amount], [region], 'North')", "10.0"),
        // Compared with this row's value: rows like this one.
        ("COUNTIF([region], [region])", "2"),
        ("SUMIF([region], [region], [amount])", "40.0"),
        ("MAXIFS([amount], [region], [region], [qty], '>0')", "30.0"),
    ]);
    check_row(1, &[("COUNTIF([region], [region])", "1"), ("SUMIF([region], [region], [amount])", "20.0")]);
}

#[test]
fn text_formats() {
    check(&[
        ("TEXT([d], 'dd.mm.yyyy')", "31.01.2024"),
        ("TEXT([d], 'mmm yyyy')", "Jan 2024"),
        ("TEXT([d], 'mmmm d, yyyy')", "January 31, 2024"),
        ("TEXT([d], 'dddd')", "Wednesday"),
        ("TEXT(1234.5, '#,##0.00')", "1,234.50"),
        ("TEXT(0.256, '0.0%')", "25.6%"),
        ("TEXT([qty], '0')", "3"),
        ("TEXT([amount], '$#,##0.00')", "$10.00"),
        ("TEXT([qty], '0 \"pcs\"')", "3 pcs"),
    ]);
}

#[test]
fn lookups() {
    check(&[
        ("XLOOKUP([code], prices[id], prices[price])", "9.5"),
        ("XLOOKUP('zz', prices[id], prices[price], 0)", "0.0"),
        ("XLOOKUP('zz', prices[id], prices[price])", "NULL"),
        ("VLOOKUP([code], prices[id:price], 2)", "9.5"),
        ("VLOOKUP([code], prices[id:price], 2, FALSE) * [qty]", "28.5"),
        ("IFNA(XLOOKUP([name], prices[id], prices[price]), -1)", "-1.0"),
    ]);
    let (_, catalog) = setup();
    let reg = FunctionRegistry::default();
    let e = |f: &str| parvu_dsl::compile_formula(f, &catalog, &reg, "t").unwrap_err().to_string();
    assert!(e("VLOOKUP([code], prices[id:price], 2, TRUE)").contains("exact"));
    assert!(e("VLOOKUP([code], prices[id:price], 3)").contains("1 to 2"));
    assert!(e("XLOOKUP([code], t[code], t[name])").contains("another open table"));
    assert!(e("COUNTIF([region], [code])").contains("own column"));
    assert!(e("TEXT([d], [code])").contains("format"));
}

/// Every registered function's example compiles.
#[test]
fn every_example_compiles() {
    let reg = FunctionRegistry::default();
    let mut catalog = Catalog::new();
    let mut failed = Vec::new();
    for f in reg.all_functions() {
        if !parvu_dsl::excel::is_excel(&f.name) {
            continue;
        }
        // The columns an example uses, as untyped columns of one table.
        let refs: Vec<String> = f
            .example
            .split('[')
            .skip(1)
            .filter_map(|s| s.split(']').next())
            .map(|s| s.split(':').next().unwrap_or(s).to_string())
            .collect();
        let mut c = Catalog::new();
        c.register_table("t", refs.iter().map(|r| (r.clone(), "VARCHAR".to_string())));
        c.register_table("prices", [("id".to_string(), "VARCHAR".to_string()), ("price".to_string(), "DOUBLE".to_string())]);
        if let Err(e) = parvu_dsl::compile_formula(&f.example, &c, &reg, "t") {
            failed.push(format!("{}: {} ({e})", f.name, f.example));
        }
        catalog = c;
    }
    let _ = catalog;
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}

/// Every formula in the guide's examples (lines "= …" in the English
/// locale) compiles.
#[test]
fn guide_examples_compile() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../parvu-config/locales/en.json");
    let locale: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let text = locale["expr.examples_text"].as_str().unwrap();
    let reg = FunctionRegistry::default();
    let mut failed = Vec::new();
    for line in text.lines().filter(|l| l.starts_with("= ")) {
        let formula = line.split("     (").next().unwrap().trim();
        let mut cols: Vec<String> = Vec::new();
        for part in formula.split('[').skip(1) {
            let inner = part.split(']').next().unwrap_or("");
            for c in inner.split(':') {
                if !cols.contains(&c.to_string()) {
                    cols.push(c.to_string());
                }
            }
        }
        let mut c = Catalog::new();
        c.register_table("t", cols.iter().map(|n| (n.clone(), "VARCHAR".to_string())));
        c.register_table("customers", [("id", "VARCHAR"), ("name", "VARCHAR")].map(|(a, b)| (a.to_string(), b.to_string())));
        c.register_table("prices", [("sku", "VARCHAR"), ("price", "DOUBLE")].map(|(a, b)| (a.to_string(), b.to_string())));
        if let Err(e) = parvu_dsl::compile_formula(formula, &c, &reg, "t") {
            failed.push(format!("{formula}: {e}"));
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}
