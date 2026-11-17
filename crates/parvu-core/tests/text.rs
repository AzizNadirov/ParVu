//! Column functions (text and yes/no) and merging columns.

use parvu_core::Workspace;
use parvu_core::colfn::{ColumnFn, Output, Side};
use parvu_core::ops::Operation;
use tempfile::TempDir;

fn open() -> (TempDir, Workspace, u64) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("people.csv");
    std::fs::write(
        &path,
        "name,code,email,paid,city\n\
         \"  ali   VELİ  \",AB-123-x,ali.veli@mail.az,true,Bakı\n\
         \"zəhra\tqasımova\",CD-9,zehra@x.com,false,Şəki\n\
         ,EF,,,\n",
    )
    .unwrap();
    let mut ws = Workspace::new(100, None).unwrap();
    let id = ws.open(&path).unwrap();
    (dir, ws, id)
}

fn column(ws: &Workspace, id: u64, name: &str) -> Vec<Option<String>> {
    let page = ws.page(id, 1).unwrap();
    let i = page.columns.iter().position(|c| c.name == name).unwrap_or_else(|| panic!("no column {name}"));
    page.rows.into_iter().map(|r| r[i].clone()).collect()
}

fn names(ws: &Workspace, id: u64) -> Vec<String> {
    ws.columns(id).unwrap().into_iter().map(|c| c.name).collect()
}

fn apply(ws: &mut Workspace, id: u64, column: &str, func: ColumnFn, output: Output) {
    ws.apply(id, &Operation::ColumnTransform { column: column.into(), func, output, filter: None }).unwrap();
}

fn s(v: &[Option<&str>]) -> Vec<Option<String>> {
    v.iter().map(|x| x.map(str::to_string)).collect()
}

#[test]
fn clean_up_text_in_place() {
    let (_d, mut ws, id) = open();
    apply(&mut ws, id, "name", ColumnFn::CollapseSpaces, Output::InPlace);
    assert_eq!(column(&ws, id, "name"), s(&[Some("ali VELİ"), Some("zəhra qasımova"), None]));
    apply(&mut ws, id, "name", ColumnFn::Proper, Output::InPlace);
    assert_eq!(column(&ws, id, "name")[0].as_deref(), Some("Ali Veli"));
    assert_eq!(column(&ws, id, "name")[1].as_deref(), Some("Zəhra Qasımova"));
    apply(&mut ws, id, "city", ColumnFn::RemoveAccents, Output::InPlace);
    assert_eq!(column(&ws, id, "city")[1].as_deref(), Some("Səki"), "Ş loses its cedilla; ə is a letter, not an accent");
    apply(&mut ws, id, "code", ColumnFn::Pad { length: 8, fill: "0".into(), side: Side::Start }, Output::InPlace);
    assert_eq!(column(&ws, id, "code"), s(&[Some("AB-123-x"), Some("0000CD-9"), Some("000000EF")]), "longer text is kept whole");
    assert_eq!(names(&ws, id), ["name", "code", "email", "paid", "city"], "in place: same columns");
    assert_eq!(ws.summary(id).unwrap().steps.len(), 4);
}

#[test]
fn extract_into_new_columns_next_to_the_source() {
    let (_d, mut ws, id) = open();
    let before = |last| ColumnFn::Before { delimiter: "-".into(), last };
    let after = |last| ColumnFn::After { delimiter: "-".into(), last };
    apply(&mut ws, id, "code", before(false), Output::New { name: String::new() });
    apply(&mut ws, id, "code", after(true), Output::New { name: "tail".into() });
    apply(&mut ws, id, "code", before(true), Output::New { name: "head".into() });
    apply(&mut ws, id, "code", after(false), Output::New { name: "rest".into() });
    assert_eq!(names(&ws, id), ["name", "code", "rest", "head", "tail", "code_before", "email", "paid", "city"]);
    assert_eq!(column(&ws, id, "code_before"), s(&[Some("AB"), Some("CD"), None]), "no delimiter: empty");
    assert_eq!(column(&ws, id, "rest"), s(&[Some("123-x"), Some("9"), None]));
    assert_eq!(column(&ws, id, "head"), s(&[Some("AB-123"), Some("CD"), None]));
    assert_eq!(column(&ws, id, "tail"), s(&[Some("x"), Some("9"), None]));

    apply(&mut ws, id, "email", ColumnFn::Between { start: "@".into(), end: ".".into() }, Output::New { name: "host".into() });
    assert_eq!(column(&ws, id, "host"), s(&[Some("mail"), Some("x"), None]));
    apply(&mut ws, id, "email", ColumnFn::RegexExtract { pattern: r"\.(\w+)$".into(), group: 1 }, Output::New { name: "tld".into() });
    assert_eq!(column(&ws, id, "tld"), s(&[Some("az"), Some("com"), None]));
    apply(&mut ws, id, "city", ColumnFn::Length, Output::New { name: String::new() });
    assert_eq!(column(&ws, id, "city_length"), s(&[Some("4"), Some("4"), None]), "characters, not bytes");
    apply(&mut ws, id, "city", ColumnFn::First { n: 2 }, Output::New { name: String::new() });
    assert_eq!(column(&ws, id, "city_first")[0].as_deref(), Some("Ba"));
}

#[test]
fn yes_no_and_merging() {
    let (_d, mut ws, id) = open();
    apply(&mut ws, id, "paid", ColumnFn::Invert, Output::InPlace);
    assert_eq!(column(&ws, id, "paid"), s(&[Some("false"), Some("true"), None]));
    let invert_text = Operation::ColumnTransform { column: "city".into(), func: ColumnFn::Invert, output: Output::InPlace, filter: None };
    assert!(ws.apply(id, &invert_text).is_err());

    let merge = |keep| Operation::MergeColumns {
        columns: vec!["code".into(), "city".into()],
        separator: " / ".into(),
        name: "label".into(),
        keep_columns: keep,
    };
    ws.apply(id, &merge(false)).unwrap();
    assert_eq!(names(&ws, id), ["name", "label", "email", "paid"]);
    assert_eq!(column(&ws, id, "label"), s(&[Some("AB-123-x / Bakı"), Some("CD-9 / Şəki"), Some("EF")]), "empty cells skipped");
    ws.undo(id).unwrap();
    ws.apply(id, &merge(true)).unwrap();
    assert_eq!(names(&ws, id), ["name", "code", "label", "email", "paid", "city"]);
}

#[test]
fn bad_input_is_refused_and_text_stays_text() {
    let (_d, mut ws, id) = open();
    let bad_regex = Operation::ColumnTransform {
        column: "email".into(),
        func: ColumnFn::RegexExtract { pattern: "(".into(), group: 0 },
        output: Output::InPlace,
        filter: None,
    };
    assert!(ws.apply(id, &bad_regex).is_err());
    apply(&mut ws, id, "city", ColumnFn::AddPrefix { text: "x'); DROP TABLE t; --".into() }, Output::InPlace);
    assert_eq!(column(&ws, id, "city")[0].as_deref(), Some("x'); DROP TABLE t; --Bakı"));
}

#[test]
fn preview_runs_on_the_given_values() {
    let (_d, ws, id) = open();
    let rows = ws
        .preview_column_fn(id, "code", &ColumnFn::Upper, &[Some("ab-1".into()), None])
        .unwrap();
    assert_eq!(rows, vec![(Some("ab-1".into()), Some("AB-1".into())), (None, None)]);
    let flags = ws.preview_column_fn(id, "paid", &ColumnFn::Invert, &[Some("true".into())]).unwrap();
    assert_eq!(flags[0].1.as_deref(), Some("false"));
}

#[test]
fn split_into_rows_and_matches() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("tags.csv");
    std::fs::write(&path, "id,tags\n1,\"red, Blue\"\n2,\n3,green\n").unwrap();
    let mut ws = Workspace::new(100, None).unwrap();
    let id = ws.open(&path).unwrap();

    use parvu_core::colfn::MatchHow;
    let m = |how, ignore_case| ColumnFn::Matches { text: "blue".into(), how, ignore_case };
    apply(&mut ws, id, "tags", m(MatchHow::Contains, true), Output::New { name: "blue".into() });
    assert_eq!(column(&ws, id, "blue"), s(&[Some("true"), None, Some("false")]));
    ws.undo(id).unwrap();
    apply(&mut ws, id, "tags", m(MatchHow::Contains, false), Output::New { name: "blue".into() });
    assert_eq!(column(&ws, id, "blue")[0].as_deref(), Some("false"), "case matters unless ignored");
    ws.undo(id).unwrap();
    apply(&mut ws, id, "tags", ColumnFn::Matches { text: "^r".into(), how: MatchHow::Regex, ignore_case: false }, Output::New { name: "r".into() });
    assert_eq!(column(&ws, id, "r")[0].as_deref(), Some("true"));
    ws.undo(id).unwrap();

    ws.apply(id, &Operation::SplitToRows { column: "tags".into(), delimiter: ",".into(), trim: true }).unwrap();
    assert_eq!(column(&ws, id, "tags"), s(&[Some("red"), Some("Blue"), None, Some("green")]));
    assert!(ws.apply(id, &Operation::SplitToRows { column: "tags".into(), delimiter: String::new(), trim: false }).is_err());
}
