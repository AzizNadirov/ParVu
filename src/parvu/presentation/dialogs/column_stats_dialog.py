"""
Column Stats Dialog — show summary statistics for a single column.
"""
from __future__ import annotations

from typing import Any, Callable

from PyQt6.QtCore import Qt
from PyQt6.QtGui import QGuiApplication
from PyQt6.QtWidgets import (
    QDialog,
    QHBoxLayout,
    QHeaderView,
    QLabel,
    QPushButton,
    QTableWidget,
    QTableWidgetItem,
    QVBoxLayout,
    QWidget,
)


def _identity(key: str, **_kw) -> str:
    return key


def _fmt_num(x: float) -> str:
    """Format a number with sensible precision: integers if whole, else 6 sig figs."""
    if x is None:
        return ""
    if isinstance(x, int):
        return f"{x:,}"
    if abs(x - round(x)) < 1e-9 and abs(x) < 1e15:
        return f"{int(round(x)):,}"
    return f"{x:,.6g}"


def _fmt_pct(numerator: int, denominator: int) -> str:
    if denominator == 0:
        return "0%"
    pct = 100.0 * numerator / denominator
    if pct == 0:
        return "0%"
    if pct < 0.01:
        return "<0.01%"
    return f"{pct:.2f}%"


class ColumnStatsDialog(QDialog):
    """Modal dialog displaying column summary statistics."""

    def __init__(
        self,
        column: str,
        stats: dict[str, Any],
        translator: Callable[..., str] | None = None,
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent)
        self._t: Callable[..., str] = translator or _identity
        self.setWindowTitle(self._t("column_stats.title", column=column))
        self.setMinimumWidth(420)
        self._stats = stats
        self._setup_ui(column, stats)

    def _setup_ui(self, column: str, stats: dict[str, Any]) -> None:
        layout = QVBoxLayout(self)
        layout.setSpacing(8)

        header = QLabel(
            self._t(
                "column_stats.header",
                column=column,
                type=stats.get("type") or "?",
            )
        )
        header.setTextFormat(Qt.TextFormat.RichText)
        layout.addWidget(header)

        rows: list[tuple[str, str]] = []

        row_count = stats.get("row_count", 0)
        non_null = stats.get("non_null", 0)
        nulls = stats.get("null", 0)
        distinct = stats.get("distinct", 0)

        rows.append((self._t("column_stats.rows"), _fmt_num(row_count)))
        rows.append((self._t("column_stats.non_null"), _fmt_num(non_null)))
        rows.append(
            (
                self._t("column_stats.null"),
                f"{_fmt_num(nulls)} ({_fmt_pct(nulls, row_count)})",
            )
        )
        rows.append((self._t("column_stats.distinct"), _fmt_num(distinct)))

        # min/max are stringified (any type)
        min_v = stats.get("min")
        max_v = stats.get("max")
        rows.append(
            (self._t("column_stats.min"), str(min_v) if min_v is not None else "—")
        )
        rows.append(
            (self._t("column_stats.max"), str(max_v) if max_v is not None else "—")
        )

        # Numeric-only
        mean = stats.get("mean")
        std = stats.get("std")
        if mean is not None:
            rows.append((self._t("column_stats.mean"), _fmt_num(mean)))
        if std is not None:
            rows.append((self._t("column_stats.std"), _fmt_num(std)))

        table = QTableWidget(len(rows), 2, self)
        table.setHorizontalHeaderLabels(
            [self._t("column_stats.col.stat"), self._t("column_stats.col.value")]
        )
        table.verticalHeader().setVisible(False)
        table.setEditTriggers(QTableWidget.EditTrigger.NoEditTriggers)
        table.setSelectionBehavior(QTableWidget.SelectionBehavior.SelectRows)
        table.setAlternatingRowColors(True)
        for r, (k, v) in enumerate(rows):
            table.setItem(r, 0, QTableWidgetItem(k))
            value_item = QTableWidgetItem(v)
            value_item.setTextAlignment(
                Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter
            )
            table.setItem(r, 1, value_item)
        hdr = table.horizontalHeader()
        hdr.setSectionResizeMode(0, QHeaderView.ResizeMode.ResizeToContents)
        hdr.setSectionResizeMode(1, QHeaderView.ResizeMode.Stretch)
        layout.addWidget(table)

        btn_row = QHBoxLayout()
        btn_row.addStretch()
        copy_btn = QPushButton(self._t("column_stats.copy"))
        copy_btn.setAutoDefault(False)
        copy_btn.clicked.connect(lambda: self._copy_to_clipboard(rows))
        btn_row.addWidget(copy_btn)
        close_btn = QPushButton(self._t("btn.close"))
        close_btn.setDefault(True)
        close_btn.clicked.connect(self.accept)
        btn_row.addWidget(close_btn)
        layout.addLayout(btn_row)

    def _copy_to_clipboard(self, rows: list[tuple[str, str]]) -> None:
        text = "\n".join(f"{k}\t{v}" for k, v in rows)
        QGuiApplication.clipboard().setText(text)
