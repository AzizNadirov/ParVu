//! A port of the subset of Python `str.format` used by the locale strings:
//! `{name}`, `{name:,}` (thousands separators), `{name:.1f}` and the rest of
//! the standard format-spec mini-language for strings, integers and floats
//! (fill/align, sign, `#`, `0`, width, `,`/`_` grouping, precision and the
//! types `s d n x X o b f F e E %`). `{{` and `}}` are literal braces.
//!
//! Unsupported features (positional fields, attribute/index access, nested
//! specs, the `g`/`G` types) return an error; the translator then returns
//! the unformatted text, as Python does on `KeyError`/`ValueError`.

use std::fmt::Write as _;

/// A value substituted into a translation.
#[derive(Debug, Clone, PartialEq)]
pub enum FmtArg {
    Str(String),
    Int(i128),
    Float(f64),
    Bool(bool),
}

macro_rules! from_int {
    ($($t:ty),*) => {$(
        impl From<$t> for FmtArg {
            fn from(v: $t) -> Self { FmtArg::Int(v as i128) }
        }
    )*};
}
from_int!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, usize);

impl From<f64> for FmtArg {
    fn from(v: f64) -> Self {
        FmtArg::Float(v)
    }
}

impl From<f32> for FmtArg {
    fn from(v: f32) -> Self {
        FmtArg::Float(f64::from(v))
    }
}

impl From<bool> for FmtArg {
    fn from(v: bool) -> Self {
        FmtArg::Bool(v)
    }
}

impl From<&str> for FmtArg {
    fn from(v: &str) -> Self {
        FmtArg::Str(v.to_owned())
    }
}

impl From<String> for FmtArg {
    fn from(v: String) -> Self {
        FmtArg::Str(v)
    }
}

impl From<&String> for FmtArg {
    fn from(v: &String) -> Self {
        FmtArg::Str(v.clone())
    }
}

impl From<&std::path::Path> for FmtArg {
    fn from(v: &std::path::Path) -> Self {
        FmtArg::Str(v.display().to_string())
    }
}

/// Why formatting failed (mirrors Python's `KeyError` / `ValueError`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatError {
    MissingKey(String),
    Invalid(String),
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingKey(k) => write!(f, "missing key {k:?}"),
            Self::Invalid(m) => f.write_str(m),
        }
    }
}

fn invalid<T>(msg: impl Into<String>) -> Result<T, FormatError> {
    Err(FormatError::Invalid(msg.into()))
}

/// One `{...}` replacement field found in a template.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Field {
    pub name: String,
    pub conversion: Option<char>,
    pub spec: String,
}

enum Piece<'a> {
    Lit(String),
    Field(&'a str),
}

fn split(template: &str) -> Result<Vec<Piece<'_>>, FormatError> {
    let mut out = Vec::new();
    let mut lit = String::new();
    let bytes = template.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' if bytes.get(i + 1) == Some(&b'{') => {
                lit.push('{');
                i += 2;
            }
            b'}' if bytes.get(i + 1) == Some(&b'}') => {
                lit.push('}');
                i += 2;
            }
            b'{' => {
                let rest = &template[i + 1..];
                let Some(end) = rest.find('}') else {
                    return invalid("expected '}' before end of string");
                };
                let inner = &rest[..end];
                if inner.contains('{') {
                    return invalid("nested replacement fields are not supported");
                }
                if !lit.is_empty() {
                    out.push(Piece::Lit(std::mem::take(&mut lit)));
                }
                out.push(Piece::Field(inner));
                i += end + 2;
            }
            b'}' => return invalid("single '}' encountered in format string"),
            _ => {
                // Copy one UTF-8 character.
                let ch = template[i..].chars().next().expect("in bounds");
                lit.push(ch);
                i += ch.len_utf8();
            }
        }
    }
    if !lit.is_empty() {
        out.push(Piece::Lit(lit));
    }
    Ok(out)
}

fn parse_field(inner: &str) -> Result<Field, FormatError> {
    let (head, spec) = match inner.find(':') {
        Some(p) => (&inner[..p], &inner[p + 1..]),
        None => (inner, ""),
    };
    let (name, conversion) = match head.find('!') {
        Some(p) => {
            let conv = &head[p + 1..];
            let mut cs = conv.chars();
            match (cs.next(), cs.next()) {
                (Some(c @ ('s' | 'r' | 'a')), None) => (&head[..p], Some(c)),
                _ => return invalid(format!("invalid conversion {conv:?}")),
            }
        }
        None => (head, None),
    };
    Ok(Field { name: name.to_owned(), conversion, spec: spec.to_owned() })
}

/// All replacement fields in `template`, in order. Used to compare
/// placeholders across locales.
pub fn fields(template: &str) -> Result<Vec<Field>, FormatError> {
    split(template)?
        .into_iter()
        .filter_map(|p| match p {
            Piece::Field(f) => Some(parse_field(f)),
            Piece::Lit(_) => None,
        })
        .collect()
}

/// Format `template` with named arguments, like Python
/// `template.format(**args)`.
pub fn format(template: &str, args: &[(&str, FmtArg)]) -> Result<String, FormatError> {
    let mut out = String::with_capacity(template.len() + 16);
    for piece in split(template)? {
        match piece {
            Piece::Lit(s) => out.push_str(&s),
            Piece::Field(inner) => {
                let field = parse_field(inner)?;
                if field.name.is_empty()
                    || field.name.chars().all(|c| c.is_ascii_digit())
                    || field.name.contains(['.', '['])
                {
                    return invalid(format!("unsupported field name {:?}", field.name));
                }
                let arg = args
                    .iter()
                    .find(|(k, _)| *k == field.name)
                    .map(|(_, v)| v)
                    .ok_or_else(|| FormatError::MissingKey(field.name.clone()))?;
                let converted;
                let arg = match field.conversion {
                    None => arg,
                    Some(c) => {
                        converted = FmtArg::Str(match (c, arg) {
                            ('r' | 'a', FmtArg::Str(s)) => py_repr_str(s),
                            _ => to_py_str(arg),
                        });
                        &converted
                    }
                };
                format_value(&mut out, arg, &field.spec)?;
            }
        }
    }
    Ok(out)
}

fn py_repr_str(s: &str) -> String {
    if s.contains('\'') && !s.contains('"') {
        format!("\"{s}\"")
    } else {
        format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
    }
}

/// Python `str(value)`.
fn to_py_str(arg: &FmtArg) -> String {
    match arg {
        FmtArg::Str(s) => s.clone(),
        FmtArg::Int(i) => i.to_string(),
        FmtArg::Float(f) => float_repr(*f),
        FmtArg::Bool(b) => if *b { "True" } else { "False" }.to_owned(),
    }
}

/// Python `repr(float)`: shortest round-trip digits, exponent outside
/// `1e-4 <= |x| < 1e16`.
fn float_repr(f: f64) -> String {
    if f.is_nan() {
        return "nan".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "inf".into() } else { "-inf".into() };
    }
    let a = f.abs();
    if a != 0.0 && !(1e-4..1e16).contains(&a) {
        let s = format!("{f:e}");
        let (mant, exp) = s.split_once('e').expect("exponent form");
        let exp: i32 = exp.parse().expect("integer exponent");
        let sign = if exp < 0 { '-' } else { '+' };
        return format!("{mant}e{sign}{:02}", exp.abs());
    }
    let s = f.to_string();
    if s.contains('.') { s } else { format!("{s}.0") }
}

#[derive(Debug, Default)]
struct Spec {
    fill: Option<char>,
    align: Option<char>,
    sign: Option<char>,
    alt: bool,
    zero: bool,
    width: usize,
    grouping: Option<char>,
    precision: Option<usize>,
    ty: Option<char>,
}

fn parse_spec(spec: &str) -> Result<Spec, FormatError> {
    let chars: Vec<char> = spec.chars().collect();
    let mut s = Spec::default();
    let mut i = 0;
    let is_align = |c: char| matches!(c, '<' | '>' | '=' | '^');
    if chars.len() >= 2 && is_align(chars[1]) {
        s.fill = Some(chars[0]);
        s.align = Some(chars[1]);
        i = 2;
    } else if chars.first().is_some_and(|&c| is_align(c)) {
        s.align = Some(chars[0]);
        i = 1;
    }
    if let Some(&c @ ('+' | '-' | ' ')) = chars.get(i) {
        s.sign = Some(c);
        i += 1;
    }
    if chars.get(i) == Some(&'z') {
        i += 1;
    }
    if chars.get(i) == Some(&'#') {
        s.alt = true;
        i += 1;
    }
    if chars.get(i) == Some(&'0') {
        s.zero = true;
        i += 1;
    }
    let start = i;
    while chars.get(i).is_some_and(char::is_ascii_digit) {
        i += 1;
    }
    if i > start {
        let w: String = chars[start..i].iter().collect();
        s.width = w.parse().map_err(|_| FormatError::Invalid("width too large".into()))?;
    }
    if let Some(&c @ (',' | '_')) = chars.get(i) {
        s.grouping = Some(c);
        i += 1;
    }
    if chars.get(i) == Some(&'.') {
        i += 1;
        let start = i;
        while chars.get(i).is_some_and(char::is_ascii_digit) {
            i += 1;
        }
        if i == start {
            return invalid("format specifier missing precision");
        }
        let p: String = chars[start..i].iter().collect();
        s.precision = Some(p.parse().map_err(|_| FormatError::Invalid("precision too large".into()))?);
    }
    if let Some(&c) = chars.get(i) {
        s.ty = Some(c);
        i += 1;
    }
    if i != chars.len() {
        return invalid(format!("invalid format specifier {spec:?}"));
    }
    if s.zero && s.align.is_none() {
        s.fill.get_or_insert('0');
        s.align = Some('=');
    }
    Ok(s)
}

fn group_digits(digits: &str, sep: char, every: usize) -> String {
    let n = digits.chars().count();
    let mut out = String::with_capacity(n + n / every);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (n - i).is_multiple_of(every) {
            out.push(sep);
        }
        out.push(c);
    }
    out
}

fn pad(out: &mut String, prefix: &str, body: &str, spec: &Spec, default_align: char) {
    let fill = spec.fill.unwrap_or(' ');
    let align = spec.align.unwrap_or(default_align);
    let len = prefix.chars().count() + body.chars().count();
    let padding = spec.width.saturating_sub(len);
    let fills = |n: usize| std::iter::repeat_n(fill, n).collect::<String>();
    match align {
        '<' => {
            let _ = write!(out, "{prefix}{body}{}", fills(padding));
        }
        '^' => {
            let left = padding / 2;
            let _ = write!(out, "{}{prefix}{body}{}", fills(left), fills(padding - left));
        }
        '=' => {
            let _ = write!(out, "{prefix}{}{body}", fills(padding));
        }
        _ => {
            let _ = write!(out, "{}{prefix}{body}", fills(padding));
        }
    }
}

fn sign_prefix(negative: bool, spec: &Spec) -> &'static str {
    match (negative, spec.sign) {
        (true, _) => "-",
        (false, Some('+')) => "+",
        (false, Some(' ')) => " ",
        _ => "",
    }
}

fn format_value(out: &mut String, arg: &FmtArg, spec_str: &str) -> Result<(), FormatError> {
    if spec_str.is_empty() {
        out.push_str(&to_py_str(arg));
        return Ok(());
    }
    let spec = parse_spec(spec_str)?;
    match arg {
        FmtArg::Str(s) => format_str(out, s, &spec),
        FmtArg::Bool(b) if matches!(spec.ty, None | Some('s')) && spec.grouping.is_none() => {
            format_str(out, if *b { "True" } else { "False" }, &spec)
        }
        FmtArg::Bool(b) => format_int(out, i128::from(*b), &spec),
        FmtArg::Int(i) => format_int(out, *i, &spec),
        FmtArg::Float(f) => format_float(out, *f, &spec),
    }
}

fn format_str(out: &mut String, s: &str, spec: &Spec) -> Result<(), FormatError> {
    if !matches!(spec.ty, None | Some('s')) {
        return invalid(format!("Unknown format code '{}' for object of type 'str'", spec.ty.unwrap_or(' ')));
    }
    if spec.sign.is_some() {
        return invalid("Sign not allowed in string format specifier");
    }
    if spec.alt {
        return invalid("Alternate form (#) not allowed in string format specifier");
    }
    if spec.grouping.is_some() {
        return invalid("Cannot specify ',' with 's'.");
    }
    if spec.align == Some('=') {
        return invalid("'=' alignment not allowed in string format specifier");
    }
    let body: String = match spec.precision {
        Some(p) => s.chars().take(p).collect(),
        None => s.to_owned(),
    };
    pad(out, "", &body, spec, '<');
    Ok(())
}

fn format_int(out: &mut String, v: i128, spec: &Spec) -> Result<(), FormatError> {
    let (radix_prefix, digits) = match spec.ty {
        None | Some('d') | Some('n') => ("", v.unsigned_abs().to_string()),
        Some('x') => ("0x", format!("{:x}", v.unsigned_abs())),
        Some('X') => ("0X", format!("{:X}", v.unsigned_abs())),
        Some('o') => ("0o", format!("{:o}", v.unsigned_abs())),
        Some('b') => ("0b", format!("{:b}", v.unsigned_abs())),
        Some('f' | 'F' | 'e' | 'E' | '%' | 'g' | 'G') => return format_float(out, v as f64, spec),
        Some(c) => return invalid(format!("Unknown format code '{c}' for object of type 'int'")),
    };
    if spec.precision.is_some() {
        return invalid("Precision not allowed in integer format specifier");
    }
    let digits = match (spec.grouping, spec.ty) {
        (None, _) => digits,
        (Some(sep), None | Some('d')) => group_digits(&digits, sep, 3),
        (Some('_'), Some('x' | 'X' | 'o' | 'b')) => group_digits(&digits, '_', 4),
        (Some(sep), Some(c)) => return invalid(format!("Cannot specify '{sep}' with '{c}'.")),
    };
    let mut prefix = sign_prefix(v < 0, spec).to_owned();
    if spec.alt {
        prefix.push_str(radix_prefix);
    }
    pad(out, &prefix, &digits, spec, '>');
    Ok(())
}

fn group_number(num: &str, sep: Option<char>) -> String {
    let Some(sep) = sep else { return num.to_owned() };
    let (int_part, rest) = match num.find(|c: char| !c.is_ascii_digit()) {
        Some(p) => num.split_at(p),
        None => (num, ""),
    };
    format!("{}{rest}", group_digits(int_part, sep, 3))
}

fn exp_form(a: f64, precision: usize, upper: bool) -> String {
    let s = format!("{a:.precision$e}");
    let (mant, exp) = s.split_once('e').expect("exponent form");
    let exp: i32 = exp.parse().expect("integer exponent");
    let sign = if exp < 0 { '-' } else { '+' };
    let e = if upper { 'E' } else { 'e' };
    format!("{mant}{e}{sign}{:02}", exp.abs())
}

fn format_float(out: &mut String, v: f64, spec: &Spec) -> Result<(), FormatError> {
    let negative = v.is_sign_negative() && !v.is_nan();
    let a = v.abs();
    let upper = matches!(spec.ty, Some('F' | 'E'));
    let body = if !a.is_finite() {
        let s = if a.is_nan() { "nan" } else { "inf" };
        let s = if upper { s.to_uppercase() } else { s.to_owned() };
        if spec.ty == Some('%') { format!("{s}%") } else { s }
    } else {
        match spec.ty {
            Some('f' | 'F') => {
                let p = spec.precision.unwrap_or(6);
                group_number(&format!("{a:.p$}"), spec.grouping)
            }
            Some('%') => {
                let p = spec.precision.unwrap_or(6);
                format!("{}%", group_number(&format!("{:.p$}", a * 100.0), spec.grouping))
            }
            Some('e' | 'E') => exp_form(a, spec.precision.unwrap_or(6), upper),
            None if spec.precision.is_none() => group_number(&float_repr(a), spec.grouping),
            None | Some('g' | 'G') => return invalid("general float format is not supported"),
            Some(c) => return invalid(format!("Unknown format code '{c}' for object of type 'float'")),
        }
    };
    let prefix = sign_prefix(negative, spec);
    pad(out, prefix, &body, spec, '>');
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(t: &str, args: &[(&str, FmtArg)]) -> String {
        format(t, args).unwrap()
    }

    #[test]
    fn python_parity() {
        // Expected values produced by CPython 3.13 str.format.
        assert_eq!(f("{rows:,} rows", &[("rows", 1234567.into())]), "1,234,567 rows");
        assert_eq!(f("{rows:,}", &[("rows", 12.into())]), "12");
        assert_eq!(f("{rows:,}", &[("rows", (-1234).into())]), "-1,234");
        assert_eq!(f("{size:.1f} MB", &[("size", 12.345.into())]), "12.3 MB");
        assert_eq!(f("{size:.1f}", &[("size", 3.into())]), "3.0");
        assert_eq!(f("{x:,}", &[("x", 1234.5.into())]), "1,234.5");
        assert_eq!(f("{x:,.2f}", &[("x", 1234567.891.into())]), "1,234,567.89");
        assert_eq!(f("{x}", &[("x", 1.0.into())]), "1.0");
        assert_eq!(f("{x}", &[("x", 1e20.into())]), "1e+20");
        assert_eq!(f("{x}", &[("x", 0.00001.into())]), "1e-05");
        assert_eq!(f("{x}", &[("x", true.into())]), "True");
        assert_eq!(f("{x:>5}|{y:<4}|{z:^6}", &[("x", 42.into()), ("y", "ab".into()), ("z", "c".into())]), "   42|ab  |  c   ");
        assert_eq!(f("{x:05d}", &[("x", (-42).into())]), "-0042");
        assert_eq!(f("{x:+d}", &[("x", 5.into())]), "+5");
        assert_eq!(f("{x:#x}", &[("x", 255.into())]), "0xff");
        assert_eq!(f("{x:.1%}", &[("x", 0.256.into())]), "25.6%");
        assert_eq!(f("{x:.2e}", &[("x", 12345.0.into())]), "1.23e+04");
        assert_eq!(f("{x:*^7.2s}", &[("x", "hello".into())]), "**he***");
        assert_eq!(f("{x!r}", &[("x", "a".into())]), "'a'");
        assert_eq!(f("{{literal}} {x}", &[("x", 1.into())]), "{literal} 1");
        assert_eq!(f("Ünïcødé {x} ✓", &[("x", "ok".into())]), "Ünïcødé ok ✓");
    }

    #[test]
    fn errors_like_python() {
        assert_eq!(format("{a}", &[]), Err(FormatError::MissingKey("a".into())));
        assert!(format("{a:,}", &[("a", "x".into())]).is_err());
        assert!(format("{a:d}", &[("a", 1.5.into())]).is_err());
        assert!(format("oops }", &[("a", 1.into())]).is_err());
        assert!(format("{a", &[("a", 1.into())]).is_err());
        assert!(format("{0}", &[("a", 1.into())]).is_err());
    }

    #[test]
    fn field_listing() {
        let fs = fields("{a} and {b:,} and {{c}} {d!r:>3}").unwrap();
        let names: Vec<_> = fs.iter().map(|f| (f.name.as_str(), f.spec.as_str())).collect();
        assert_eq!(names, [("a", ""), ("b", ","), ("d", ">3")]);
    }
}
