"""
Crash Reporter Dialog for ParVu.
"""
from __future__ import annotations

import traceback
import platform
import subprocess
import sys
from pathlib import Path

from PyQt6.QtWidgets import (
    QDialog, QVBoxLayout, QHBoxLayout, QLabel, QTextEdit,
    QPushButton, QMessageBox,
)
from PyQt6.QtCore import Qt
from PyQt6.QtGui import QFont

from parvu.config.settings import Settings


class CrashReportDialog(QDialog):
    """Dialog for displaying crash information."""

    def __init__(self, exception: Exception, log_file: Path, settings: Settings, parent=None):
        super().__init__(parent)
        self._exception = exception
        self._log_file = log_file
        self._settings = settings
        self._setup_ui()

    def _setup_ui(self) -> None:
        self.setWindowTitle("ParVu - Application Error")
        self.setMinimumWidth(700)
        self.setMinimumHeight(500)

        layout = QVBoxLayout(self)

        title_layout = QHBoxLayout()
        title = QLabel("⚠️ An unexpected error occurred")
        title.setFont(QFont("Arial", 14, QFont.Weight.Bold))
        title.setStyleSheet("color: #d32f2f;")
        title_layout.addWidget(title)
        title_layout.addStretch()
        layout.addLayout(title_layout)

        desc = QLabel(
            "ParVu has encountered an error. You can help improve the application "
            "by reporting this issue."
        )
        desc.setWordWrap(True)
        desc.setStyleSheet("padding: 10px 0;")
        layout.addWidget(desc)

        group = QLabel("Error Details:")
        group.setFont(QFont("Arial", 10, QFont.Weight.Bold))
        layout.addWidget(group)

        self._error_text = QTextEdit()
        self._error_text.setReadOnly(True)
        self._error_text.setFont(QFont("Courier New", 9))
        self._error_text.setPlainText(self._format_error())
        layout.addWidget(self._error_text)

        report = QLabel("To Report This Issue:")
        report.setFont(QFont("Arial", 10, QFont.Weight.Bold))
        layout.addWidget(report)

        instructions = QLabel(
            f"<b>📧 Please send this report to:</b> <a href='mailto:{self._settings.bug_report_email}'>{self._settings.bug_report_email}</a><br><br>"
            f"1. Copy the error details above<br>"
            f"2. Send an email to the address above<br>"
            f"3. Attach the log file: <b>{self._log_file.name}</b><br>"
            f"   (Located at: {self._log_file.parent})<br><br>"
            f"Please include:<br>"
            f"  • What you were doing when the error occurred<br>"
            f"  • Steps to reproduce the issue<br>"
            f"  • The error details and log file"
        )
        instructions.setWordWrap(True)
        instructions.setTextFormat(Qt.TextFormat.RichText)
        instructions.setStyleSheet(
            "background-color: #fff9c4; padding: 10px; border-radius: 5px;"
        )
        layout.addWidget(instructions)

        btn_layout = QHBoxLayout()
        copy_btn = QPushButton("📋 Copy Error Details")
        copy_btn.clicked.connect(self._copy_error)
        btn_layout.addWidget(copy_btn)

        email_btn = QPushButton("📧 Open Email Client")
        email_btn.clicked.connect(self._open_email)
        btn_layout.addWidget(email_btn)

        open_btn = QPushButton("📁 Open Logs Folder")
        open_btn.clicked.connect(self._open_logs)
        btn_layout.addWidget(open_btn)
        btn_layout.addStretch()

        close_btn = QPushButton("Close")
        close_btn.clicked.connect(self.accept)
        btn_layout.addWidget(close_btn)
        layout.addLayout(btn_layout)

    def _format_error(self) -> str:
        tb = traceback.format_exception(
            type(self._exception), self._exception, self._exception.__traceback__
        )
        return f"""ParVu Error Report
{'=' * 70}

ERROR TYPE: {type(self._exception).__name__}
ERROR MESSAGE: {self._exception}

SYSTEM INFORMATION:
  • OS: {platform.system()} {platform.release()}
  • Platform: {platform.platform()}
  • Python Version: {platform.python_version()}
  • Log File: {self._log_file}

TRACEBACK:
{''.join(tb)}

{'=' * 70}
End of Error Report
""".strip()

    def _copy_error(self) -> None:
        from PyQt6.QtWidgets import QApplication
        QApplication.clipboard().setText(self._error_text.toPlainText())
        QMessageBox.information(
            self, "Copied",
            f"Error details copied to clipboard.\n\n"
            f"Please send to: {self._settings.bug_report_email}"
        )

    def _open_email(self) -> None:
        """Open default email client with pre-filled message."""
        import urllib.parse
        subject = urllib.parse.quote("ParVu Crash Report")
        body = urllib.parse.quote(
            f"ParVu crashed.\n\n"
            f"Log file: {self._log_file}\n\n"
            f"Error details:\n{self._error_text.toPlainText()[:2000]}"
        )
        import subprocess
        import sys
        mailto = f"mailto:{self._settings.bug_report_email}?subject={subject}&body={body}"
        try:
            if sys.platform == "win32":
                subprocess.run(["start", mailto], shell=True)
            elif sys.platform == "darwin":
                subprocess.run(["open", mailto])
            else:
                subprocess.run(["xdg-open", mailto])
        except Exception as e:
            QMessageBox.warning(
                self, "Error",
                f"Could not open email client:\n{e}\n\n"
                f"Please manually send an email to:\n{self._settings.bug_report_email}"
            )

    def _open_logs(self) -> None:
        logs_path = self._log_file.parent
        try:
            if sys.platform == "win32":
                subprocess.run(["explorer", str(logs_path)])
            elif sys.platform == "darwin":
                subprocess.run(["open", str(logs_path)])
            else:
                subprocess.run(["xdg-open", str(logs_path)])
        except Exception as e:
            QMessageBox.warning(
                self, "Error",
                f"Could not open logs folder:\n{e}\n\nNavigate to:\n{logs_path}"
            )


def show_crash_report(exception: Exception, log_file: Path, settings: Settings) -> None:
    """Show crash report dialog."""
    dialog = CrashReportDialog(exception, log_file, settings)
    dialog.exec()
