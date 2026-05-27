"""
Copy Tuple Dialog — choose sampling mode and label for copying column values.
"""
from __future__ import annotations

from PyQt6.QtWidgets import (
    QDialog, QVBoxLayout, QHBoxLayout, QLabel,
    QLineEdit, QSpinBox, QRadioButton, QButtonGroup,
    QPushButton, QGroupBox,
)
from PyQt6.QtCore import Qt


class CopyTupleDialog(QDialog):
    """Dialog for configuring how to copy column values as a tuple."""

    def __init__(
        self,
        column_name: str,
        total_rows: int,
        page_size: int,
        default_sample_size: int = 500,
        parent=None,
    ):
        super().__init__(parent)
        self._column_name = column_name
        self._total_rows = total_rows
        self._page_size = page_size
        self._default_sample_size = default_sample_size
        self.setWindowTitle("Copy Values as Tuple")
        self.setMinimumWidth(360)
        self._setup_ui()

    def _setup_ui(self) -> None:
        layout = QVBoxLayout(self)
        layout.setSpacing(12)

        info = QLabel(
            f"Column: <b>{self._column_name}</b><br>"
            f"Total rows: {self._total_rows:,}"
        )
        info.setTextFormat(Qt.TextFormat.RichText)
        layout.addWidget(info)

        # Sampling options
        options_group = QGroupBox("What to copy")
        options_layout = QVBoxLayout()
        self._mode_group = QButtonGroup(self)

        self._page_radio = QRadioButton(f"Only this page ({self._page_size:,} rows)")
        self._page_radio.setChecked(True)
        self._mode_group.addButton(self._page_radio, 0)
        options_layout.addWidget(self._page_radio)

        self._first_radio = QRadioButton("First N rows")
        self._mode_group.addButton(self._first_radio, 1)
        options_layout.addWidget(self._first_radio)

        self._random_radio = QRadioButton("Random N rows")
        self._mode_group.addButton(self._random_radio, 2)
        options_layout.addWidget(self._random_radio)

        # N spinbox
        n_row = QHBoxLayout()
        n_row.addWidget(QLabel("N:"))
        self._n_spin = QSpinBox()
        self._n_spin.setRange(1, 1000)
        self._n_spin.setValue(min(self._default_sample_size, self._total_rows, 1000))
        n_row.addWidget(self._n_spin)
        n_row.addStretch()
        options_layout.addLayout(n_row)

        options_group.setLayout(options_layout)
        layout.addWidget(options_group)

        # Label input
        label_row = QHBoxLayout()
        label_row.addWidget(QLabel("Label (optional):"))
        self._label_edit = QLineEdit()
        self._label_edit.setPlaceholderText("e.g. my_values")
        label_row.addWidget(self._label_edit)
        layout.addLayout(label_row)

        # Buttons
        btn_row = QHBoxLayout()
        btn_row.addStretch()

        cancel_btn = QPushButton("Cancel")
        cancel_btn.clicked.connect(self.reject)
        btn_row.addWidget(cancel_btn)

        ok_btn = QPushButton("Copy")
        ok_btn.setDefault(True)
        ok_btn.clicked.connect(self.accept)
        btn_row.addWidget(ok_btn)

        layout.addLayout(btn_row)

    def get_result(self) -> tuple[str, int, str] | None:
        """Return (mode, n, label) or None if cancelled.

        mode: 'page', 'first', or 'random'
        """
        if self.result() != QDialog.DialogCode.Accepted:
            return None
        if self._first_radio.isChecked():
            mode = "first"
        elif self._random_radio.isChecked():
            mode = "random"
        else:
            mode = "page"
        return mode, self._n_spin.value(), self._label_edit.text().strip()
