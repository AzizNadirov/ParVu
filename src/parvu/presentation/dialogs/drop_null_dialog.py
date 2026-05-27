"""
Drop Null Dialog — pick a column and optionally a custom null sentinel,
then drop rows where the column matches.
"""
from __future__ import annotations

from typing import Callable

from PyQt6.QtWidgets import (
    QCheckBox,
    QComboBox,
    QDialog,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QPushButton,
    QVBoxLayout,
    QWidget,
)


def _identity(key: str, **_kw) -> str:
    return key


class DropNullDialog(QDialog):
    """Configure a DROP_NULL(column, null_value) transform.

    Result: ``(column_name, null_value)`` where ``null_value`` is ``None``
    when "real NULL" mode is selected, or a string when a sentinel was
    supplied.
    """

    def __init__(
        self,
        table_name: str,
        columns: list[str],
        translator: Callable[..., str] | None = None,
        selected_column: str | None = None,
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent)
        self._t: Callable[..., str] = translator or _identity
        self._table_name = table_name
        self._columns = columns
        self.setWindowTitle(self._t("dialog.drop_null.title"))
        self.setMinimumWidth(380)
        self._setup_ui(selected_column)

    def _setup_ui(self, selected_column: str | None) -> None:
        layout = QVBoxLayout(self)
        layout.setSpacing(10)

        layout.addWidget(
            QLabel(
                self._t("dialog.drop_null.table", table=self._table_name)
            )
        )

        # Column picker
        col_row = QHBoxLayout()
        col_row.addWidget(QLabel(self._t("dialog.drop_null.column")))
        self._column_combo = QComboBox()
        self._column_combo.addItems(self._columns)
        if selected_column and selected_column in self._columns:
            self._column_combo.setCurrentText(selected_column)
        col_row.addWidget(self._column_combo, stretch=1)
        layout.addLayout(col_row)

        # Sentinel toggle
        self._sentinel_check = QCheckBox(self._t("dialog.drop_null.use_sentinel"))
        self._sentinel_check.toggled.connect(self._toggle_sentinel)
        layout.addWidget(self._sentinel_check)

        # Sentinel value input
        val_row = QHBoxLayout()
        val_row.addWidget(QLabel(self._t("dialog.drop_null.value")))
        self._value_input = QLineEdit()
        self._value_input.setPlaceholderText(
            self._t("dialog.drop_null.value_placeholder")
        )
        self._value_input.setEnabled(False)
        val_row.addWidget(self._value_input, stretch=1)
        layout.addLayout(val_row)

        self._hint = QLabel(self._t("dialog.drop_null.hint_null"))
        self._hint.setStyleSheet("color: gray;")
        self._hint.setWordWrap(True)
        layout.addWidget(self._hint)

        # Buttons
        btn_row = QHBoxLayout()
        btn_row.addStretch()
        cancel_btn = QPushButton(self._t("btn.cancel"))
        cancel_btn.clicked.connect(self.reject)
        btn_row.addWidget(cancel_btn)
        ok_btn = QPushButton(self._t("btn.apply"))
        ok_btn.clicked.connect(self.accept)
        ok_btn.setDefault(True)
        btn_row.addWidget(ok_btn)
        layout.addLayout(btn_row)

    def _toggle_sentinel(self, checked: bool) -> None:
        self._value_input.setEnabled(checked)
        if checked:
            self._hint.setText(self._t("dialog.drop_null.hint_sentinel"))
            self._value_input.setFocus()
        else:
            self._hint.setText(self._t("dialog.drop_null.hint_null"))

    def get_result(self) -> tuple[str, object] | None:
        """Return ``(column, null_value)`` or ``None`` if cancelled.

        ``null_value`` is ``None`` for real-NULL mode; otherwise it's the
        sentinel string the user typed (which the caller may parse as a
        number/bool if needed).
        """
        if self.result() != QDialog.DialogCode.Accepted:
            return None
        col = self._column_combo.currentText()
        if not col:
            return None
        if self._sentinel_check.isChecked():
            return col, self._value_input.text()
        return col, None
