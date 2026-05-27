"""
Function registry — single source of truth for all DSL functions.

Every supported function is declared here with:
- canonical name
- parameter signatures
- return type
- description + example
- sqlglot rendering rule
"""
from __future__ import annotations

from dataclasses import dataclass
from typing import Callable

from parvu.core.dsl.types import LogicalType


@dataclass(frozen=True)
class ParamDef:
    """Parameter definition for a registry function."""
    name: str
    logical_type: LogicalType
    required: bool = True
    description: str = ""


@dataclass(frozen=True)
class FunctionDef:
    """Definition of a function in the ParVu DSL."""
    name: str                      # canonical name: SUM, UPPER, DATEDIFF
    params: list[ParamDef]         # parameter signatures
    return_type: LogicalType
    description: str
    example: str
    sqlglot_name: str | None = None   # "SUM", "DATE_DIFF", etc.
    sqlglot_template: str | None = None  # for special cases like "DATE_DIFF('unit', {0}, {1})"


class FunctionRegistry:
    """Central registry of all DSL functions."""

    def __init__(self):
        self._functions: dict[str, FunctionDef] = {}
        self._methods: dict[str, str] = {}  # method_name -> function_name
        self._register_defaults()

    def register(self, func: FunctionDef) -> None:
        """Add a function to the registry."""
        self._functions[func.name.upper()] = func

    def register_method(self, method_name: str, function_name: str) -> None:
        """Map a dot-method to a registry function."""
        self._methods[method_name.lower()] = function_name.upper()

    def lookup(self, name: str) -> FunctionDef | None:
        """Get a function by canonical name (case-insensitive)."""
        return self._functions.get(name.upper())

    def lookup_method(self, method_name: str) -> FunctionDef | None:
        """Get a function mapped from a dot-method name."""
        func_name = self._methods.get(method_name.lower())
        if func_name:
            return self._functions.get(func_name)
        return None

    def by_return_type(self, t: LogicalType) -> list[FunctionDef]:
        """Get all functions returning a given logical type."""
        return [f for f in self._functions.values() if f.return_type == t]

    def by_param_type(self, t: LogicalType, position: int = 0) -> list[FunctionDef]:
        """Get functions whose Nth parameter accepts a given logical type."""
        result = []
        for f in self._functions.values():
            if position < len(f.params):
                if f.params[position].logical_type == t or f.params[position].logical_type == LogicalType.UNKNOWN:
                    result.append(f)
            elif any(p.logical_type == t or p.logical_type == LogicalType.UNKNOWN for p in f.params):
                result.append(f)
        return result

    def all_functions(self) -> list[FunctionDef]:
        """Return all registered functions."""
        return list(self._functions.values())

    def all_methods(self) -> list[str]:
        """Return all registered method names."""
        return list(self._methods.keys())

    def _register_defaults(self) -> None:
        """Register the built-in function set."""
        from parvu.core.dsl.types import LogicalType as LT

        # Aggregates
        self.register(FunctionDef(
            "SUM", [ParamDef("value", LT.NUMERIC)], LT.NUMERIC,
            "Sum of all values.", "SUM(sales[revenue])", sqlglot_name="SUM"
        ))
        self.register(FunctionDef(
            "AVG", [ParamDef("value", LT.NUMERIC)], LT.NUMERIC,
            "Average of all values.", "AVG(sales[revenue])", sqlglot_name="AVG"
        ))
        self.register(FunctionDef(
            "COUNT", [ParamDef("value", LT.UNKNOWN, required=False)], LT.NUMERIC,
            "Count of rows.", "COUNT(sales[id])", sqlglot_name="COUNT"
        ))
        self.register(FunctionDef(
            "COUNTD", [ParamDef("value", LT.UNKNOWN)], LT.NUMERIC,
            "Count of distinct values.", "COUNTD(sales[id])", sqlglot_name="COUNT_DISTINCT"
        ))
        self.register(FunctionDef(
            "MIN", [ParamDef("value", LT.UNKNOWN)], LT.UNKNOWN,
            "Minimum value.", "MIN(sales[revenue])", sqlglot_name="MIN"
        ))
        self.register(FunctionDef(
            "MAX", [ParamDef("value", LT.UNKNOWN)], LT.UNKNOWN,
            "Maximum value.", "MAX(sales[revenue])", sqlglot_name="MAX"
        ))

        # Conditionals
        self.register(FunctionDef(
            "IF", [
                ParamDef("condition", LT.BOOLEAN),
                ParamDef("true_value", LT.UNKNOWN),
                ParamDef("false_value", LT.UNKNOWN),
            ], LT.UNKNOWN,
            "Return true_value if condition is true, else false_value.",
            "IF(sales[revenue] > 100, 'high', 'low')", sqlglot_name="IF"
        ))
        self.register(FunctionDef(
            "COALESCE", [ParamDef("values", LT.UNKNOWN)], LT.UNKNOWN,
            "Return first non-null value.", "COALESCE(sales[note], 'N/A')", sqlglot_name="COALESCE"
        ))

        # Text
        self.register(FunctionDef(
            "LEN", [ParamDef("text", LT.TEXT)], LT.NUMERIC,
            "Length of text.", "LEN(products[name])", sqlglot_name="LENGTH"
        ))
        self.register(FunctionDef(
            "UPPER", [ParamDef("text", LT.TEXT)], LT.TEXT,
            "Convert text to uppercase.", "UPPER(products[name])", sqlglot_name="UPPER"
        ))
        self.register(FunctionDef(
            "LOWER", [ParamDef("text", LT.TEXT)], LT.TEXT,
            "Convert text to lowercase.", "LOWER(products[name])", sqlglot_name="LOWER"
        ))
        self.register(FunctionDef(
            "TRIM", [ParamDef("text", LT.TEXT)], LT.TEXT,
            "Remove leading and trailing whitespace.", "TRIM(products[name])", sqlglot_name="TRIM"
        ))
        self.register(FunctionDef(
            "CONCAT", [ParamDef("values", LT.TEXT)], LT.TEXT,
            "Concatenate strings.", "CONCAT(products[name], ' - ', products[category])", sqlglot_name="CONCAT"
        ))
        self.register(FunctionDef(
            "CONTAINS", [
                ParamDef("text", LT.TEXT),
                ParamDef("substring", LT.TEXT),
            ], LT.BOOLEAN,
            "Check if text contains substring.", "CONTAINS(products[name], 'apple')", sqlglot_name="CONTAINS"
        ))
        self.register(FunctionDef(
            "STARTSWITH", [
                ParamDef("text", LT.TEXT),
                ParamDef("prefix", LT.TEXT),
            ], LT.BOOLEAN,
            "Check if text starts with prefix.", "STARTSWITH(products[name], 'A')", sqlglot_name="STARTS_WITH"
        ))
        self.register(FunctionDef(
            "ENDSWITH", [
                ParamDef("text", LT.TEXT),
                ParamDef("suffix", LT.TEXT),
            ], LT.BOOLEAN,
            "Check if text ends with suffix.", "ENDSWITH(products[name], 'Z')", sqlglot_name="ENDS_WITH"
        ))

        # Numeric
        self.register(FunctionDef(
            "ABS", [ParamDef("value", LT.NUMERIC)], LT.NUMERIC,
            "Absolute value.", "ABS(sales[discount])", sqlglot_name="ABS"
        ))
        self.register(FunctionDef(
            "ROUND", [
                ParamDef("value", LT.NUMERIC),
                ParamDef("decimals", LT.NUMERIC, required=False),
            ], LT.NUMERIC,
            "Round to given decimal places.", "ROUND(sales[revenue], 2)", sqlglot_name="ROUND"
        ))
        self.register(FunctionDef(
            "FLOOR", [ParamDef("value", LT.NUMERIC)], LT.NUMERIC,
            "Round down to nearest integer.", "FLOOR(sales[revenue])", sqlglot_name="FLOOR"
        ))
        self.register(FunctionDef(
            "CEIL", [ParamDef("value", LT.NUMERIC)], LT.NUMERIC,
            "Round up to nearest integer.", "CEIL(sales[revenue])", sqlglot_name="CEIL"
        ))
        self.register(FunctionDef(
            "POWER", [
                ParamDef("base", LT.NUMERIC),
                ParamDef("exponent", LT.NUMERIC),
            ], LT.NUMERIC,
            "Raise base to exponent power.", "POWER(2, 10)", sqlglot_name="POWER"
        ))
        self.register(FunctionDef(
            "SQRT", [ParamDef("value", LT.NUMERIC)], LT.NUMERIC,
            "Square root.", "SQRT(sales[revenue])", sqlglot_name="SQRT"
        ))

        # Date
        self.register(FunctionDef(
            "DATEDIFF", [
                ParamDef("unit", LT.TEXT),
                ParamDef("start", LT.DATE),
                ParamDef("end", LT.DATE),
            ], LT.NUMERIC,
            "Difference between two dates in given unit.",
            "DATEDIFF('day', orders[start], orders[end])",
            sqlglot_name="DATE_DIFF"
        ))
        self.register(FunctionDef(
            "YEAR", [ParamDef("date", LT.DATE)], LT.NUMERIC,
            "Extract year from date.", "YEAR(orders[date])", sqlglot_name="YEAR"
        ))
        self.register(FunctionDef(
            "MONTH", [ParamDef("date", LT.DATE)], LT.NUMERIC,
            "Extract month from date.", "MONTH(orders[date])", sqlglot_name="MONTH"
        ))
        self.register(FunctionDef(
            "DAY", [ParamDef("date", LT.DATE)], LT.NUMERIC,
            "Extract day from date.", "DAY(orders[date])", sqlglot_name="DAY"
        ))
        self.register(FunctionDef(
            "TODAY", [], LT.DATE,
            "Current date.", "TODAY()", sqlglot_name="CURRENT_DATE"
        ))
        self.register(FunctionDef(
            "NOW", [], LT.TIMESTAMP,
            "Current timestamp.", "NOW()", sqlglot_name="CURRENT_TIMESTAMP"
        ))

        # Type casts
        self.register(FunctionDef(
            "CAST", [
                ParamDef("value", LT.UNKNOWN),
                ParamDef("type", LT.TEXT),
            ], LT.UNKNOWN,
            "Cast value to a different type.", "CAST(sales[id], 'INTEGER')", sqlglot_name="CAST"
        ))
        self.register(FunctionDef(
            "TRY_CAST", [
                ParamDef("value", LT.UNKNOWN),
                ParamDef("type", LT.TEXT),
            ], LT.UNKNOWN,
            "Cast value, returning NULL on failure.", "TRY_CAST(sales[id], 'INTEGER')", sqlglot_name="TRY_CAST"
        ))

        # Table-level operations
        self.register(FunctionDef(
            "DROP_DUPLICATES", [
                ParamDef("table", LT.UNKNOWN),
                ParamDef("columns", LT.UNKNOWN, required=False),
                ParamDef("keep", LT.TEXT, required=False),
            ], LT.QUERY,
            "Remove duplicate rows based on column subset.",
            "DROP_DUPLICATES(sales[id], sales[name], 'first')",
        ))

        # Register method mappings
        from parvu.core.dsl.types import methods_for_type
        for lt in LT:
            for method in methods_for_type(lt):
                self.register_method(method, method.upper())
