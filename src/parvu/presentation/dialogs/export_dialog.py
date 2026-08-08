"""
Export Dialog — target file, per-column type casting, date formatting and
format-specific writer options.
"""
from __future__ import annotations

from pathlib import Path

from PyQt6.QtCore import Qt
from PyQt6.QtWidgets import (
    QCheckBox,
    QComboBox,
    QDialog,
    QFileDialog,
    QFormLayout,
    QGroupBox,
    QHBoxLayout,
    QHeaderView,
    QLabel,
    QLineEdit,
    QMessageBox,
    QPushButton,
    QStackedWidget,
    QTableWidget,
    QTableWidgetItem,
    QVBoxLayout,
    QWidget,
)

from parvu.core.exporter import (
    CAST_TYPES,
    SUFFIX_BY_FORMAT,
    ColumnSpec,
    ExportOptions,
    format_for_path,
    is_temporal,
)

FORMAT_LABELS = [("CSV", "csv"), ("Parquet", "parquet"), ("JSON", "json")]

FILE_FILTERS = {
    "csv": "CSV Files (*.csv *.tsv)",
    "parquet": "Parquet Files (*.parquet)",
    "json": "JSON Files (*.json *.jsonl *.ndjson)",
}

DATE_PRESETS = ["", "%Y-%m-%d", "%d/%m/%Y", "%m/%d/%Y", "%d.%m.%Y", "%Y%m%d"]
TIMESTAMP_PRESETS = [
    "",
    "%Y-%m-%d %H:%M:%S",
    "%Y-%m-%dT%H:%M:%S",
    "%d/%m/%Y %H:%M",
    "%Y-%m-%d",
]

DELIMITERS = [("Comma  ,", ","), ("Semicolon  ;", ";"), ("Tab  \\t", "\t"), ("Pipe  |", "|")]


class ExportDialog(QDialog):
    """Collects ExportOptions for the active table."""

    def __init__(
        self,
        default_path: Path,
        column_types: list[tuple[str, str]],
        parent=None,
        pending_edits: int = 0,
    ):
        super().__init__(parent)
        self._column_types = column_types
        self._cast_combos: list[QComboBox] = []
        self.setWindowTitle("Export")
        self.setModal(True)
        self.resize(620, 560)
        self._setup_ui(default_path, pending_edits)
        self._on_format_changed()

    # ------------------------------------------------------------------
    # UI
    # ------------------------------------------------------------------

    def _setup_ui(self, default_path: Path, pending_edits: int) -> None:
        layout = QVBoxLayout(self)
        layout.setSpacing(10)

        # ── Target ──
        target = QFormLayout()
        path_row = QHBoxLayout()
        self._path_edit = QLineEdit(str(default_path))
        path_row.addWidget(self._path_edit)
        browse = QPushButton("Browse…")
        browse.clicked.connect(self._browse)
        path_row.addWidget(browse)
        target.addRow("File:", path_row)

        self._format_combo = QComboBox()
        for label, _ in FORMAT_LABELS:
            self._format_combo.addItem(label)
        self._format_combo.setCurrentIndex(
            [f for _, f in FORMAT_LABELS].index(format_for_path(default_path))
        )
        self._format_combo.currentIndexChanged.connect(self._on_format_changed)
        target.addRow("Format:", self._format_combo)
        layout.addLayout(target)

        if pending_edits:
            note = QLabel(
                f"ℹ️ {pending_edits} unsaved cell edit(s) will be included in the export."
            )
            note.setWordWrap(True)
            layout.addWidget(note)

        # ── Columns / type casting ──
        cols_group = QGroupBox("Columns and types")
        cols_layout = QVBoxLayout(cols_group)

        self._table = QTableWidget(len(self._column_types), 3)
        self._table.setMinimumHeight(200)
        self._table.setHorizontalHeaderLabels(["Column", "Source type", "Export as"])
        self._table.verticalHeader().setVisible(False)
        header = self._table.horizontalHeader()
        header.setSectionResizeMode(0, QHeaderView.ResizeMode.Stretch)
        header.setSectionResizeMode(1, QHeaderView.ResizeMode.ResizeToContents)
        header.setSectionResizeMode(2, QHeaderView.ResizeMode.ResizeToContents)

        for row, (name, dtype) in enumerate(self._column_types):
            item = QTableWidgetItem(name)
            item.setFlags(item.flags() | Qt.ItemFlag.ItemIsUserCheckable)
            item.setCheckState(Qt.CheckState.Checked)
            self._table.setItem(row, 0, item)

            type_item = QTableWidgetItem(dtype)
            type_item.setFlags(type_item.flags() & ~Qt.ItemFlag.ItemIsEditable)
            self._table.setItem(row, 1, type_item)

            combo = QComboBox()
            for t in CAST_TYPES:
                combo.addItem("Keep" if t == "" else t, t)
            self._cast_combos.append(combo)
            self._table.setCellWidget(row, 2, combo)
        cols_layout.addWidget(self._table)

        btn_row = QHBoxLayout()
        toggles = (("Select all", Qt.CheckState.Checked), ("Select none", Qt.CheckState.Unchecked))
        for text, state in toggles:
            btn = QPushButton(text)
            btn.clicked.connect(lambda _, s=state: self._set_all_checked(s))
            btn_row.addWidget(btn)
        btn_row.addStretch()
        cols_layout.addLayout(btn_row)
        layout.addWidget(cols_group, 1)

        # ── Date / time formatting ──
        self._dates_group = QGroupBox("Date and time formatting")
        dates_form = QFormLayout(self._dates_group)
        self._date_combo = self._preset_combo(DATE_PRESETS)
        self._timestamp_combo = self._preset_combo(TIMESTAMP_PRESETS)
        dates_form.addRow("DATE columns:", self._date_combo)
        dates_form.addRow("TIMESTAMP / TIME columns:", self._timestamp_combo)
        self._dates_hint = QLabel(
            "strftime pattern — leave empty for the default ISO output."
        )
        self._dates_hint.setWordWrap(True)
        dates_form.addRow(self._dates_hint)
        layout.addWidget(self._dates_group)

        # ── Format-specific options ──
        self._options_group = QGroupBox("Format options")
        options_layout = QVBoxLayout(self._options_group)
        self._options_stack = QStackedWidget()
        self._options_stack.addWidget(self._build_csv_page())
        self._options_stack.addWidget(self._build_parquet_page())
        self._options_stack.addWidget(self._build_json_page())
        options_layout.addWidget(self._options_stack)
        layout.addWidget(self._options_group)

        # ── Buttons ──
        actions = QHBoxLayout()
        actions.addStretch()
        cancel = QPushButton("Cancel")
        cancel.clicked.connect(self.reject)
        actions.addWidget(cancel)
        self._export_btn = QPushButton("Export")
        self._export_btn.setDefault(True)
        self._export_btn.clicked.connect(self._on_export)
        actions.addWidget(self._export_btn)
        layout.addLayout(actions)

    def _preset_combo(self, presets: list[str]) -> QComboBox:
        combo = QComboBox()
        combo.setEditable(True)
        for p in presets:
            combo.addItem(p)
        combo.setCurrentText("")
        return combo

    def _build_csv_page(self) -> QWidget:
        page = QWidget()
        form = QFormLayout(page)
        self._delimiter_combo = QComboBox()
        for label, value in DELIMITERS:
            self._delimiter_combo.addItem(label, value)
        form.addRow("Delimiter:", self._delimiter_combo)
        self._header_check = QCheckBox("Write header row")
        self._header_check.setChecked(True)
        form.addRow(self._header_check)
        self._quoting_combo = QComboBox()
        self._quoting_combo.addItem("Text values", "needed")
        self._quoting_combo.addItem("Every value", "all_valid")
        self._quoting_combo.addItem("Never (fails on embedded delimiters)", "none")
        form.addRow("Quoting:", self._quoting_combo)
        return page

    def _build_parquet_page(self) -> QWidget:
        page = QWidget()
        form = QFormLayout(page)
        self._compression_combo = QComboBox()
        for c in ("snappy", "zstd", "gzip", "brotli", "none"):
            self._compression_combo.addItem(c)
        form.addRow("Compression:", self._compression_combo)
        return page

    def _build_json_page(self) -> QWidget:
        page = QWidget()
        form = QFormLayout(page)
        self._json_lines_check = QCheckBox("Newline-delimited (JSONL) instead of a JSON array")
        form.addRow(self._json_lines_check)
        return page

    # ------------------------------------------------------------------
    # Behaviour
    # ------------------------------------------------------------------

    def _current_format(self) -> str:
        return FORMAT_LABELS[self._format_combo.currentIndex()][1]

    def _on_format_changed(self) -> None:
        fmt = self._current_format()
        self._options_stack.setCurrentIndex(self._format_combo.currentIndex())

        path = Path(self._path_edit.text().strip() or "export")
        if format_for_path(path) != fmt or not path.suffix:
            self._path_edit.setText(str(path.with_suffix(SUFFIX_BY_FORMAT[fmt])))

        # Parquet keeps native temporal types — formatting only shapes text output.
        has_temporal = any(is_temporal(t) for _, t in self._column_types)
        self._dates_group.setEnabled(fmt != "parquet" and has_temporal)
        if fmt == "parquet":
            self._dates_hint.setText(
                "Parquet stores dates natively — cast to VARCHAR to control text layout."
            )
        elif not has_temporal:
            self._dates_hint.setText("No date or timestamp columns in this result.")
        else:
            self._dates_hint.setText(
                "strftime pattern — leave empty for the default ISO output."
            )

    def _set_all_checked(self, state: Qt.CheckState) -> None:
        for row in range(self._table.rowCount()):
            self._table.item(row, 0).setCheckState(state)

    def _browse(self) -> None:
        fmt = self._current_format()
        path, _ = QFileDialog.getSaveFileName(
            self, "Export to", self._path_edit.text(), FILE_FILTERS[fmt]
        )
        if path:
            chosen = Path(path)
            if not chosen.suffix:
                chosen = chosen.with_suffix(SUFFIX_BY_FORMAT[fmt])
            self._path_edit.setText(str(chosen))
            self._format_combo.setCurrentIndex(
                [f for _, f in FORMAT_LABELS].index(format_for_path(chosen))
            )

    def _on_export(self) -> None:
        path = Path(self._path_edit.text().strip())
        if not self._path_edit.text().strip():
            QMessageBox.warning(self, "Export", "Choose a target file.")
            return
        if not path.parent.exists():
            QMessageBox.warning(self, "Export", f"Folder does not exist:\n{path.parent}")
            return
        if not any(
            self._table.item(r, 0).checkState() == Qt.CheckState.Checked
            for r in range(self._table.rowCount())
        ):
            QMessageBox.warning(self, "Export", "Select at least one column.")
            return
        if path.exists():
            reply = QMessageBox.question(
                self, "Export", f"{path.name} already exists. Overwrite?",
                QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No,
            )
            if reply != QMessageBox.StandardButton.Yes:
                return
        self.accept()

    # ------------------------------------------------------------------
    # Result
    # ------------------------------------------------------------------

    def get_options(self) -> ExportOptions:
        """Return the configured ExportOptions (call after exec() accepted)."""
        fmt = self._current_format()
        columns = [
            ColumnSpec(
                name=name,
                source_type=dtype,
                include=self._table.item(row, 0).checkState() == Qt.CheckState.Checked,
                cast_to=self._cast_combos[row].currentData(),
            )
            for row, (name, dtype) in enumerate(self._column_types)
        ]
        dates_on = self._dates_group.isEnabled()
        return ExportOptions(
            path=Path(self._path_edit.text().strip()),
            format=fmt,
            columns=columns,
            date_format=self._date_combo.currentText().strip() if dates_on else "",
            timestamp_format=self._timestamp_combo.currentText().strip() if dates_on else "",
            delimiter=self._delimiter_combo.currentData(),
            include_header=self._header_check.isChecked(),
            quoting=self._quoting_combo.currentData(),
            compression=self._compression_combo.currentText(),
            json_lines=self._json_lines_check.isChecked(),
        )
