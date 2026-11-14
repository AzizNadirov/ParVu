//! Function registry: single source of truth for all DSL functions.
//!
//! The expression editor reads it for autocomplete and the doc popup.

use serde::Serialize;

use crate::types::{LogicalType, methods_for_type};

/// Parameter definition for a registry function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ParamDef {
    pub name: String,
    pub logical_type: LogicalType,
    pub required: bool,
    pub description: String,
}

impl ParamDef {
    pub fn new(name: &str, logical_type: LogicalType) -> Self {
        Self {
            name: name.to_string(),
            logical_type,
            required: true,
            description: String::new(),
        }
    }

    pub fn optional(name: &str, logical_type: LogicalType) -> Self {
        Self {
            required: false,
            ..Self::new(name, logical_type)
        }
    }
}

/// Definition of a function in the ParVu DSL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FunctionDef {
    /// Canonical name, e.g. `SUM`, `DATEDIFF`.
    pub name: String,
    pub params: Vec<ParamDef>,
    pub return_type: LogicalType,
    pub description: String,
    pub example: String,
    /// SQL function name emitted by the compiler (e.g. `DATE_DIFF`).
    pub sql_name: Option<String>,
    /// Reserved for special rendering rules; unused by the defaults.
    pub sql_template: Option<String>,
}

impl FunctionDef {
    pub fn new(
        name: &str,
        params: Vec<ParamDef>,
        return_type: LogicalType,
        description: &str,
        example: &str,
        sql_name: Option<&str>,
    ) -> Self {
        Self {
            name: name.to_string(),
            params,
            return_type,
            description: description.to_string(),
            example: example.to_string(),
            sql_name: sql_name.map(str::to_string),
            sql_template: None,
        }
    }
}

/// Central registry of all DSL functions. `Default` holds the built-in set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FunctionRegistry {
    /// Keyed by upper-case name, in registration order.
    functions: Vec<(String, FunctionDef)>,
    /// method_name (lower) -> function name (upper), in registration order.
    methods: Vec<(String, String)>,
}

impl Default for FunctionRegistry {
    fn default() -> Self {
        let mut r = Self::empty();
        r.register_defaults();
        r
    }
}

impl FunctionRegistry {
    /// Registry with the built-in function set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registry with no functions at all.
    pub fn empty() -> Self {
        Self {
            functions: Vec::new(),
            methods: Vec::new(),
        }
    }

    /// Add (or replace) a function.
    pub fn register(&mut self, func: FunctionDef) {
        let key = func.name.to_uppercase();
        match self.functions.iter_mut().find(|(k, _)| *k == key) {
            Some(entry) => entry.1 = func,
            None => self.functions.push((key, func)),
        }
    }

    /// Map a dot-method to a registry function.
    pub fn register_method(&mut self, method_name: &str, function_name: &str) {
        let key = method_name.to_lowercase();
        let target = function_name.to_uppercase();
        match self.methods.iter_mut().find(|(k, _)| *k == key) {
            Some(entry) => entry.1 = target,
            None => self.methods.push((key, target)),
        }
    }

    /// Function by canonical name (case-insensitive).
    pub fn lookup(&self, name: &str) -> Option<&FunctionDef> {
        let key = name.to_uppercase();
        self.functions
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, f)| f)
    }

    /// Function mapped from a dot-method name (case-insensitive). Returns
    /// `None` when the method is registered but its target function is not.
    pub fn lookup_method(&self, method_name: &str) -> Option<&FunctionDef> {
        let key = method_name.to_lowercase();
        let (_, target) = self.methods.iter().find(|(k, _)| *k == key)?;
        self.lookup(target)
    }

    pub fn by_return_type(&self, t: LogicalType) -> Vec<&FunctionDef> {
        self.functions
            .iter()
            .map(|(_, f)| f)
            .filter(|f| f.return_type == t)
            .collect()
    }

    /// Functions whose `position`-th parameter accepts `t` (or any parameter
    /// does, when the function has fewer parameters).
    pub fn by_param_type(&self, t: LogicalType, position: usize) -> Vec<&FunctionDef> {
        let accepts = |p: &ParamDef| p.logical_type == t || p.logical_type == LogicalType::Unknown;
        self.functions
            .iter()
            .map(|(_, f)| f)
            .filter(|f| match f.params.get(position) {
                Some(p) => accepts(p),
                None => f.params.iter().any(accepts),
            })
            .collect()
    }

    pub fn all_functions(&self) -> Vec<&FunctionDef> {
        self.functions.iter().map(|(_, f)| f).collect()
    }

    /// All registered method names (lower-case).
    pub fn all_methods(&self) -> Vec<String> {
        self.methods.iter().map(|(k, _)| k.clone()).collect()
    }

    /// Method names available for a logical type.
    pub fn methods_for_type(&self, t: LogicalType) -> Vec<String> {
        methods_for_type(t)
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    fn register_defaults(&mut self) {
        use LogicalType as LT;
        let p = ParamDef::new;
        let opt = ParamDef::optional;
        let mut add = |name: &str,
                       params: Vec<ParamDef>,
                       ret: LT,
                       desc: &str,
                       example: &str,
                       sql: Option<&str>| {
            self.register(FunctionDef::new(name, params, ret, desc, example, sql));
        };

        // Aggregates
        add(
            "SUM",
            vec![p("value", LT::Numeric)],
            LT::Numeric,
            "Sum of all values.",
            "SUM(sales[revenue])",
            Some("SUM"),
        );
        add(
            "AVG",
            vec![p("value", LT::Numeric)],
            LT::Numeric,
            "Average of all values.",
            "AVG(sales[revenue])",
            Some("AVG"),
        );
        add(
            "COUNT",
            vec![opt("value", LT::Unknown)],
            LT::Numeric,
            "Count of rows.",
            "COUNT(sales[id])",
            Some("COUNT"),
        );
        add(
            "COUNTD",
            vec![p("value", LT::Unknown)],
            LT::Numeric,
            "Count of distinct values.",
            "COUNTD(sales[id])",
            Some("COUNT_DISTINCT"),
        );
        add(
            "MIN",
            vec![p("value", LT::Unknown)],
            LT::Unknown,
            "Minimum value.",
            "MIN(sales[revenue])",
            Some("MIN"),
        );
        add(
            "MAX",
            vec![p("value", LT::Unknown)],
            LT::Unknown,
            "Maximum value.",
            "MAX(sales[revenue])",
            Some("MAX"),
        );

        // Conditionals
        add(
            "IF",
            vec![
                p("condition", LT::Boolean),
                p("true_value", LT::Unknown),
                p("false_value", LT::Unknown),
            ],
            LT::Unknown,
            "Return true_value if condition is true, else false_value.",
            "IF(sales[revenue] > 100, 'high', 'low')",
            Some("IF"),
        );
        add(
            "COALESCE",
            vec![p("values", LT::Unknown)],
            LT::Unknown,
            "Return first non-null value.",
            "COALESCE(sales[note], 'N/A')",
            Some("COALESCE"),
        );

        // Text
        add(
            "LEN",
            vec![p("text", LT::Text)],
            LT::Numeric,
            "Length of text.",
            "LEN(products[name])",
            Some("LENGTH"),
        );
        add(
            "UPPER",
            vec![p("text", LT::Text)],
            LT::Text,
            "Convert text to uppercase.",
            "UPPER(products[name])",
            Some("UPPER"),
        );
        add(
            "LOWER",
            vec![p("text", LT::Text)],
            LT::Text,
            "Convert text to lowercase.",
            "LOWER(products[name])",
            Some("LOWER"),
        );
        add(
            "TRIM",
            vec![p("text", LT::Text)],
            LT::Text,
            "Remove leading and trailing whitespace.",
            "TRIM(products[name])",
            Some("TRIM"),
        );
        add(
            "CONCAT",
            vec![p("values", LT::Text)],
            LT::Text,
            "Concatenate strings.",
            "CONCAT(products[name], ' - ', products[category])",
            Some("CONCAT"),
        );
        add(
            "CONTAINS",
            vec![p("text", LT::Text), p("substring", LT::Text)],
            LT::Boolean,
            "Check if text contains substring.",
            "CONTAINS(products[name], 'apple')",
            Some("CONTAINS"),
        );
        add(
            "STARTSWITH",
            vec![p("text", LT::Text), p("prefix", LT::Text)],
            LT::Boolean,
            "Check if text starts with prefix.",
            "STARTSWITH(products[name], 'A')",
            Some("STARTS_WITH"),
        );
        add(
            "ENDSWITH",
            vec![p("text", LT::Text), p("suffix", LT::Text)],
            LT::Boolean,
            "Check if text ends with suffix.",
            "ENDSWITH(products[name], 'Z')",
            Some("ENDS_WITH"),
        );

        // Numeric
        add(
            "ABS",
            vec![p("value", LT::Numeric)],
            LT::Numeric,
            "Absolute value.",
            "ABS(sales[discount])",
            Some("ABS"),
        );
        add(
            "ROUND",
            vec![p("value", LT::Numeric), opt("decimals", LT::Numeric)],
            LT::Numeric,
            "Round to given decimal places.",
            "ROUND(sales[revenue], 2)",
            Some("ROUND"),
        );
        add(
            "FLOOR",
            vec![p("value", LT::Numeric)],
            LT::Numeric,
            "Round down to nearest integer.",
            "FLOOR(sales[revenue])",
            Some("FLOOR"),
        );
        add(
            "CEIL",
            vec![p("value", LT::Numeric)],
            LT::Numeric,
            "Round up to nearest integer.",
            "CEIL(sales[revenue])",
            Some("CEIL"),
        );
        add(
            "POWER",
            vec![p("base", LT::Numeric), p("exponent", LT::Numeric)],
            LT::Numeric,
            "Raise base to exponent power.",
            "POWER(2, 10)",
            Some("POWER"),
        );
        add(
            "SQRT",
            vec![p("value", LT::Numeric)],
            LT::Numeric,
            "Square root.",
            "SQRT(sales[revenue])",
            Some("SQRT"),
        );

        // Date
        add(
            "DATEDIFF",
            vec![
                p("unit", LT::Text),
                p("start", LT::Date),
                p("end", LT::Date),
            ],
            LT::Numeric,
            "Difference between two dates in given unit.",
            "DATEDIFF('day', orders[start], orders[end])",
            Some("DATE_DIFF"),
        );
        add(
            "YEAR",
            vec![p("date", LT::Date)],
            LT::Numeric,
            "Extract year from date.",
            "YEAR(orders[date])",
            Some("YEAR"),
        );
        add(
            "MONTH",
            vec![p("date", LT::Date)],
            LT::Numeric,
            "Extract month from date.",
            "MONTH(orders[date])",
            Some("MONTH"),
        );
        add(
            "DAY",
            vec![p("date", LT::Date)],
            LT::Numeric,
            "Extract day from date.",
            "DAY(orders[date])",
            Some("DAY"),
        );
        add(
            "TODAY",
            vec![],
            LT::Date,
            "Current date.",
            "TODAY()",
            Some("CURRENT_DATE"),
        );
        add(
            "NOW",
            vec![],
            LT::Timestamp,
            "Current timestamp.",
            "NOW()",
            Some("CURRENT_TIMESTAMP"),
        );

        // Type casts
        add(
            "CAST",
            vec![p("value", LT::Unknown), p("type", LT::Text)],
            LT::Unknown,
            "Cast value to a different type.",
            "CAST(sales[id], 'INTEGER')",
            Some("CAST"),
        );
        add(
            "TRY_CAST",
            vec![p("value", LT::Unknown), p("type", LT::Text)],
            LT::Unknown,
            "Cast value, returning NULL on failure.",
            "TRY_CAST(sales[id], 'INTEGER')",
            Some("TRY_CAST"),
        );
        add(
            "REPLACE",
            vec![
                p("text", LT::Text),
                p("pattern", LT::Text),
                p("with_value", LT::Text),
                opt("case_sensitive", LT::Boolean),
                opt("regex", LT::Boolean),
            ],
            LT::Text,
            "Replace occurrences of pattern in text with another value.",
            "REPLACE(products[name], 'old', 'new')",
            Some("REPLACE"),
        );

        // More conditionals
        add(
            "IFNULL",
            vec![p("value", LT::Unknown), p("fallback", LT::Unknown)],
            LT::Unknown,
            "The value, or the fallback where the value is NULL (pandas fillna).",
            "IFNULL(sales[discount], 0)",
            Some("IFNULL"),
        );
        add(
            "GREATEST",
            vec![p("a", LT::Unknown), p("b", LT::Unknown)],
            LT::Unknown,
            "The largest of the arguments (any number of them).",
            "GREATEST(sales[price], 0)",
            Some("GREATEST"),
        );
        add(
            "LEAST",
            vec![p("a", LT::Unknown), p("b", LT::Unknown)],
            LT::Unknown,
            "The smallest of the arguments (any number of them).",
            "LEAST(sales[qty], 100)",
            Some("LEAST"),
        );

        // More text functions
        add(
            "SPLIT_PART",
            vec![p("text", LT::Text), p("separator", LT::Text), p("index", LT::Numeric)],
            LT::Text,
            "The index-th piece (from 1) of text split by the separator.",
            "SPLIT_PART(people[email], '@', 2)",
            Some("SPLIT_PART"),
        );
        add(
            "SUBSTR",
            vec![p("text", LT::Text), p("start", LT::Numeric), opt("length", LT::Numeric)],
            LT::Text,
            "Part of the text from position start (from 1), optionally of a given length.",
            "SUBSTR(sales[code], 1, 3)",
            Some("SUBSTRING"),
        );
        add(
            "LEFT",
            vec![p("text", LT::Text), p("count", LT::Numeric)],
            LT::Text,
            "The first count characters.",
            "LEFT(sales[code], 2)",
            Some("LEFT"),
        );
        add(
            "RIGHT",
            vec![p("text", LT::Text), p("count", LT::Numeric)],
            LT::Text,
            "The last count characters.",
            "RIGHT(sales[code], 4)",
            Some("RIGHT"),
        );
        add(
            "LPAD",
            vec![p("text", LT::Text), p("length", LT::Numeric), p("fill", LT::Text)],
            LT::Text,
            "Pad text on the left with fill up to length characters.",
            "LPAD(CAST(sales[id], 'VARCHAR'), 6, '0')",
            Some("LPAD"),
        );
        add(
            "RPAD",
            vec![p("text", LT::Text), p("length", LT::Numeric), p("fill", LT::Text)],
            LT::Text,
            "Pad text on the right with fill up to length characters.",
            "RPAD(sales[code], 8, '_')",
            Some("RPAD"),
        );
        add(
            "REVERSE",
            vec![p("text", LT::Text)],
            LT::Text,
            "The text backwards.",
            "REVERSE(sales[code])",
            Some("REVERSE"),
        );
        add(
            "REGEX_EXTRACT",
            vec![p("text", LT::Text), p("pattern", LT::Text), opt("group", LT::Numeric)],
            LT::Text,
            "The part of the text matching a regular expression, or one of its groups (pandas str.extract).",
            "REGEX_EXTRACT(people[phone], '(\\d{3})', 1)",
            Some("REGEXP_EXTRACT"),
        );
        add(
            "REGEX_MATCH",
            vec![p("text", LT::Text), p("pattern", LT::Text)],
            LT::Boolean,
            "True where the text contains a match of the regular expression.",
            "REGEX_MATCH(people[email], '@example\\.com$')",
            Some("REGEXP_MATCHES"),
        );

        // More date functions
        add(
            "STRFTIME",
            vec![p("date", LT::Unknown), p("format", LT::Text)],
            LT::Text,
            "Format a date or timestamp as text, e.g. '%Y-%m' or '%d.%m.%Y'.",
            "STRFTIME(sales[day], '%Y-%m')",
            Some("STRFTIME"),
        );
        add(
            "STRPTIME",
            vec![p("text", LT::Text), p("format", LT::Text)],
            LT::Timestamp,
            "Parse text into a timestamp with a format such as '%d.%m.%Y'.",
            "STRPTIME(sales[day_text], '%d.%m.%Y')",
            Some("STRPTIME"),
        );
        add(
            "QUARTER",
            vec![p("date", LT::Date)],
            LT::Numeric,
            "Quarter of the year (1-4).",
            "QUARTER(sales[day])",
            Some("QUARTER"),
        );
        add(
            "WEEK",
            vec![p("date", LT::Date)],
            LT::Numeric,
            "ISO week of the year (1-53).",
            "WEEK(sales[day])",
            Some("WEEK"),
        );
        add(
            "DAYOFWEEK",
            vec![p("date", LT::Date)],
            LT::Numeric,
            "Day of the week, 0 = Sunday to 6 = Saturday.",
            "DAYOFWEEK(sales[day])",
            Some("DAYOFWEEK"),
        );
        add(
            "HOUR",
            vec![p("timestamp", LT::Timestamp)],
            LT::Numeric,
            "Hour (0-23).",
            "HOUR(events[time])",
            Some("HOUR"),
        );
        add(
            "MINUTE",
            vec![p("timestamp", LT::Timestamp)],
            LT::Numeric,
            "Minute (0-59).",
            "MINUTE(events[time])",
            Some("MINUTE"),
        );
        add(
            "DATE_TRUNC",
            vec![p("part", LT::Text), p("date", LT::Unknown)],
            LT::Date,
            "Round a date down to the start of its 'year', 'quarter', 'month', 'week', 'day' or 'hour' (pandas dt.floor).",
            "DATE_TRUNC('month', sales[day])",
            Some("DATE_TRUNC"),
        );

        // More math
        add("LN", vec![p("value", LT::Numeric)], LT::Numeric, "Natural logarithm.", "LN(sales[price])", Some("LN"));
        add("LOG10", vec![p("value", LT::Numeric)], LT::Numeric, "Base-10 logarithm.", "LOG10(sales[price])", Some("LOG10"));
        add("EXP", vec![p("value", LT::Numeric)], LT::Numeric, "e raised to the value.", "EXP(sales[rate])", Some("EXP"));
        add(
            "MOD",
            vec![p("value", LT::Numeric), p("divisor", LT::Numeric)],
            LT::Numeric,
            "Remainder of a division.",
            "MOD(sales[id], 10)",
            Some("MOD"),
        );
        add(
            "SIGN",
            vec![p("value", LT::Numeric)],
            LT::Numeric,
            "-1, 0 or 1 by the sign of the value.",
            "SIGN(sales[change])",
            Some("SIGN"),
        );

        // Table-level operations
        add(
            "DROP_DUPLICATES",
            vec![
                p("table", LT::Unknown),
                opt("columns", LT::Unknown),
                opt("keep", LT::Text),
            ],
            LT::Query,
            "Remove duplicate rows based on column subset.",
            "DROP_DUPLICATES(sales[id], sales[name], 'first')",
            None,
        );
        add(
            "DROP_NULL",
            vec![p("column", LT::Unknown), opt("null_value", LT::Unknown)],
            LT::Query,
            "Drop rows where a column equals the null sentinel. \
             With no sentinel, drops SQL NULLs. With a sentinel value, \
             drops only rows whose column equals that value (real NULLs are kept).",
            "DROP_NULL(sales[note])  or  DROP_NULL(sales[score], -1)",
            None,
        );

        // Method mappings
        for lt in LogicalType::ALL {
            for method in methods_for_type(lt) {
                self.register_method(method, &method.to_uppercase());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let r = FunctionRegistry::default();
        assert_eq!(r.all_functions().len(), 33 + 25);
        assert_eq!(r.lookup("sum").unwrap().name, "SUM");
        assert_eq!(
            r.lookup("countd").unwrap().sql_name.as_deref(),
            Some("COUNT_DISTINCT")
        );
        assert_eq!(r.lookup_method("LEN").unwrap().name, "LEN");
        assert!(r.lookup_method("if_true").is_none());
        assert!(r.lookup_method("add_days").is_none());
        assert!(r.all_methods().contains(&"add_days".to_string()));
        assert_eq!(r.all_methods()[0], "abs");
        assert!(r.lookup("nope").is_none());
    }

    #[test]
    fn by_type_queries() {
        let r = FunctionRegistry::default();
        assert!(
            r.by_return_type(LogicalType::Boolean)
                .iter()
                .all(|f| f.return_type == LogicalType::Boolean)
        );
        assert!(
            r.by_param_type(LogicalType::Date, 1)
                .iter()
                .any(|f| f.name == "DATEDIFF")
        );
    }
}
