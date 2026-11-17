//! Excel's functions under their Excel names and argument order, so people
//! who know Excel can write formulas the way they're used to. Most are SQL
//! templates over DuckDB functions (`{0}`, `{1}`… are the arguments).
//!
//! Functions that take any number of arguments (`IFS`, `SWITCH`, `AND`,
//! `TEXTJOIN`…) are built in `resolver.rs`; they are registered here too
//! for autocomplete and the reference.

use crate::registry::{FunctionDef, FunctionRegistry, ParamDef};
use crate::types::LogicalType as LT;

fn template(name: &str, params: Vec<ParamDef>, ret: LT, desc: &str, example: &str, sql: &str) -> FunctionDef {
    let mut f = FunctionDef::new(name, params, ret, desc, example, None);
    f.sql_template = Some(sql.to_string());
    f
}

/// Registered for autocomplete and the reference; resolved in `resolver.rs`.
fn special(name: &str, params: Vec<ParamDef>, ret: LT, desc: &str, example: &str) -> FunctionDef {
    FunctionDef::new(name, params, ret, desc, example, None)
}

/// Functions resolved by the resolver because they take any number of
/// arguments (or ranges).
pub const VARIADIC: &[&str] = &["IFS", "SWITCH", "CHOOSE", "AND", "OR", "XOR", "TEXTJOIN", "CONCATENATE", "PRODUCT", "SUMPRODUCT"];

/// Conditional totals with Excel's criteria, built in the resolver.
pub const CONDITIONAL: &[&str] =
    &["COUNTIF", "COUNTIFS", "SUMIF", "SUMIFS", "AVERAGEIF", "AVERAGEIFS", "MAXIFS", "MINIFS"];

/// Excel functions that total a whole column (cached totals in formulas).
pub const AGGREGATES: &[&str] = &[
    "AVERAGE", "COUNTA", "COUNTBLANK", "MEDIAN", "MODE", "MODE.SNGL", "STDEV", "STDEV.S", "STDEV.P", "VAR", "VAR.S",
    "VAR.P", "LARGE", "SMALL", "PERCENTILE", "PERCENTILE.INC", "QUARTILE", "QUARTILE.INC", "CORREL",
];

pub(crate) fn register(reg: &mut FunctionRegistry) {
    let p = ParamDef::new;
    let d = ParamDef::with_default;
    let opt = ParamDef::optional;
    let mut add = |f: FunctionDef| reg.register(f);

    // ---------------------------------------------------------------- math
    add(template("ROUNDUP", vec![p("number", LT::Numeric), d("digits", LT::Numeric, "0")], LT::Numeric,
        "Rounds away from zero to `digits` decimals.", "ROUNDUP([price] * 1.18, 2)",
        "CASE WHEN {0} >= 0 THEN ceil({0} * power(10, {1})) ELSE floor({0} * power(10, {1})) END / power(10, {1})"));
    add(template("ROUNDDOWN", vec![p("number", LT::Numeric), d("digits", LT::Numeric, "0")], LT::Numeric,
        "Rounds towards zero to `digits` decimals.", "ROUNDDOWN([price], 1)",
        "trunc({0} * power(10, {1})) / power(10, {1})"));
    add(template("TRUNC", vec![p("number", LT::Numeric), d("digits", LT::Numeric, "0")], LT::Numeric,
        "Cuts off the decimals after `digits` (towards zero).", "TRUNC([price])",
        "trunc({0} * power(10, {1})) / power(10, {1})"));
    add(template("MROUND", vec![p("number", LT::Numeric), p("multiple", LT::Numeric)], LT::Numeric,
        "Rounds to the nearest multiple.", "MROUND([minutes], 15)", "round({0} / {1}) * {1}"));
    add(template("INT", vec![p("number", LT::Numeric)], LT::Numeric,
        "Rounds down to the whole number below.", "INT([price])", "floor({0})"));
    add(template("CEILING", vec![p("number", LT::Numeric), d("significance", LT::Numeric, "1")], LT::Numeric,
        "Rounds up to a multiple of `significance`.", "CEILING([price], 0.05)", "ceil({0} / {1}) * {1}"));
    add(template("FLOOR.MATH", vec![p("number", LT::Numeric), d("significance", LT::Numeric, "1")], LT::Numeric,
        "Rounds down to a multiple of `significance` (Excel's FLOOR(number, significance) means the same).",
        "FLOOR.MATH([price], 0.5)", "floor({0} / {1}) * {1}"));
    add(template("LOG", vec![p("number", LT::Numeric), d("base", LT::Numeric, "10")], LT::Numeric,
        "Logarithm in `base` (10 by default).", "LOG([value], 2)",
        "CASE {1} WHEN 10 THEN log10({0}) WHEN 2 THEN log2({0}) ELSE ln({0}) / ln({1}) END"));
    add(template("PI", vec![], LT::Numeric, "The number π.", "PI() * POWER([radius], 2)", "pi()"));
    add(template("QUOTIENT", vec![p("numerator", LT::Numeric), p("denominator", LT::Numeric)], LT::Numeric,
        "The whole-number part of a division.", "QUOTIENT([minutes], 60)", "trunc({0} / {1})"));
    add(special("PRODUCT", vec![p("number1", LT::Numeric), opt("number2", LT::Numeric)], LT::Numeric,
        "Multiplies its arguments (a range like [a:c] counts as its columns).", "PRODUCT([qty], [price])"));
    add(template("RAND", vec![], LT::Numeric,
        "A random number from 0 to 1 (new each time the table is read).", "RAND()", "random()"));
    add(template("RANDBETWEEN", vec![p("bottom", LT::Numeric), p("top", LT::Numeric)], LT::Numeric,
        "A random whole number from `bottom` to `top` (new each time the table is read).", "RANDBETWEEN(1, 6)",
        "CAST(floor(random() * ({1} - {0} + 1)) + {0} AS BIGINT)"));

    // ---------------------------------------------------------------- statistics (column totals)
    let col = |n: &str| p(n, LT::Numeric);
    add(FunctionDef::new("AVERAGE", vec![col("column")], LT::Numeric, "The column's average (same as AVG).", "[price] / AVERAGE([price])", Some("AVG")));
    add(template("COUNTA", vec![p("column", LT::Unknown)], LT::Numeric, "How many values the column has (empty ones left out).", "COUNTA([email])", "count({0})"));
    add(template("COUNTBLANK", vec![p("column", LT::Unknown)], LT::Numeric, "How many values of the column are empty.", "COUNTBLANK([email])",
        "count(*) FILTER (WHERE {0} IS NULL OR CAST({0} AS VARCHAR) = '')"));
    for (name, sql, desc) in [
        ("MEDIAN", "median({0})", "The column's median (middle value)."),
        ("MODE", "mode({0})", "The column's most frequent value."),
        ("MODE.SNGL", "mode({0})", "The column's most frequent value."),
        ("STDEV", "stddev_samp({0})", "The column's standard deviation (sample)."),
        ("STDEV.S", "stddev_samp({0})", "The column's standard deviation (sample)."),
        ("STDEV.P", "stddev_pop({0})", "The column's standard deviation (whole population)."),
        ("VAR", "var_samp({0})", "The column's variance (sample)."),
        ("VAR.S", "var_samp({0})", "The column's variance (sample)."),
        ("VAR.P", "var_pop({0})", "The column's variance (whole population)."),
    ] {
        add(template(name, vec![col("column")], LT::Numeric, desc, &format!("{name}([price])"), sql));
    }
    add(template("LARGE", vec![col("column"), col("k")], LT::Numeric, "The k-th largest value of the column.", "LARGE([score], 3)",
        "list_extract(max({0}, CAST({1} AS BIGINT)), CAST({1} AS BIGINT))"));
    add(template("SMALL", vec![col("column"), col("k")], LT::Numeric, "The k-th smallest value of the column.", "SMALL([score], 1)",
        "list_extract(min({0}, CAST({1} AS BIGINT)), CAST({1} AS BIGINT))"));
    for name in ["PERCENTILE", "PERCENTILE.INC"] {
        add(template(name, vec![col("column"), col("k")], LT::Numeric, "The column's k-th percentile, k from 0 to 1.", &format!("{name}([price], 0.9)"),
            "quantile_cont({0}, {1})"));
    }
    for name in ["QUARTILE", "QUARTILE.INC"] {
        add(template(name, vec![col("column"), col("quart")], LT::Numeric, "The column's quartile: 0 minimum, 1, 2 (median), 3, 4 maximum.",
            &format!("{name}([price], 1)"), "quantile_cont({0}, {1} / 4)"));
    }
    add(template("CORREL", vec![col("column1"), col("column2")], LT::Numeric, "The correlation of two columns (-1 to 1).", "CORREL([ads], [sales])",
        "corr({0}, {1})"));
    add(special("SUMPRODUCT", vec![col("column1"), col("column2")], LT::Numeric,
        "Multiplies the columns row by row and adds it all up: the column total of the products.", "SUMPRODUCT([qty], [price])"));

    // ---------------------------------------------------------------- conditional totals
    let crit = |n: &str| p(n, LT::Unknown);
    add(special("COUNTIF", vec![p("range", LT::Unknown), crit("criteria")], LT::Numeric,
        "How many rows meet the criteria: a value ('North', 5) or a comparison ('>10', '<>0', '>=2024-01-01'); * and ? are wildcards. With a column of this row, e.g. COUNTIF([region], [region]), it counts the rows like this one.",
        "COUNTIF([region], 'North')"));
    add(special("COUNTIFS", vec![p("range1", LT::Unknown), crit("criteria1"), opt("range2", LT::Unknown), opt("criteria2", LT::Unknown)], LT::Numeric,
        "How many rows meet every criteria (pairs of range and criteria).", "COUNTIFS([region], 'North', [qty], '>5')"));
    add(special("SUMIF", vec![p("range", LT::Unknown), crit("criteria"), opt("sum_range", LT::Numeric)], LT::Numeric,
        "The total of `sum_range` (or `range`) over the rows that meet the criteria.", "SUMIF([region], 'North', [amount])"));
    add(special("SUMIFS", vec![p("sum_range", LT::Numeric), p("range1", LT::Unknown), crit("criteria1")], LT::Numeric,
        "The total of `sum_range` over the rows that meet every criteria.", "SUMIFS([amount], [region], 'North', [qty], '>5')"));
    add(special("AVERAGEIF", vec![p("range", LT::Unknown), crit("criteria"), opt("average_range", LT::Numeric)], LT::Numeric,
        "The average of `average_range` (or `range`) over the rows that meet the criteria.", "AVERAGEIF([qty], '>0')"));
    add(special("AVERAGEIFS", vec![p("average_range", LT::Numeric), p("range1", LT::Unknown), crit("criteria1")], LT::Numeric,
        "The average of `average_range` over the rows that meet every criteria.", "AVERAGEIFS([amount], [region], 'North')"));
    add(special("MAXIFS", vec![p("max_range", LT::Numeric), p("range1", LT::Unknown), crit("criteria1")], LT::Numeric,
        "The largest `max_range` value among the rows that meet every criteria.", "MAXIFS([amount], [region], [region])"));
    add(special("MINIFS", vec![p("min_range", LT::Numeric), p("range1", LT::Unknown), crit("criteria1")], LT::Numeric,
        "The smallest `min_range` value among the rows that meet every criteria.", "MINIFS([amount], [region], 'North')"));

    // ---------------------------------------------------------------- lookups and TEXT
    add(special("XLOOKUP", vec![p("lookup_value", LT::Unknown), p("lookup_column", LT::Unknown), p("return_column", LT::Unknown), opt("if_not_found", LT::Unknown)], LT::Unknown,
        "Finds `lookup_value` in a column of another table and returns the same row's value from another column; `if_not_found` (or empty) when there is no match.",
        "XLOOKUP([id], prices[id], prices[price], 0)"));
    add(special("VLOOKUP", vec![p("lookup_value", LT::Unknown), p("table_range", LT::Unknown), p("column_number", LT::Numeric), opt("range_lookup", LT::Boolean)], LT::Unknown,
        "Finds `lookup_value` in the first column of a range of another table (prices[id:price]) and returns column `column_number` of that row. Exact matches only.",
        "VLOOKUP([id], prices[id:price], 2, FALSE)"));
    add(special("TEXT", vec![p("value", LT::Unknown), p("format_text", LT::Text)], LT::Text,
        "The value as text in an Excel format: '0.00', '#,##0', '0%', 'dd.mm.yyyy', 'mmm yyyy', 'dddd', 'hh:mm'…",
        "TEXT([date], 'dd.mm.yyyy')"));

    // ---------------------------------------------------------------- information
    add(template("ISBLANK", vec![p("value", LT::Unknown)], LT::Boolean,
        "TRUE when the value is empty (NULL or empty text).", "ISBLANK([email])",
        "{0} IS NULL OR CAST({0} AS VARCHAR) = ''"));
    add(template("ISNUMBER", vec![p("value", LT::Unknown)], LT::Boolean,
        "TRUE when the value is a number (not text that looks like one).", "ISNUMBER([amount])",
        "{0} IS NOT NULL AND (typeof({0}) IN ('TINYINT','SMALLINT','INTEGER','BIGINT','HUGEINT','UTINYINT','USMALLINT','UINTEGER','UBIGINT','UHUGEINT','FLOAT','DOUBLE') OR typeof({0}) LIKE 'DECIMAL%')"));
    add(template("ISTEXT", vec![p("value", LT::Unknown)], LT::Boolean,
        "TRUE when the value is text.", "ISTEXT([code])", "{0} IS NOT NULL AND typeof({0}) = 'VARCHAR'"));
    add(template("ISEVEN", vec![p("number", LT::Numeric)], LT::Boolean,
        "TRUE for even numbers.", "ISEVEN([row_id])", "CAST(trunc({0}) AS BIGINT) % 2 = 0"));
    add(template("ISODD", vec![p("number", LT::Numeric)], LT::Boolean,
        "TRUE for odd numbers.", "ISODD([row_id])", "CAST(trunc({0}) AS BIGINT) % 2 <> 0"));
    add(template("NA", vec![], LT::Unknown, "An empty value (Excel's #N/A).", "IF([qty] > 0, [qty], NA())", "NULL"));

    // ---------------------------------------------------------------- logical
    add(template("IFERROR", vec![p("value", LT::Unknown), p("value_if_error", LT::Unknown)], LT::Unknown,
        "`value`, or `value_if_error` when it fails (bad conversion, division by zero) or is empty.",
        "IFERROR([total] / [qty], 0)",
        "COALESCE(TRY(CASE WHEN TRY(isinf(CAST({0} AS DOUBLE))) THEN NULL ELSE {0} END), {1})"));
    add(template("IFNA", vec![p("value", LT::Unknown), p("value_if_na", LT::Unknown)], LT::Unknown,
        "`value`, or `value_if_na` when it is empty.", "IFNA(XLOOKUP([id], prices[id], prices[price]), 0)",
        "COALESCE({0}, {1})"));
    add(template("NOT", vec![p("logical", LT::Boolean)], LT::Boolean, "Reverses TRUE and FALSE.", "NOT([returned])", "NOT {0}"));
    add(special("IFS", vec![p("condition1", LT::Boolean), p("value1", LT::Unknown), opt("condition2", LT::Boolean), opt("value2", LT::Unknown)], LT::Unknown,
        "The value of the first condition that is TRUE; empty when none is. Pairs of condition, value.",
        "IFS([score] >= 90, 'A', [score] >= 80, 'B', TRUE, 'C')"));
    add(special("SWITCH", vec![p("expression", LT::Unknown), p("value1", LT::Unknown), p("result1", LT::Unknown), opt("default", LT::Unknown)], LT::Unknown,
        "Compares `expression` with each value and returns the matching result; an odd last argument is the default.",
        "SWITCH([code], 'N', 'North', 'S', 'South', 'Other')"));
    add(special("CHOOSE", vec![p("index", LT::Numeric), p("value1", LT::Unknown), opt("value2", LT::Unknown)], LT::Unknown,
        "The value at position `index` (1, 2, …).", "CHOOSE(WEEKDAY([date], 2), 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun')"));
    add(special("AND", vec![p("logical1", LT::Boolean), opt("logical2", LT::Boolean)], LT::Boolean,
        "TRUE when every argument is TRUE.", "AND([qty] > 0, [price] > 0)"));
    add(special("OR", vec![p("logical1", LT::Boolean), opt("logical2", LT::Boolean)], LT::Boolean,
        "TRUE when any argument is TRUE.", "OR([region] = 'North', [region] = 'East')"));
    add(special("XOR", vec![p("logical1", LT::Boolean), opt("logical2", LT::Boolean)], LT::Boolean,
        "TRUE when an odd number of arguments is TRUE.", "XOR([paid], [refunded])"));

    // ---------------------------------------------------------------- text
    add(template("MID", vec![p("text", LT::Text), p("start", LT::Numeric), p("length", LT::Numeric)], LT::Text,
        "`length` characters starting at position `start` (1 is the first).", "MID([code], 3, 2)",
        "substr({0}, {1}, {2})"));
    add(template("FIND", vec![p("find_text", LT::Text), p("within_text", LT::Text), d("start", LT::Numeric, "1")], LT::Numeric,
        "Position of `find_text` in `within_text` (case-sensitive); empty when not found.", "FIND('-', [code])",
        "NULLIF(instr(substr({1}, {2}), {0}), 0) + {2} - 1"));
    add(template("SEARCH", vec![p("find_text", LT::Text), p("within_text", LT::Text), d("start", LT::Numeric, "1")], LT::Numeric,
        "Position of `find_text` in `within_text`, ignoring case; empty when not found.", "SEARCH('ltd', [company])",
        "NULLIF(instr(lower(substr({1}, {2})), lower({0})), 0) + {2} - 1"));
    add(template("SUBSTITUTE", vec![p("text", LT::Text), p("old_text", LT::Text), p("new_text", LT::Text)], LT::Text,
        "Replaces every `old_text` with `new_text`.", "SUBSTITUTE([phone], ' ', '')", "replace({0}, {1}, {2})"));
    add(template("PROPER", vec![p("text", LT::Text)], LT::Text,
        "Capitalises the first letter of each word.", "PROPER([name])",
        "array_to_string(list_transform(string_split(lower({0}), ' '), lambda w: upper(w[1]) || w[2:]), ' ')"));
    add(template("CLEAN", vec![p("text", LT::Text)], LT::Text,
        "Removes non-printable characters (line breaks, tabs…).", "CLEAN([comment])",
        "regexp_replace({0}, '[\\x00-\\x1F\\x7F]', '', 'g')"));
    add(special("CONCATENATE", vec![p("text1", LT::Text), opt("text2", LT::Text)], LT::Text,
        "Joins texts (empty values count as empty text).", "CONCATENATE([first], ' ', [last])"));
    add(special("TEXTJOIN", vec![p("delimiter", LT::Text), p("ignore_empty", LT::Boolean), p("text1", LT::Text), opt("text2", LT::Text)], LT::Text,
        "Joins texts with `delimiter`, skipping empty ones when `ignore_empty` is TRUE. A range [a:c] counts as its columns.",
        "TEXTJOIN(', ', TRUE, [street], [city], [zip])"));
    add(template("VALUE", vec![p("text", LT::Text)], LT::Numeric,
        "The number in a text (thousands commas allowed); empty when it isn't one.", "VALUE([amount_text])",
        "TRY_CAST(replace(trim(CAST({0} AS VARCHAR)), ',', '') AS DOUBLE)"));
    add(template("REPT", vec![p("text", LT::Text), p("times", LT::Numeric)], LT::Text,
        "Repeats `text`.", "REPT('*', [stars])", "repeat({0}, CAST({1} AS BIGINT))"));
    add(template("EXACT", vec![p("text1", LT::Text), p("text2", LT::Text)], LT::Boolean,
        "TRUE when both texts are exactly equal, case included.", "EXACT([code], [code_check])", "{0} = {1}"));
    add(template("TEXTBEFORE", vec![p("text", LT::Text), p("delimiter", LT::Text)], LT::Text,
        "The text before the first `delimiter`; empty when there is none.", "TEXTBEFORE([email], '@')",
        "CASE WHEN instr({0}, {1}) > 0 THEN substr({0}, 1, instr({0}, {1}) - 1) END"));
    add(template("TEXTAFTER", vec![p("text", LT::Text), p("delimiter", LT::Text)], LT::Text,
        "The text after the first `delimiter`; empty when there is none.", "TEXTAFTER([email], '@')",
        "CASE WHEN instr({0}, {1}) > 0 THEN substr({0}, instr({0}, {1}) + length({1})) END"));

    // ---------------------------------------------------------------- dates
    add(template("DATE", vec![p("year", LT::Numeric), p("month", LT::Numeric), p("day", LT::Numeric)], LT::Date,
        "The date from a year, month and day; months and days past the end roll over (month 13 is January).",
        "DATE([year], [month], 1)",
        "CAST(make_date(CAST({0} AS INTEGER), 1, 1) + to_months(CAST({1} AS INTEGER) - 1) + to_days(CAST({2} AS INTEGER) - 1) AS DATE)"));
    add(template("TIME", vec![p("hour", LT::Numeric), p("minute", LT::Numeric), p("second", LT::Numeric)], LT::Unknown,
        "The time of day.", "TIME(9, 30, 0)", "make_time(CAST({0} AS BIGINT), CAST({1} AS BIGINT), CAST({2} AS DOUBLE))"));
    add(template("SECOND", vec![p("time", LT::Timestamp)], LT::Numeric, "The seconds (0–59).", "SECOND([created])", "second({0})"));
    add(template("WEEKDAY", vec![p("date", LT::Date), d("return_type", LT::Numeric, "1")], LT::Numeric,
        "The day of the week: 1 (Sunday) to 7 by default; type 2: 1 (Monday) to 7; type 3: 0 (Monday) to 6.",
        "WEEKDAY([date], 2)",
        "CASE {1} WHEN 2 THEN isodow({0}) WHEN 3 THEN isodow({0}) - 1 ELSE dayofweek({0}) + 1 END"));
    add(template("WEEKNUM", vec![p("date", LT::Date), d("return_type", LT::Numeric, "1")], LT::Numeric,
        "The week of the year; the week with 1 January is week 1. Weeks start on Sunday (type 1) or Monday (type 2).",
        "WEEKNUM([date])",
        "CAST(floor((dayofyear({0}) - 1 + CASE WHEN {1} = 2 THEN isodow(make_date(year({0}), 1, 1)) - 1 ELSE dayofweek(make_date(year({0}), 1, 1)) END) / 7) + 1 AS INTEGER)"));
    add(template("ISOWEEKNUM", vec![p("date", LT::Date)], LT::Numeric,
        "The ISO week of the year (weeks start on Monday).", "ISOWEEKNUM([date])", "weekofyear({0})"));
    add(template("EDATE", vec![p("start_date", LT::Date), p("months", LT::Numeric)], LT::Date,
        "The date `months` months later (earlier when negative); 31 January + 1 is 28 or 29 February.",
        "EDATE([start], 3)", "CAST({0} + to_months(CAST({1} AS INTEGER)) AS DATE)"));
    add(template("EOMONTH", vec![p("start_date", LT::Date), p("months", LT::Numeric)], LT::Date,
        "The last day of the month, `months` months later.", "EOMONTH([date], 0)",
        "last_day(CAST({0} + to_months(CAST({1} AS INTEGER)) AS DATE))"));
    add(template("DATEDIF", vec![p("start_date", LT::Date), p("end_date", LT::Date), p("unit", LT::Text)], LT::Numeric,
        "Complete years (\"Y\"), months (\"M\") or days (\"D\") between two dates; \"MD\" and \"YM\" give the days or months left over.",
        "DATEDIF([birth], TODAY(), 'Y')",
        "CASE upper({2}) WHEN 'Y' THEN date_part('year', age(CAST({1} AS TIMESTAMP), CAST({0} AS TIMESTAMP))) WHEN 'M' THEN date_part('year', age(CAST({1} AS TIMESTAMP), CAST({0} AS TIMESTAMP))) * 12 + date_part('month', age(CAST({1} AS TIMESTAMP), CAST({0} AS TIMESTAMP))) WHEN 'D' THEN date_diff('day', CAST({0} AS DATE), CAST({1} AS DATE)) WHEN 'MD' THEN date_part('day', age(CAST({1} AS TIMESTAMP), CAST({0} AS TIMESTAMP))) WHEN 'YM' THEN date_part('month', age(CAST({1} AS TIMESTAMP), CAST({0} AS TIMESTAMP))) END"));
    add(template("DAYS", vec![p("end_date", LT::Date), p("start_date", LT::Date)], LT::Numeric,
        "Days from `start_date` to `end_date`.", "DAYS([due], TODAY())",
        "date_diff('day', CAST({1} AS DATE), CAST({0} AS DATE))"));
    add(template("NETWORKDAYS", vec![p("start_date", LT::Date), p("end_date", LT::Date)], LT::Numeric,
        "Working days (Monday to Friday) from `start_date` to `end_date`, both included.", "NETWORKDAYS([start], [end])",
        "CASE WHEN CAST({0} AS DATE) <= CAST({1} AS DATE) THEN len(list_filter(range(CAST(CAST({0} AS DATE) AS TIMESTAMP), CAST(CAST({1} AS DATE) AS TIMESTAMP) + INTERVAL 1 DAY, INTERVAL 1 DAY), lambda x: isodow(x) < 6)) ELSE -len(list_filter(range(CAST(CAST({1} AS DATE) AS TIMESTAMP), CAST(CAST({0} AS DATE) AS TIMESTAMP) + INTERVAL 1 DAY, INTERVAL 1 DAY), lambda x: isodow(x) < 6)) END"));
    add(template("YEARFRAC", vec![p("start_date", LT::Date), p("end_date", LT::Date)], LT::Numeric,
        "The fraction of a year between two dates (30/360, Excel's default basis).", "YEARFRAC([start], [end])",
        "((year({1}) - year({0})) * 360 + (month({1}) - month({0})) * 30 + (CASE WHEN day({1}) = 31 AND least(day({0}), 30) >= 30 THEN 30 ELSE day({1}) END - least(day({0}), 30))) / 360.0"));
    add(template("DATEVALUE", vec![p("date_text", LT::Text)], LT::Date,
        "The date in a text like 2024-03-31; empty when it isn't one.", "DATEVALUE([date_text])",
        "TRY_CAST({0} AS DATE)"));
}

/// Whether `name` is one of Excel's functions added here.
pub fn is_excel(name: &str) -> bool {
    let mut reg = FunctionRegistry::empty();
    register(&mut reg);
    reg.lookup(name).is_some()
}

// ---------------------------------------------------------------- the catalogue

/// One of Excel's functions as the reference shows it: the ParVu function
/// that does the job (often the same name), or what to use instead.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ExcelEntry {
    pub name: &'static str,
    /// Category key: math, statistics, conditional, logical, information,
    /// text, date, lookup, tables.
    pub category: &'static str,
    /// The ParVu function to write (in the function registry).
    pub function: Option<&'static str>,
    /// For the functions that don't fit a formula: what does the job.
    pub instead: Option<&'static str>,
}

/// Excel's most used functions, by category.
pub fn catalog() -> Vec<ExcelEntry> {
    let f = |category, name, function| ExcelEntry { name, category, function: Some(function), instead: None };
    let use_ = |category, name, instead| ExcelEntry { name, category, function: None, instead: Some(instead) };
    let same = |category: &'static str, names: &[&'static str]| -> Vec<ExcelEntry> {
        names.iter().map(|n| f(category, *n, *n)).collect()
    };
    let mut out = Vec::new();
    out.extend(same("math", &["SUM", "SUMPRODUCT", "PRODUCT", "ROUND", "ROUNDUP", "ROUNDDOWN", "MROUND", "INT", "TRUNC", "CEILING"]));
    out.push(f("math", "FLOOR", "FLOOR.MATH"));
    out.extend(same("math", &["ABS", "SIGN", "MOD", "POWER", "SQRT", "EXP", "LN", "LOG", "LOG10", "PI", "QUOTIENT", "RAND", "RANDBETWEEN"]));
    out.extend(same("statistics", &[
        "AVERAGE", "COUNT", "COUNTA", "COUNTBLANK", "MIN", "MAX", "MEDIAN", "MODE", "STDEV", "STDEV.P", "VAR", "VAR.P", "LARGE",
        "SMALL", "PERCENTILE", "QUARTILE", "CORREL",
    ]));
    out.push(use_("statistics", "RANK", "Column → Transform → Running Totals, Ranks and Rolling → Rank"));
    out.extend(same("conditional", &["COUNTIF", "COUNTIFS", "SUMIF", "SUMIFS", "AVERAGEIF", "AVERAGEIFS", "MAXIFS", "MINIFS"]));
    out.extend(same("logical", &["IF", "IFS", "IFERROR", "IFNA", "AND", "OR", "NOT", "XOR", "SWITCH", "CHOOSE"]));
    out.push(use_("logical", "TRUE", "Write TRUE"));
    out.push(use_("logical", "FALSE", "Write FALSE"));
    out.extend(same("information", &["ISBLANK", "ISNUMBER", "ISTEXT", "ISEVEN", "ISODD", "NA"]));
    out.extend(same("text", &["LEFT", "RIGHT", "MID", "LEN", "FIND", "SEARCH", "SUBSTITUTE"]));
    out.push(use_("text", "REPLACE", "LEFT, MID and RIGHT for positions; ParVu's REPLACE(text, old, new) replaces text"));
    out.extend(same("text", &[
        "UPPER", "LOWER", "PROPER", "TRIM", "CLEAN", "CONCAT", "CONCATENATE", "TEXTJOIN", "TEXT", "VALUE", "REPT", "EXACT",
        "TEXTBEFORE", "TEXTAFTER",
    ]));
    out.extend(same("date", &[
        "TODAY", "NOW", "DATE", "TIME", "YEAR", "MONTH", "DAY", "HOUR", "MINUTE", "SECOND", "WEEKDAY", "WEEKNUM", "ISOWEEKNUM",
        "EDATE", "EOMONTH", "DATEDIF", "DAYS", "NETWORKDAYS", "YEARFRAC", "DATEVALUE",
    ]));
    out.extend(same("lookup", &["XLOOKUP", "VLOOKUP"]));
    out.push(use_("lookup", "INDEX", "XLOOKUP to find a value; Rows → First, Last or Random Rows for rows by position"));
    out.push(use_("lookup", "MATCH", "XLOOKUP to find a value; FIND or SEARCH for a position in text"));
    out.push(use_("lookup", "HLOOKUP", "Table → Transpose, then XLOOKUP"));
    out.push(use_("lookup", "LOOKUP", "XLOOKUP"));
    out.push(use_("lookup", "OFFSET", "PREV and NEXT for neighbouring rows"));
    out.push(use_("tables", "FILTER", "Rows → Filter Rows"));
    out.push(use_("tables", "SORT", "Rows → Sort"));
    out.push(use_("tables", "UNIQUE", "Column → Analyze → Unique Values, or Rows → Drop Duplicates"));
    out.push(use_("tables", "TRANSPOSE", "Table → Transpose"));
    out.push(use_("tables", "SUBTOTAL", "Table → Group By"));
    out.push(use_("tables", "GROUPBY", "Table → Group By"));
    out.push(use_("tables", "PIVOTBY", "Table → Pivot Table"));
    out
}

// ---------------------------------------------------------------- criteria

fn sql_text(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// An Excel criterion (`"North"`, `">10"`, `"<>"`, `"a*"`, `5`) as a SQL
/// condition on argument `{i}`. Text compares ignoring case, as in Excel.
pub fn criterion(i: usize, value: &crate::ir::LiteralValue) -> String {
    use crate::ir::LiteralValue as L;
    let x = format!("({{{i}}})");
    let text = format!("lower(CAST({x} AS VARCHAR))");
    let blank = format!("({x} IS NULL OR CAST({x} AS VARCHAR) = '')");
    let s = match value {
        L::Null => return blank,
        L::Bool(b) => return format!("{x} = {}", if *b { "TRUE" } else { "FALSE" }),
        L::Int(n) => return format!("TRY_CAST({x} AS DOUBLE) = {n}"),
        L::Float(f) => return format!("TRY_CAST({x} AS DOUBLE) = {f}"),
        L::Str(s) => s.as_str(),
    };
    let (op, rest) = ["<>", ">=", "<=", "=", ">", "<"]
        .iter()
        .find_map(|op| s.strip_prefix(op).map(|r| (*op, r)))
        .unwrap_or(("=", s));
    // "<>" also counts empty cells, as in Excel.
    let or_blank = |cond: String| if op == "<>" { format!("({cond} OR {x} IS NULL)") } else { cond };
    if rest.is_empty() {
        return if op == "<>" { format!("NOT {blank}") } else { blank };
    }
    if let Ok(n) = rest.trim().parse::<f64>() {
        return or_blank(format!("TRY_CAST({x} AS DOUBLE) {op} {n}"));
    }
    let is_date = rest.len() == 10 && rest.as_bytes()[4] == b'-' && rest.as_bytes()[7] == b'-' && rest.chars().filter(char::is_ascii_digit).count() == 8;
    if is_date {
        return or_blank(format!("TRY_CAST({x} AS DATE) {op} DATE {}", sql_text(rest)));
    }
    let lower = rest.to_lowercase();
    if (op == "=" || op == "<>") && (lower.contains('*') || lower.contains('?')) {
        let mut pattern = String::new();
        for c in lower.chars() {
            match c {
                '*' => pattern.push('%'),
                '?' => pattern.push('_'),
                '%' | '_' | '\\' => {
                    pattern.push('\\');
                    pattern.push(c);
                }
                c => pattern.push(c),
            }
        }
        let like = if op == "=" { "LIKE" } else { "NOT LIKE" };
        return or_blank(format!("{text} {like} {} ESCAPE '\\'", sql_text(&pattern)));
    }
    or_blank(format!("{text} {op} {}", sql_text(&lower)))
}

// ---------------------------------------------------------------- TEXT formats

/// SQL for `TEXT({0}, format)` with an Excel format code.
pub fn text_format(format: &str) -> Result<String, String> {
    let f = format.trim();
    if f.is_empty() || f.eq_ignore_ascii_case("general") || f == "@" {
        return Ok("CAST(({0}) AS VARCHAR)".into());
    }
    let unquoted: String = strip_quoted(f).to_lowercase();
    let datey = unquoted.contains('y') || unquoted.contains('d') || unquoted.contains('h') || unquoted.contains('s');
    if datey || (unquoted.contains('m') && !unquoted.contains('0') && !unquoted.contains('#')) {
        return Ok(format!("strftime(CAST(({{0}}) AS TIMESTAMP), {})", sql_text(&date_pattern(f))));
    }
    if unquoted.contains('0') || unquoted.contains('#') {
        return number_format(f);
    }
    Err(format!("TEXT doesn't know the format '{format}'; try '0.00', '#,##0', '0%' or 'dd.mm.yyyy'"))
}

fn strip_quoted(f: &str) -> String {
    let mut out = String::new();
    let mut quoted = false;
    for c in f.chars() {
        if c == '"' {
            quoted = !quoted;
        } else if !quoted {
            out.push(c);
        }
    }
    out
}

/// Excel's date codes as a strftime pattern.
fn date_pattern(f: &str) -> String {
    let chars: Vec<char> = f.chars().collect();
    let twelve = f.to_uppercase().contains("AM/PM");
    let mut out = String::new();
    let mut i = 0;
    let mut last_was_hour = false;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '%' {
                    out.push_str("%%");
                } else {
                    out.push(chars[i]);
                }
                i += 1;
            }
            i += 1;
            continue;
        }
        if f[f.char_indices().nth(i).map(|(b, _)| b).unwrap_or(0)..].to_uppercase().starts_with("AM/PM") {
            out.push_str("%p");
            i += 5;
            continue;
        }
        let lc = c.to_ascii_lowercase();
        let mut n = 1;
        while i + n < chars.len() && chars[i + n].to_ascii_lowercase() == lc {
            n += 1;
        }
        let code = match lc {
            'y' => Some(if n <= 2 { "%y" } else { "%Y" }),
            'd' => Some(match n {
                1 => "%-d",
                2 => "%d",
                3 => "%a",
                _ => "%A",
            }),
            'h' => Some(match (twelve, n) {
                (true, 1) => "%-I",
                (true, _) => "%I",
                (false, 1) => "%-H",
                (false, _) => "%H",
            }),
            's' => Some(if n == 1 { "%-S" } else { "%S" }),
            'm' => {
                // Minutes right after hours or right before seconds.
                let rest: String = chars[i + n..].iter().filter(|c| c.is_alphabetic()).take(1).collect();
                let minutes = n <= 2 && (last_was_hour || rest.eq_ignore_ascii_case("s"));
                Some(match (minutes, n) {
                    (true, 1) => "%-M",
                    (true, _) => "%M",
                    (false, 1) => "%-m",
                    (false, 2) => "%m",
                    (false, 3) => "%b",
                    (false, _) => "%B",
                })
            }
            _ => None,
        };
        match code {
            Some(code) => {
                out.push_str(code);
                last_was_hour = lc == 'h';
                i += n;
            }
            None => {
                if c == '%' {
                    out.push_str("%%");
                } else if c != '\\' {
                    out.push(c);
                }
                i += 1;
            }
        }
    }
    out
}

/// Excel's number codes (`0.00`, `#,##0`, `0%`, `$#,##0.00`, `0.0 "kg"`).
fn number_format(f: &str) -> Result<String, String> {
    let first = f.find(['0', '#']).unwrap_or(0);
    let last = f.rfind(['0', '#']).map(|i| i + 1).unwrap_or(f.len());
    let (prefix, digits, suffix) = (&f[..first], &f[first..last], &f[last..]);
    let percent = suffix.contains('%') || prefix.contains('%');
    let comma = digits.contains(',');
    let decimals = digits.split_once('.').map(|(_, d)| d.chars().filter(|c| *c == '0' || *c == '#').count()).unwrap_or(0);
    let clean = |s: &str| s.replace(['"', '\\'], "");
    let spec = format!("{{:{}.{decimals}f}}", if comma { "," } else { "" });
    let value = if percent { "CAST(({0}) AS DOUBLE) * 100" } else { "CAST(({0}) AS DOUBLE)" };
    Ok(format!("{} || format({}, {value}) || {}", sql_text(&clean(prefix)), sql_text(&spec), sql_text(&clean(suffix))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::LiteralValue as L;

    #[test]
    fn catalog_points_at_real_functions() {
        let reg = FunctionRegistry::default();
        let all = catalog();
        assert!(all.len() >= 100, "{}", all.len());
        let mut names: Vec<_> = all.iter().map(|e| e.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), all.len(), "each Excel name once");
        for e in &all {
            match (e.function, e.instead) {
                (Some(f), None) => assert!(reg.lookup(f).is_some(), "{} → {f} is not a function", e.name),
                (None, Some(_)) => {}
                _ => panic!("{}: a function or an alternative", e.name),
            }
        }
    }

    #[test]
    fn criteria() {
        let c = |s: &str| criterion(0, &L::Str(s.into()));
        assert_eq!(c("North"), "lower(CAST(({0}) AS VARCHAR)) = 'north'");
        assert_eq!(c(">10"), "TRY_CAST(({0}) AS DOUBLE) > 10");
        assert_eq!(c("<>5"), "(TRY_CAST(({0}) AS DOUBLE) <> 5 OR ({0}) IS NULL)");
        assert_eq!(c(">=2024-01-01"), "TRY_CAST(({0}) AS DATE) >= DATE '2024-01-01'");
        assert_eq!(c("a*"), "lower(CAST(({0}) AS VARCHAR)) LIKE 'a%' ESCAPE '\\'");
        assert_eq!(c("50%?"), "lower(CAST(({0}) AS VARCHAR)) LIKE '50\\%_' ESCAPE '\\'");
        assert_eq!(c(""), "(({0}) IS NULL OR CAST(({0}) AS VARCHAR) = '')");
        assert_eq!(c("<>"), "NOT (({0}) IS NULL OR CAST(({0}) AS VARCHAR) = '')");
        assert_eq!(c("O'Brien"), "lower(CAST(({0}) AS VARCHAR)) = 'o''brien'");
        assert_eq!(criterion(2, &L::Int("7".into())), "TRY_CAST(({2}) AS DOUBLE) = 7");
    }

    #[test]
    fn date_codes() {
        assert_eq!(date_pattern("dd.mm.yyyy"), "%d.%m.%Y");
        assert_eq!(date_pattern("yyyy-mm-dd hh:mm:ss"), "%Y-%m-%d %H:%M:%S");
        assert_eq!(date_pattern("mmm yyyy"), "%b %Y");
        assert_eq!(date_pattern("mmmm d, yyyy"), "%B %-d, %Y");
        assert_eq!(date_pattern("dddd"), "%A");
        assert_eq!(date_pattern("h:mm AM/PM"), "%-I:%M %p");
        assert_eq!(date_pattern("d \"of\" mmmm"), "%-d of %B");
    }

    #[test]
    fn number_codes() {
        assert_eq!(text_format("0.00").unwrap(), "'' || format('{:.2f}', CAST(({0}) AS DOUBLE)) || ''");
        assert_eq!(text_format("#,##0").unwrap(), "'' || format('{:,.0f}', CAST(({0}) AS DOUBLE)) || ''");
        assert_eq!(text_format("0.0%").unwrap(), "'' || format('{:.1f}', CAST(({0}) AS DOUBLE) * 100) || '%'");
        assert_eq!(text_format("$#,##0.00").unwrap(), "'$' || format('{:,.2f}', CAST(({0}) AS DOUBLE)) || ''");
        assert!(text_format("dd.mm.yyyy").unwrap().starts_with("strftime("));
        assert!(text_format("@").unwrap().starts_with("CAST("));
        assert!(text_format("zzz").is_err());
    }
}
