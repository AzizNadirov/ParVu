"""
Keyboard Shortcuts cheatsheet — a flat modal listing every action and its key.
"""
from __future__ import annotations

from typing import Callable

from PyQt6.QtCore import Qt
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


# Translation-key tuples. Each row references locale keys; the actual strings
# come from the translator at render time so language changes are honored.
SHORTCUT_GROUPS: list[tuple[str, list[tuple[str, str]]]] = [
    (
        "shortcuts.group.file",
        [
            ("shortcuts.action.new_window", "Ctrl+N"),
            ("shortcuts.action.open_file", "Ctrl+O"),
            ("shortcuts.action.save", "Ctrl+S"),
            ("shortcuts.action.save_as", "Ctrl+Shift+S"),
            ("shortcuts.action.drag_drop", "shortcuts.value.drag_drop"),
        ],
    ),
    (
        "shortcuts.group.table",
        [
            ("shortcuts.action.sort", "shortcuts.value.sort"),
            ("shortcuts.action.column_actions", "shortcuts.value.column_actions"),
            ("shortcuts.action.cell_actions", "shortcuts.value.cell_actions"),
            ("shortcuts.action.copy_tsv", "Ctrl+C"),
            ("shortcuts.action.copy_other", "shortcuts.value.copy_other"),
        ],
    ),
    (
        "shortcuts.group.find",
        [
            ("shortcuts.action.find", "Ctrl+F"),
            ("shortcuts.action.find_next", "shortcuts.value.find_next"),
            ("shortcuts.action.find_prev", "shortcuts.value.find_prev"),
            ("shortcuts.action.find_close", "Esc"),
        ],
    ),
    (
        "shortcuts.group.help",
        [
            ("shortcuts.action.cheatsheet", "Ctrl+/"),
        ],
    ),
]


def _maybe_translate(t: Callable[..., str], value: str) -> str:
    """Translate ``value`` if it looks like a translation key; otherwise return as-is."""
    if "." in value and not value.startswith(("Ctrl", "Shift", "Alt", "F")):
        return t(value)
    return value


class ShortcutsDialog(QDialog):
    """Modal dialog showing all available keyboard shortcuts."""

    def __init__(
        self,
        translator: Callable[..., str],
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent)
        self._t = translator
        self.setWindowTitle(self._t("shortcuts.title"))
        self.setMinimumSize(520, 480)

        rows: list[tuple[str, str, str]] = []
        for group_key, items in SHORTCUT_GROUPS:
            group_label = self._t(group_key)
            for action_key, key_value in items:
                action_label = self._t(action_key)
                shortcut_label = _maybe_translate(self._t, key_value)
                rows.append((group_label, action_label, shortcut_label))

        layout = QVBoxLayout(self)

        intro = QLabel(self._t("shortcuts.intro"))
        intro.setTextFormat(Qt.TextFormat.RichText)
        layout.addWidget(intro)

        table = QTableWidget(len(rows), 3, self)
        table.setHorizontalHeaderLabels(
            [
                self._t("shortcuts.col.group"),
                self._t("shortcuts.col.action"),
                self._t("shortcuts.col.shortcut"),
            ]
        )
        table.verticalHeader().setVisible(False)
        table.setEditTriggers(QTableWidget.EditTrigger.NoEditTriggers)
        table.setSelectionBehavior(QTableWidget.SelectionBehavior.SelectRows)
        table.setAlternatingRowColors(True)

        for r, (g, a, k) in enumerate(rows):
            table.setItem(r, 0, QTableWidgetItem(g))
            table.setItem(r, 1, QTableWidgetItem(a))
            table.setItem(r, 2, QTableWidgetItem(k))

        header = table.horizontalHeader()
        header.setSectionResizeMode(0, QHeaderView.ResizeMode.ResizeToContents)
        header.setSectionResizeMode(1, QHeaderView.ResizeMode.Stretch)
        header.setSectionResizeMode(2, QHeaderView.ResizeMode.ResizeToContents)

        layout.addWidget(table)

        btn_row = QHBoxLayout()
        btn_row.addStretch(1)
        close = QPushButton(self._t("btn.close"))
        close.clicked.connect(self.accept)
        btn_row.addWidget(close)
        layout.addLayout(btn_row)
