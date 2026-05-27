"""
Expression Language Help Dialog — comprehensive DSL reference.

Shows syntax overview, operators, and a dynamically-generated
function reference table from the FunctionRegistry.
"""
from __future__ import annotations

from PyQt6.QtWidgets import QDialog, QVBoxLayout, QPushButton, QTextBrowser
from PyQt6.QtCore import Qt

from parvu.infrastructure.i18n.translator import _Translator
from parvu.core.dsl.registry import FunctionRegistry
from parvu.core.dsl.types import LogicalType


class ExpressionHelpDialog(QDialog):
    """Help dialog showing expression language syntax and function reference."""

    def __init__(
        self,
        translator: _Translator,
        registry: FunctionRegistry | None = None,
        parent=None,
    ):
        super().__init__(parent)
        self._t = translator
        self._registry = registry or FunctionRegistry()
        self._setup_ui()

    def _setup_ui(self) -> None:
        self.setWindowTitle(self._t("dialog.expression_help"))
        self.resize(800, 700)

        layout = QVBoxLayout(self)

        browser = QTextBrowser()
        browser.setOpenExternalLinks(True)
        browser.setMarkdown(self._build_content())
        layout.addWidget(browser)

        close_btn = QPushButton(self._t("btn.close"))
        close_btn.clicked.connect(self.close)
        layout.addWidget(close_btn)

    def _build_content(self) -> str:
        t = self._t
        parts = [f"# {t('expr.title')}\n"]

        # Overview
        parts.append(f"\n## {t('expr.overview')}\n")
        parts.append(t("expr.overview_text"))

        # Column References
        parts.append(f"\n## {t('expr.column_refs')}\n")
        parts.append(t("expr.column_refs_text"))
        parts.append("\n```\ndata[column_name]       # table + column\ndata[\"column name\"]     # quoted for names with spaces\n```")

        # Assignment
        parts.append(f"\n## {t('expr.assignment')}\n")
        parts.append(t("expr.assignment_text"))
        parts.append("\n```\ndata[discount] = price * 0.15\ndata[full_name] = first_name || ' ' || last_name\n```")

        # Literals
        parts.append(f"\n## {t('expr.literals')}\n")
        parts.append(t("expr.literals_text"))

        # Operators
        parts.append(f"\n## {t('expr.operators')}\n")
        parts.append(t("expr.operators_text"))

        # Functions
        parts.append(f"\n## {t('expr.functions')}\n")
        parts.append(t("expr.functions_text"))
        parts.append(self._build_function_tables())

        # Methods
        parts.append(f"\n## {t('expr.methods')}\n")
        parts.append(t("expr.methods_text"))

        # Examples
        parts.append(f"\n## {t('expr.examples')}\n")
        parts.append(t("expr.examples_text"))

        # Auto-completion
        parts.append(f"\n## {t('expr.autocomplete')}\n")
        parts.append(t("expr.autocomplete_text"))

        # Errors
        parts.append(f"\n## {t('expr.errors')}\n")
        parts.append(t("expr.errors_text"))

        return "\n".join(parts)

    def _build_function_tables(self) -> str:
        """Generate markdown tables from the function registry."""
        t = self._t
        lines: list[str] = []

        categories = [
            ("expr.aggregates", ["SUM", "AVG", "COUNT", "COUNTD", "MIN", "MAX"]),
            ("expr.conditionals", ["IF", "COALESCE"]),
            ("expr.text", ["LEN", "UPPER", "LOWER", "TRIM", "CONCAT", "CONTAINS", "STARTSWITH", "ENDSWITH", "REPLACE"]),
            ("expr.numeric", ["ABS", "ROUND", "FLOOR", "CEIL", "POWER", "SQRT"]),
            ("expr.date", ["DATEDIFF", "YEAR", "MONTH", "DAY", "TODAY", "NOW"]),
            ("expr.type_casts", ["CAST", "TRY_CAST"]),
            ("expr.special", ["DROP_DUPLICATES"]),
        ]

        for section_key, func_names in categories:
            section_funcs = []
            for name in func_names:
                fdef = self._registry.lookup(name)
                if fdef:
                    section_funcs.append(fdef)

            if not section_funcs:
                continue

            lines.append(f"\n### {t(section_key)}\n")
            lines.append("| Function | Signature | Description |")
            lines.append("|----------|-----------|-------------|")

            for fdef in section_funcs:
                sig = self._format_signature(fdef)
                desc = fdef.description.replace("|", "\\|")
                lines.append(f"| `{fdef.name}` | `{sig}` | {desc} |")

        return "\n".join(lines)

    @staticmethod
    def _format_signature(fdef) -> str:
        """Build a human-readable signature from a FunctionDef."""
        params = []
        for p in fdef.params:
            if p.required:
                params.append(p.name)
            else:
                params.append(f"[{p.name}]")
        return f"{fdef.name}({', '.join(params)})"
