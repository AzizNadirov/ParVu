"""
Incremental full-table Search dialog.

Searches across every row (every page) of the active engine's current query
result. Next/Previous step through matches one at a time; previously visited
matches are cached so re-navigation is instant.
"""
from __future__ import annotations

from PyQt6.QtCore import Qt, pyqtSignal
from PyQt6.QtGui import QKeySequence, QShortcut
from PyQt6.QtWidgets import (
    QApplication,
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
from loguru import logger

from typing import Callable

from parvu.core.query_engine import QueryEngine


class SearchDialog(QDialog):
    """Modeless search with Next/Prev navigation across all pages.

    Emits ``jump_requested(absolute_row, column_name)`` whenever the displayed
    match changes; the owning window paginates to the row and selects the
    cell.
    """

    jump_requested = pyqtSignal(int, str)

    # Match tuple: (absolute_row, column_index, column_name, value)
    _Match = tuple[int, int, str, str]

    def __init__(
        self,
        translator: Callable[..., str],
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent)
        self._t = translator
        self.setWindowTitle(self._t("search.title"))
        self.setMinimumSize(540, 240)
        self.setModal(False)

        self._engine: QueryEngine | None = None

        # Cached matches in table order; pointer = position of currently shown one
        self._cache: list[SearchDialog._Match] = []
        self._pointer: int = -1
        self._reached_end: bool = False
        self._reached_start: bool = False
        # Snapshot of params last used to populate the cache (for invalidation)
        self._cache_params: tuple | None = None

        # Initial anchor (table cursor) used when the cache is empty
        self._initial_anchor_row: int | None = None
        self._initial_anchor_col_idx: int | None = None

        self._build_ui()

        esc = QShortcut(QKeySequence("Esc"), self)
        esc.activated.connect(self.close)

    # ── UI ──────────────────────────────────────────────────────────────
    def _build_ui(self) -> None:
        layout = QVBoxLayout(self)
        layout.setSpacing(8)

        row1 = QHBoxLayout()
        row1.addWidget(QLabel(self._t("search.label.find")))
        self._input = QLineEdit()
        self._input.setPlaceholderText(self._t("search.placeholder"))
        self._input.returnPressed.connect(self._on_next)
        self._input.textChanged.connect(self._on_param_changed)
        row1.addWidget(self._input, stretch=1)
        layout.addLayout(row1)

        row2 = QHBoxLayout()
        row2.addWidget(QLabel(self._t("search.label.column")))
        self._column_combo = QComboBox()
        self._column_combo.addItem(self._t("search.all_columns"))
        self._column_combo.currentTextChanged.connect(self._on_param_changed)
        row2.addWidget(self._column_combo, stretch=1)
        layout.addLayout(row2)

        row3 = QHBoxLayout()
        self._case = QCheckBox(self._t("search.option.case"))
        self._whole = QCheckBox(self._t("search.option.whole"))
        self._regex = QCheckBox(self._t("search.option.regex"))
        for w in (self._case, self._whole, self._regex):
            w.stateChanged.connect(self._on_param_changed)
            row3.addWidget(w)
        row3.addStretch(1)
        layout.addLayout(row3)

        row4 = QHBoxLayout()
        self._prev_btn = QPushButton(self._t("search.btn.prev"))
        self._prev_btn.setAutoDefault(False)
        self._prev_btn.setDefault(False)
        self._prev_btn.setShortcut(QKeySequence("Shift+Return"))
        self._prev_btn.clicked.connect(self._on_prev)
        row4.addWidget(self._prev_btn)

        self._next_btn = QPushButton(self._t("search.btn.next"))
        self._next_btn.setAutoDefault(False)
        self._next_btn.setDefault(False)
        self._next_btn.clicked.connect(self._on_next)
        row4.addWidget(self._next_btn)

        row4.addStretch(1)
        close_btn = QPushButton(self._t("btn.close"))
        close_btn.clicked.connect(self.close)
        row4.addWidget(close_btn)
        layout.addLayout(row4)

        self._status = QLabel(self._t("search.status.initial"))
        self._status.setStyleSheet("color: gray;")
        self._status.setWordWrap(True)
        layout.addWidget(self._status)

        self._match_info = QLabel("")
        self._match_info.setWordWrap(True)
        layout.addWidget(self._match_info)

        layout.addStretch(1)

    # ── Public API ──────────────────────────────────────────────────────
    def set_engine(self, engine: QueryEngine | None) -> None:
        """Bind the dialog to a new engine. Column list refreshes; cache clears."""
        self._engine = engine
        cols = engine.get_columns() if engine is not None else []
        logger.info(
            f"[search] set_engine: engine={engine is not None}, "
            f"total_rows={engine.total_rows if engine else 0}, "
            f"columns={cols}"
        )
        self._column_combo.blockSignals(True)
        self._column_combo.clear()
        self._column_combo.addItem(self._t("search.all_columns"))
        for col in cols:
            self._column_combo.addItem(col)
        self._column_combo.blockSignals(False)
        self._invalidate_cache()
        self._status.setText(self._t("search.status.initial"))

    def set_initial_anchor(self, row: int | None, col_idx: int | None) -> None:
        """Cursor coordinate to use as anchor for the very first Next/Prev."""
        logger.debug(f"[search] set_initial_anchor: row={row}, col_idx={col_idx}")
        self._initial_anchor_row = row
        self._initial_anchor_col_idx = col_idx

    def focus_input(self, preset: str = "") -> None:
        if preset:
            self._input.setText(preset)
        self._input.setFocus()
        self._input.selectAll()

    # ── Cache management ────────────────────────────────────────────────
    def _current_params(self) -> tuple:
        col = self._column_combo.currentText()
        all_columns_label = self._t("search.all_columns")
        return (
            self._input.text(),
            col if col != all_columns_label else None,
            self._case.isChecked(),
            self._whole.isChecked(),
            self._regex.isChecked(),
        )

    def _invalidate_cache(self, *_args) -> None:
        self._cache = []
        self._pointer = -1
        self._reached_end = False
        self._reached_start = False
        self._cache_params = None
        self._match_info.setText("")

    def _on_param_changed(self, *_args) -> None:
        """Any search-param change clears the cache. Search runs only on Enter/Next."""
        text = self._input.text()
        logger.debug(
            f"[search] param changed: text={text!r}, "
            f"column={self._column_combo.currentText()!r}, "
            f"case={self._case.isChecked()}, whole={self._whole.isChecked()}, "
            f"regex={self._regex.isChecked()}"
        )
        self._invalidate_cache()
        if text.strip():
            self._status.setText(self._t("search.status.pending"))
        else:
            self._status.setText(self._t("search.status.empty"))

    def _ensure_params_match(self) -> None:
        cur = self._current_params()
        if self._cache_params != cur:
            logger.debug(
                f"[search] params changed; invalidating cache. "
                f"old={self._cache_params}, new={cur}"
            )
            self._invalidate_cache()
            self._cache_params = cur

    # ── Navigation ──────────────────────────────────────────────────────
    def _on_next(self) -> None:
        logger.info(
            f"[search] Next clicked: text={self._input.text()!r}, "
            f"cache_size={len(self._cache)}, pointer={self._pointer}, "
            f"reached_end={self._reached_end}, "
            f"initial_anchor=({self._initial_anchor_row}, {self._initial_anchor_col_idx})"
        )
        if not self._validate_ready():
            logger.debug("[search] _on_next: validation failed, returning")
            return
        self._ensure_params_match()

        # Cache hit: just advance
        if self._pointer < len(self._cache) - 1:
            self._pointer += 1
            logger.debug(f"[search] cache hit forward; pointer -> {self._pointer}")
            self._show_current()
            return

        if self._reached_end:
            logger.debug("[search] already at reached_end")
            self._status.setText(
                self._t(
                    "search.status.last",
                    current=self._pointer + 1,
                    total=len(self._cache),
                )
            )
            return

        anchor_row, anchor_col = self._anchor_for_query("next")
        logger.debug(
            f"[search] querying next with anchor=({anchor_row}, {anchor_col})"
        )
        match = self._run_query("next", anchor_row, anchor_col)
        logger.info(f"[search] next query result: {match}")
        if match is None:
            self._reached_end = True
            if self._cache:
                self._status.setText(
                    self._t(
                        "search.status.last",
                        current=self._pointer + 1,
                        total=len(self._cache),
                    )
                )
            else:
                self._status.setText(self._t("search.status.no_matches"))
                self._match_info.setText("")
            return
        self._cache.append(match)
        self._pointer = len(self._cache) - 1
        self._show_current()

    def _on_prev(self) -> None:
        logger.info(
            f"[search] Prev clicked: text={self._input.text()!r}, "
            f"cache_size={len(self._cache)}, pointer={self._pointer}, "
            f"reached_start={self._reached_start}"
        )
        if not self._validate_ready():
            return
        self._ensure_params_match()

        if self._pointer > 0:
            self._pointer -= 1
            logger.debug(f"[search] cache hit backward; pointer -> {self._pointer}")
            self._show_current()
            return

        if self._reached_start:
            self._status.setText(
                self._t("search.status.first", total=len(self._cache))
            )
            return

        anchor_row, anchor_col = self._anchor_for_query("prev")
        logger.debug(f"[search] querying prev with anchor=({anchor_row}, {anchor_col})")
        match = self._run_query("prev", anchor_row, anchor_col)
        logger.info(f"[search] prev query result: {match}")
        if match is None:
            self._reached_start = True
            if self._cache:
                self._status.setText(
                    self._t("search.status.first", total=len(self._cache))
                )
            else:
                self._status.setText(self._t("search.status.no_matches"))
                self._match_info.setText("")
            return
        self._cache.insert(0, match)
        self._pointer = 0
        self._show_current()

    def _validate_ready(self) -> bool:
        if self._engine is None:
            self._status.setText(self._t("search.status.no_table"))
            return False
        if not self._input.text():
            self._status.setText(self._t("search.status.empty"))
            return False
        return True

    def _anchor_for_query(self, direction: str) -> tuple[int | None, int | None]:
        """Return (row, col_idx) anchor for a fresh forward/backward SQL query."""
        if self._cache:
            if direction == "next":
                last = self._cache[-1]
                return last[0], last[1]
            first = self._cache[0]
            return first[0], first[1]
        # Cache empty → use the table cursor position set by the caller
        return self._initial_anchor_row, self._initial_anchor_col_idx

    def _run_query(
        self, direction: str, anchor_row: int | None, anchor_col_idx: int | None
    ) -> _Match | None:
        assert self._engine is not None
        self._status.setText(self._t("search.status.searching"))
        self._prev_btn.setEnabled(False)
        self._next_btn.setEnabled(False)
        QApplication.processEvents()
        try:
            query_text, column, case_s, whole, regex = self._current_params()
            logger.debug(
                f"[search] engine.find_next_match("
                f"query={query_text!r}, column={column!r}, "
                f"after_row={anchor_row}, after_col_index={anchor_col_idx}, "
                f"direction={direction!r}, case={case_s}, whole={whole}, regex={regex})"
            )
            result = self._engine.find_next_match(
                query=query_text,
                column=column,
                after_row=anchor_row,
                after_col_index=anchor_col_idx,
                direction=direction,
                case_sensitive=case_s,
                whole_cell=whole,
                use_regex=regex,
            )
            logger.debug(f"[search] engine returned: {result}")
            return result
        except Exception as e:
            logger.exception(f"[search] engine call failed: {e}")
            self._status.setText(self._t("search.status.failed", error=str(e)))
            return None
        finally:
            self._prev_btn.setEnabled(True)
            self._next_btn.setEnabled(True)

    # ── Display ─────────────────────────────────────────────────────────
    def _show_current(self) -> None:
        if not (0 <= self._pointer < len(self._cache)):
            logger.debug(
                f"[search] _show_current: pointer={self._pointer} out of range "
                f"(cache size={len(self._cache)})"
            )
            return
        row, _col_idx, col_name, value = self._cache[self._pointer]

        total = len(self._cache)
        if self._pointer == total - 1 and not self._reached_end:
            status_key = "search.status.match_more"
        else:
            status_key = "search.status.match"
        self._status.setText(
            self._t(status_key, current=self._pointer + 1, total=total)
        )

        preview = value if len(value) <= 200 else value[:197] + "..."
        self._match_info.setText(
            self._t(
                "search.match_info",
                row=row + 1,
                column=col_name,
                value=repr(preview),
            )
        )

        logger.info(
            f"[search] showing match {self._pointer + 1}/{total}: "
            f"row={row}, col='{col_name}', value={preview!r}"
        )
        self.jump_requested.emit(row, col_name)

    # ── Key handling ────────────────────────────────────────────────────
    def keyPressEvent(self, event) -> None:
        """Make Enter == Next, Shift+Enter == Prev anywhere in the dialog."""
        key = event.key()
        if key in (Qt.Key.Key_Return, Qt.Key.Key_Enter):
            if event.modifiers() & Qt.KeyboardModifier.ShiftModifier:
                logger.debug("[search] Shift+Enter -> Prev")
                self._on_prev()
            else:
                logger.debug("[search] Enter -> Next")
                self._on_next()
            event.accept()
            return
        super().keyPressEvent(event)
