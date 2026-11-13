"""Generate golden files for the Rust DSL crate (crates/parvu-dsl).

Runs the Python DSL (parser -> resolver -> compiler, plus the QueryEditor
translate logic) over a corpus of expressions and writes
crates/parvu-dsl/tests/golden/dsl.json.

Usage:
    uv run --extra dev python scripts/dsl_golden.py
"""
from __future__ import annotations

import json
import re
from pathlib import Path

from loguru import logger

from parvu.core.dsl.catalog import Catalog, ColumnInfo
from parvu.core.dsl.compiler import Compiler, CompileError
from parvu.core.dsl.ir import Assignment, ColumnRef, DropDuplicates, DropNull, Literal, Replace
from parvu.core.dsl.parser import DSLParser, ParseError
from parvu.core.dsl.registry import FunctionRegistry
from parvu.core.dsl.resolver import Resolver, ResolutionError
from parvu.core.dsl.types import LogicalType, duckdb_type_to_logical, methods_for_type

logger.remove()

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "crates" / "parvu-dsl" / "tests" / "golden" / "dsl.json"

CATALOGS: dict[str, dict[str, list[tuple[str, str]]]] = {
    "main": {
        "sales": [
            ("id", "INTEGER"),
            ("revenue", "DOUBLE"),
            ("qty", "BIGINT"),
            ("price", "DECIMAL(10,2)"),
            ("name", "VARCHAR"),
            ("note", "TEXT"),
            ("flag", "BOOLEAN"),
            ("active", "BOOLEAN"),
            ("d", "DATE"),
            ("ts", "TIMESTAMP WITH TIME ZONE"),
            ("new col", "VARCHAR"),
            ("select", "INTEGER"),
            ("Mixed", "INTEGER"),
            ('x"y', "INTEGER"),
            ("tags", "INTEGER[]"),
            ("café", "VARCHAR"),
            ("TRUE", "INTEGER"),
            ("NOT", "INTEGER"),
        ],
        "products": [
            ("id", "INTEGER"),
            ("name", "VARCHAR"),
            ("category", "VARCHAR"),
        ],
        "order": [("a", "INTEGER")],
    },
    "empty": {},
    "nocols": {"t": []},
}


def make_catalog(spec: dict[str, list[tuple[str, str]]]) -> Catalog:
    cat = Catalog()
    for table, cols in spec.items():
        cat._tabs[table] = {
            name: ColumnInfo(name=name, physical_type=phys, logical_type=duckdb_type_to_logical(phys))
            for name, phys in cols
        }
    return cat


# (source, catalog, table)
CASES: list[tuple[str, str, str]] = []


def add(*sources: str, catalog: str = "main", table: str = "sales") -> None:
    for s in sources:
        CASES.append((s, catalog, table))


# --- literals ---------------------------------------------------------------
add(
    "42", "0", "-0", "00012", "-7", "3.14", "1.50", "-0.0", "-2.5", "0.00001", "0.0001",
    "10000000000000000.0", "1234567890123456.5", "12345678901234567890123",
    "'hello'", '"world"', "''", '""', "\"it's\"", "'a\"b'", "'back\\slash'", "'multi\nline'",
    "'tab\there'", "'ünïcode ✓'", "TRUE", "FALSE", "NULL",
)
# --- column references --------------------------------------------------------
add(
    "sales[id]", "sales[revenue]", 'sales["new col"]', "sales[select]", "sales[Mixed]",
    "sales[café]", "sales[TRUE]", "sales[NOT]", "products[name]", "order[a]",
    "sales [ id ]", "  sales[id]  ", "sales['id']", "sales[\"id\"]",
)
# --- arithmetic / precedence ---------------------------------------------------
add(
    "sales[id] + 1", "sales[id] - 1", "sales[id] * 2", "sales[id] / 2",
    "sales[id] + sales[revenue] * 2", "sales[id] * sales[revenue] + 2",
    "sales[id] - sales[qty] - 1", "sales[id] / sales[qty] / 2", "sales[id] + sales[qty] - 3",
    "1 + 2 * 3 - 4 / 5", "sales[id] -5", "sales[id] - -5", "sales[id] * -1",
    "(sales[id] * 2) + 1", "sales[id] + (1 * 2)", "(sales[id])", "((sales[id]))",
    "sales[id] + (sales[qty] + 1)", "sales[id] * (sales[qty] * 2)",
    "-5.abs", "(1 + 2).abs", "sales[id]+sales[qty]", "sales[id]\n+\tsales[qty]",
)
# --- comparison ----------------------------------------------------------------
add(
    "sales[id] == 1", "sales[id] != 1", "sales[id] <> 1", "sales[id] > 1", "sales[id] >= 1",
    "sales[id] < 1", "sales[id] <= 1", "sales[name] == 'x'", "sales[id] + 1 > sales[qty] * 2",
    "sales[note] == NULL",
)
# --- logical -------------------------------------------------------------------
add(
    "sales[flag] AND sales[active]", "sales[flag] && sales[active]", "sales[flag] OR sales[active]",
    "sales[flag] || sales[active]", "sales[flag] OR sales[active] AND sales[flag]",
    "sales[flag] AND sales[active] OR sales[flag]",
    "sales[id] > 1 AND sales[id] < 10", "sales[id] > 1 OR sales[name] == 'a' AND sales[flag]",
    "sales[flag] ORsales[active]", "sales[flag] ANDsales[active]", "sales[flag] AND TRUE",
    "sales[flag] OR (sales[active] OR sales[flag])", "sales[flag] AND (sales[active] AND sales[flag])",
)
# --- functions (every registered one) --------------------------------------------
add(
    "SUM(sales[revenue])", "sum(sales[revenue])", "Sum(sales[revenue])", "AVG(sales[revenue])",
    "COUNT(sales[id])", "COUNT()", "COUNTD(sales[id])", "MIN(sales[revenue])", "MAX(sales[d])",
    "IF(sales[revenue] > 100, 'high', 'low')", "COALESCE(sales[note], 'N/A')",
    "COALESCE(sales[note], sales[name], 'x')", "LEN(sales[name])", "UPPER(sales[name])",
    "LOWER(sales[name])", "TRIM(sales[name])", "CONCAT(sales[name], ' - ', products[category])",
    "CONTAINS(sales[name], 'apple')", "STARTSWITH(sales[name], 'A')", "ENDSWITH(sales[name], 'Z')",
    "ABS(sales[revenue])", "ROUND(sales[revenue], 2)", "ROUND(sales[revenue])",
    "FLOOR(sales[revenue])", "CEIL(sales[revenue])", "POWER(2, 10)", "SQRT(sales[revenue])",
    "DATEDIFF('day', sales[d], sales[d])", "YEAR(sales[d])", "MONTH(sales[d])", "DAY(sales[d])",
    "TODAY()", "NOW()", "CAST(sales[id], 'INTEGER')", "TRY_CAST(sales[name], 'DOUBLE')",
    "UPPER(sales[name], 1, 2)", "TODAY(1)",
    "SUM(sales[revenue]) / COUNT(sales[id])", "ROUND(SUM(sales[revenue]) / COUNT(sales[id]), 2)",
    "IF(CONTAINS(LOWER(sales[name]), 'a'), UPPER(sales[name]), sales[note])",
    "IF(sales[id] > 1 AND sales[flag], 1, 0)", "ABS(sales[id] - sales[qty]) * 2",
    "UPPER(sales[\"new col\"])", "SUM(sales[select])", "NOTE(1)", "NOTsales[id]",
    "UPPER('x' )", "CONCAT('a', \"b\")",
)
# --- methods (every registered one) ----------------------------------------------
add(
    "sales[name].upper", "sales[name].upper()", "sales[name].UPPER", "sales[name].lower",
    "sales[name].len", "sales[name].trim", "sales[name].contains('a')",
    "sales[name].startswith('a')", "sales[name].endswith('z')", "sales[revenue].abs",
    "sales[revenue].round(2)", "sales[revenue].round", "sales[revenue].floor", "sales[revenue].ceil",
    "sales[revenue].power(2)", "sales[revenue].sqrt", "sales[d].year", "sales[d].month",
    "sales[d].day", "sales[revenue].abs.round(2)", "'x'.upper", "sales[name].upper.len",
    "UPPER(sales[name]).lower", "sales[id].upper", "sales[name] . upper ( )",
    "sales[name].upper + 'x'", "sales[revenue].round(1) > 2",
)
# --- assignments -----------------------------------------------------------------
add(
    "sales[new_col] = sales[revenue] + 100", 'sales["new col"] = 1', "sales[id] = sales[id] * 2",
    "sales[name] = UPPER(sales[name])", "sales[total] = sales[qty] * sales[price]",
    "sales[select] = 1", "sales[flag2] = sales[id] > 5", "sales['q'] = 1", 'sales[""] = 1',
    "sales[x] = NULL", "sales[x]=1", "sales[x] =-5", "sales[x] = sales[name].replace('a', 'b')",
    "products[name] = LOWER(products[name])", "products[upper_name] = UPPER(products[name])",
    "order[b] = order[a] + 1",
)
add("t[x] = 1", catalog="nocols", table="t")
# --- drop_duplicates -------------------------------------------------------------
add(
    "drop_duplicates(sales[id])", "drop_duplicates(sales[id], 'last')",
    "drop_duplicates(sales[id], \"first\")", "drop_duplicates(sales[id], sales[name])",
    "drop_duplicates(sales[id], sales[name], 'first')", "drop_duplicates(sales[id], sales[name], 'last')",
    "drop_duplicates(sales[id], sales[name], 'LAST')", "DROP_DUPLICATES(sales[id], sales[\"new col\"])",
    "drop_duplicates(sales[select], sales[Mixed], 'last')", "drop_duplicates(sales[id], sales[id])",
    "drop_duplicates(products[id], products[name])", "drop_duplicates(order[a])",
    "drop_duplicates()", "drop_duplicates('first')", "drop_duplicates(sales[id], products[id])",
    "drop_duplicates(sales[id], 'middle')", "drop_duplicates(sales[id], 1)",
)
# --- drop_null -------------------------------------------------------------------
add(
    "drop_null(sales[note])", "DROP_NULL(sales[note])", "drop_null(sales[note], NULL)",
    "drop_null(sales[id], -1)", "drop_null(sales[id], 0)", "drop_null(sales[revenue], 1.5)",
    "drop_null(sales[note], 'N/A')", "drop_null(sales[note], '')", "drop_null(sales[note], \"it's\")",
    "drop_null(sales[flag], TRUE)", "drop_null(sales[flag], FALSE)", "drop_null(sales[select])",
    "drop_null(sales[\"new col\"], 'x')", "drop_null(order[a])",
    "drop_null()", "drop_null(1)", "drop_null(sales[id], sales[qty])", "drop_null(sales[id], 1, 2)",
)
# --- replace ---------------------------------------------------------------------
add(
    "REPLACE(sales[name], 'old', 'new')", "replace(sales[name], 'old', 'new')",
    "REPLACE(sales[name], 'old', 'new', TRUE)", "REPLACE(sales[name], 'old', 'new', FALSE)",
    "REPLACE(sales[name], 'old', 'new', TRUE, FALSE)", "REPLACE(sales[name], 'a.b', 'x', FALSE)",
    "REPLACE(sales[name], 'a.b', 'x', TRUE, TRUE)", "REPLACE(sales[name], '[0-9]+', '#', FALSE, TRUE)",
    "REPLACE(sales[name], '(a|b)*?{1}', 'x', FALSE, FALSE)", "REPLACE(sales[name], 'a b&c~d#e-f', 'x', FALSE)",
    "REPLACE(sales[name], sales[note], 'x', FALSE)", "REPLACE(sales[name], 5, 'x', FALSE)",
    "REPLACE(sales[name], 'a', sales[note])", "REPLACE(sales[name], \"it's\", 'x', FALSE)",
    "REPLACE(sales[name], 'a', 'b', TRUE, FALSE, 99)", "sales[name].replace('old', 'new')",
    "sales[name].REPLACE('old', 'new', FALSE)", "sales[name].replace('a.b', 'x', FALSE, FALSE)",
    "sales[name].replace('x', 'y').upper", "UPPER(REPLACE(sales[name], 'a', 'b'))",
    "REPLACE(sales[name], 'old')", "REPLACE(sales[name], 'a', 'b', 1)",
    "REPLACE(sales[name], 'a', 'b', TRUE, 'yes')", "sales[name].replace('a')",
)
# --- scalar table handling ---------------------------------------------------------
add("1 + 1", "'x'", "TODAY()", catalog="empty", table="")
add("products[name]", table="products")
# --- parse errors --------------------------------------------------------------------
add(
    "", "   ", "@#$%^&*", "SUM(", "sales[", "sales[id", "sales[id] +", "1 +* 2", "TRUEX",
    "NULLIF(1)", "true", "null", "1e5", "5abc", "sales[1a]", "sales[id] = = 1",
    "sales[id] + sales[qty] = 1", "(sales[id] = 1)", "sales[id].upper = 1", "f(1,)", "f(,)",
    "sales[id] sales[qty]", "sales", "sales[id] < > 1", "sales[id] === 1", "a.b", "'unterminated",
    "sales[id]..upper", "sales[id] ! 1", "sales[a]b", "UPPER(sales[name]))", "#", "sales[id] | 1",
    "sales[id] & 1",
)
# --- resolve errors ----------------------------------------------------------------
add(
    "unknown[col]", "sales[unknown_col]", "UNKNOWN_FUNC(sales[revenue])", "unknown[x] = 1",
    "POWER(2)", "IF(1, 2)", "DATEDIFF('day')", "sales[d].add_days(1)", "sales[flag].if_true(1, 2)",
    "sales[name].nosuch", "sales[revenue].power", "UPPER(sales[nope])", "sales[x] = sales[nope]",
    "SUM()", "products[revenue]",
)

# Behaviour that the Python implementation gets wrong; the Rust crate fixes it.
# Each of these is listed in the allowlist of tests/golden.rs with the reason.
add(
    "(sales[id] + sales[qty]) * 2", "sales[id] - (sales[qty] - 1)", "sales[id] / (sales[qty] / 2)",
    "(sales[flag] OR sales[active]) AND sales[flag]", "sales[id] > 1 == TRUE",
    "-sales[id]", "- 5", "-(sales[id] + 1)", "NOT sales[flag]", "!sales[flag]",
    "NOT (sales[flag] AND sales[active])", "+sales[id]",
    "UPPER(drop_null(sales[note]))", "sales[n] = drop_duplicates(sales[id])",
    "drop_null(sales[note], 'x') + 1",
)


def error_kind(exc: Exception) -> str:
    if isinstance(exc, ParseError):
        return "parse"
    if isinstance(exc, ResolutionError):
        return "resolve"
    if isinstance(exc, CompileError):
        return "compile"
    return "internal"


def build_assignment_query(compiler: Compiler, catalog: Catalog, expr: Assignment) -> str:
    """Copy of QueryEditor._build_assignment_query."""
    table = expr.table
    column = expr.column
    value_sql = compiler.compile(expr.value)
    all_cols: list[str] = []
    if catalog and table in catalog.tables():
        all_cols = list(catalog.columns(table))
    if not all_cols:
        return f'SELECT *, {value_sql} AS "{column}" FROM {table}'
    select_parts: list[str] = []
    for c in all_cols:
        if c == column:
            select_parts.append(f'{value_sql} AS "{c}"')
        else:
            select_parts.append(f'"{c}"')
    if column not in all_cols:
        select_parts.append(f'{value_sql} AS "{column}"')
    return f"SELECT {', '.join(select_parts)} FROM {table}"


def translate(source: str, catalog: Catalog, table: str) -> dict:
    """QueryEditor.get_query + get_expression_info for the active `table`.

    Deviations from QueryEditor (both intended):
    - the scalar case wraps with the given active table instead of
      catalog.tables()[0];
    - DropNull is treated as a table-level query like DropDuplicates
      (QueryEditor wraps it in another SELECT, producing invalid SQL).
    """
    registry = FunctionRegistry()
    parser = DSLParser()
    resolver = Resolver(catalog, registry)
    compiler = Compiler(registry)
    try:
        expr = resolver.resolve(parser.parse(source.strip()))
        compiled = compiler.compile(expr)
        if isinstance(expr, (DropDuplicates, DropNull)):
            return {"sql": compiled, "kind": {"type": "table_query"}}
        if isinstance(expr, Assignment):
            return {
                "sql": build_assignment_query(compiler, catalog, expr),
                "kind": {"type": "assignment", "column": expr.column},
            }
        sql = f"SELECT {compiled} FROM {table}" if catalog.tables() else compiled
        return {"sql": sql, "kind": {"type": "scalar"}}
    except Exception as exc:  # noqa: BLE001
        return {"error_kind": error_kind(exc)}


def preview(source: str, catalog: Catalog) -> dict:
    """QueryEditor._update_preview."""
    registry = FunctionRegistry()
    try:
        expr = Resolver(catalog, registry).resolve(DSLParser().parse(source.strip()))
        return {"sql": Compiler(registry).compile(expr)}
    except Exception as exc:  # noqa: BLE001
        return {"error_kind": error_kind(exc)}


REPLACE_CASES = [
    ("sales", "name", "old", "new", True, False),
    ("sales", "name", "old", "new", False, False),
    ("sales", "name", "a.b", "x", False, False),
    ("sales", "name", "[0-9]+", "#", True, True),
    ("sales", "name", "[a-z]+", "#", False, True),
    ("sales", "new col", "it's", "it is", True, False),
    ("sales", "select", "x", "y", False, False),
    ("my_tab", "col", "(1+1)", "2", False, False),
    ("data", "data", "data.x", "y", True, False),
    ("sales", "name", "a b\tc", "", False, False),
]


def compile_replace(table, column, pattern, replacement, cs, rx) -> tuple[str, str]:
    """Copy of the Replace IR built by MainWindow._run_replace_dialog."""
    expr = Replace(
        text=ColumnRef(table=table, column=column, logical_type=LogicalType.TEXT),
        pattern=Literal(value=pattern, logical_type=LogicalType.TEXT),
        with_value=Literal(value=replacement, logical_type=LogicalType.TEXT),
        case_sensitive=cs,
        regex=rx,
    )
    sql = Compiler(FunctionRegistry()).compile(expr)
    stripped = re.sub(rf"\b{re.escape(table)}\.", "", sql)
    return sql, stripped


def registry_dump() -> dict:
    reg = FunctionRegistry()
    return {
        "functions": [
            {
                "name": f.name,
                "params": [
                    {
                        "name": p.name,
                        "logical_type": p.logical_type.name,
                        "required": p.required,
                        "description": p.description,
                    }
                    for p in f.params
                ],
                "return_type": f.return_type.name,
                "description": f.description,
                "example": f.example,
                "sql_name": f.sqlglot_name,
            }
            for f in reg.all_functions()
        ],
        "methods": reg.all_methods(),
        "methods_for_type": {lt.name: methods_for_type(lt) for lt in LogicalType},
        "method_targets": {
            m: (reg.lookup_method(m).name if reg.lookup_method(m) else None) for m in reg.all_methods()
        },
    }


def main() -> None:
    seen: set[tuple[str, str, str]] = set()
    cases = []
    for source, cat_name, table in CASES:
        key = (source, cat_name, table)
        if key in seen:
            raise SystemExit(f"duplicate case: {key!r}")
        seen.add(key)
        catalog = make_catalog(CATALOGS[cat_name])
        cases.append(
            {
                "source": source,
                "table": table,
                "catalog": cat_name,
                "expected": translate(source, catalog, table),
                "preview": preview(source, catalog),
            }
        )
    replace_cases = []
    for table, column, pattern, replacement, cs, rx in REPLACE_CASES:
        sql, stripped = compile_replace(table, column, pattern, replacement, cs, rx)
        replace_cases.append(
            {
                "table": table,
                "column": column,
                "pattern": pattern,
                "replacement": replacement,
                "case_sensitive": cs,
                "regex": rx,
                "sql": sql,
                "stripped": stripped,
            }
        )
    doc = {
        "catalogs": {
            name: [{"name": t, "columns": [list(c) for c in cols]} for t, cols in spec.items()]
            for name, spec in CATALOGS.items()
        },
        "cases": cases,
        "replace_cases": replace_cases,
        "registry": registry_dump(),
    }
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(doc, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    n_err = sum(1 for c in cases if "error_kind" in c["expected"])
    print(f"wrote {OUT.relative_to(ROOT)}: {len(cases)} cases ({n_err} errors), "
          f"{len(replace_cases)} replace cases")


if __name__ == "__main__":
    main()
