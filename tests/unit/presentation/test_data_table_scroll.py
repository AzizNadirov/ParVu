"""Shift+wheel on the data grid scrolls horizontally."""
from PyQt6.QtCore import QPoint, QPointF, Qt
from PyQt6.QtGui import QWheelEvent
from PyQt6.QtWidgets import QApplication

from parvu.presentation.widgets.data_table import DataTableView


def _send_wheel(table, angle: QPoint, mods) -> None:
    table.verticalScrollBar().setValue(0)
    table.horizontalScrollBar().setValue(0)
    vp = table.viewport()
    pos = QPointF(20, 20)
    event = QWheelEvent(
        pos, QPointF(vp.mapToGlobal(pos)), QPoint(0, 0), angle,
        Qt.MouseButton.NoButton, mods, Qt.ScrollPhase.NoScrollPhase, False,
    )
    QApplication.sendEvent(vp, event)


def _table(qtbot):
    table = DataTableView()
    qtbot.addWidget(table)
    table.setRowCount(200)
    table.setColumnCount(60)
    table.resize(400, 300)
    table.show()
    qtbot.waitExposed(table)
    return table


def test_plain_wheel_scrolls_vertically(qtbot):
    table = _table(qtbot)
    _send_wheel(table, QPoint(0, -120), Qt.KeyboardModifier.NoModifier)
    assert table.verticalScrollBar().value() > 0
    assert table.horizontalScrollBar().value() == 0


def test_shift_wheel_scrolls_horizontally(qtbot):
    table = _table(qtbot)
    _send_wheel(table, QPoint(0, -120), Qt.KeyboardModifier.ShiftModifier)
    assert table.verticalScrollBar().value() == 0
    assert table.horizontalScrollBar().value() > 0


def test_shift_wheel_already_horizontal(qtbot):
    """Platforms that pre-swap Shift+wheel send an x delta instead."""
    table = _table(qtbot)
    _send_wheel(table, QPoint(-120, 0), Qt.KeyboardModifier.ShiftModifier)
    assert table.verticalScrollBar().value() == 0
    assert table.horizontalScrollBar().value() > 0


def test_shift_wheel_up_scrolls_left(qtbot):
    table = _table(qtbot)
    table.horizontalScrollBar().setValue(10)
    vp = table.viewport()
    pos = QPointF(20, 20)
    QApplication.sendEvent(vp, QWheelEvent(
        pos, QPointF(vp.mapToGlobal(pos)), QPoint(0, 0), QPoint(0, 120),
        Qt.MouseButton.NoButton, Qt.KeyboardModifier.ShiftModifier,
        Qt.ScrollPhase.NoScrollPhase, False,
    ))
    assert table.horizontalScrollBar().value() < 10
