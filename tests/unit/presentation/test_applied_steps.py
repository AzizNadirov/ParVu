"""
Unit tests for AppliedStepsPanel.
"""
from __future__ import annotations

import pytest

from parvu.presentation.widgets.applied_steps import AppliedStepsPanel


class TestAppliedStepsPanel:
    """Test the applied steps list and undo button."""

    def test_set_steps_populates_list(self, qtbot) -> None:
        panel = AppliedStepsPanel()
        qtbot.addWidget(panel)
        panel.set_steps(["Sort 'price' ascending", "Add column 'total'"])
        assert panel._list.count() == 2

    def test_set_steps_enables_undo_button(self, qtbot) -> None:
        panel = AppliedStepsPanel()
        qtbot.addWidget(panel)
        assert not panel._undo_btn.isEnabled()
        panel.set_steps(["Add column 'x'"])
        assert panel._undo_btn.isEnabled()

    def test_clear_disables_undo_button(self, qtbot) -> None:
        panel = AppliedStepsPanel()
        qtbot.addWidget(panel)
        panel.set_steps(["Add column 'x'"])
        assert panel._undo_btn.isEnabled()
        panel.clear()
        assert not panel._undo_btn.isEnabled()

    def test_undo_requested_signal(self, qtbot) -> None:
        panel = AppliedStepsPanel()
        qtbot.addWidget(panel)
        panel.set_steps(["Add column 'x'"])

        with qtbot.waitSignal(panel.undo_requested, timeout=1000):
            panel._undo_btn.click()
