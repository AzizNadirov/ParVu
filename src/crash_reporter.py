"""
Crash Reporter Dialog for ParVu
Displays crash information and instructions for reporting bugs
"""
import traceback
import platform
from pathlib import Path
from PyQt6.QtWidgets import (QDialog, QVBoxLayout, QHBoxLayout, QLabel,
                             QTextEdit, QPushButton, QMessageBox)
from PyQt6.QtCore import Qt
from PyQt6.QtGui import QFont
from loguru import logger

from schemas import settings
from i18n import t


class CrashReportDialog(QDialog):
    """Dialog for displaying crash information and reporting instructions"""

    def __init__(self, exception: Exception, log_file: Path, parent=None):
        super().__init__(parent)
        self.exception = exception
        self.log_file = log_file
        self.setup_ui()

    def setup_ui(self):
        """Setup the crash report dialog UI"""
        self.setWindowTitle("ParVu - Application Error")
        self.setMinimumWidth(700)
        self.setMinimumHeight(500)

        layout = QVBoxLayout(self)

        # Error icon and title
        title_layout = QHBoxLayout()
        title_label = QLabel("⚠️ An unexpected error occurred")
        title_label.setFont(QFont("Arial", 14, QFont.Weight.Bold))
        title_label.setStyleSheet("color: #d32f2f;")
        title_layout.addWidget(title_label)
        title_layout.addStretch()
        layout.addLayout(title_layout)

        # Description
        desc_label = QLabel(
            "ParVu has encountered an error. You can help improve the application "
            "by reporting this issue."
        )
        desc_label.setWordWrap(True)
        desc_label.setStyleSheet("padding: 10px 0;")
        layout.addWidget(desc_label)

        # Error details
        error_group = QLabel("Error Details:")
        error_group.setFont(QFont("Arial", 10, QFont.Weight.Bold))
        layout.addWidget(error_group)

        self.error_text = QTextEdit()
        self.error_text.setReadOnly(True)
        self.error_text.setFont(QFont("Courier New", 9))

        # Format error information
        error_info = self.format_error_info()
        self.error_text.setPlainText(error_info)

        layout.addWidget(self.error_text)

        # Reporting instructions
        report_label = QLabel("To Report This Issue:")
        report_label.setFont(QFont("Arial", 10, QFont.Weight.Bold))
        layout.addWidget(report_label)

        instructions = QLabel(
            f"1. Copy the error details above\n"
            f"2. Send an email to: <b>{settings.bug_report_email}</b>\n"
            f"3. Attach the log file: <b>{self.log_file.name}</b>\n"
            f"   (Located at: {self.log_file.parent})\n\n"
            f"Please include:\n"
            f"  • What you were doing when the error occurred\n"
            f"  • Steps to reproduce the issue\n"
            f"  • The error details and log file"
        )
        instructions.setWordWrap(True)
        instructions.setTextFormat(Qt.TextFormat.RichText)
        instructions.setStyleSheet(
            "background-color: #fff9c4; padding: 10px; border-radius: 5px;"
        )
        layout.addWidget(instructions)

        # Buttons
        button_layout = QHBoxLayout()

        self.copy_btn = QPushButton("Copy Error Details")
        self.copy_btn.clicked.connect(self.copy_error_details)
        self.copy_btn.setStyleSheet(
            "QPushButton { background-color: #2196F3; color: white; padding: 8px 16px; }"
            "QPushButton:hover { background-color: #1976D2; }"
        )
        button_layout.addWidget(self.copy_btn)

        self.open_logs_btn = QPushButton("Open Logs Folder")
        self.open_logs_btn.clicked.connect(self.open_logs_folder)
        self.open_logs_btn.setStyleSheet(
            "QPushButton { background-color: #FF9800; color: white; padding: 8px 16px; }"
            "QPushButton:hover { background-color: #F57C00; }"
        )
        button_layout.addWidget(self.open_logs_btn)

        button_layout.addStretch()

        self.close_btn = QPushButton("Close")
        self.close_btn.clicked.connect(self.accept)
        self.close_btn.setStyleSheet(
            "QPushButton { padding: 8px 16px; }"
        )
        button_layout.addWidget(self.close_btn)

        layout.addLayout(button_layout)

    def format_error_info(self) -> str:
        """Format error information for display"""
        error_type = type(self.exception).__name__
        error_message = str(self.exception)

        # Get traceback
        tb = traceback.format_exception(type(self.exception), self.exception,
                                       self.exception.__traceback__)
        traceback_str = ''.join(tb)

        # System information
        system_info = f"""
ParVu Error Report
{'=' * 70}

ERROR TYPE: {error_type}
ERROR MESSAGE: {error_message}

SYSTEM INFORMATION:
  • OS: {platform.system()} {platform.release()}
  • Platform: {platform.platform()}
  • Python Version: {platform.python_version()}
  • Log File: {self.log_file}

TRACEBACK:
{traceback_str}

{'=' * 70}
End of Error Report
"""
        return system_info.strip()

    def copy_error_details(self):
        """Copy error details to clipboard"""
        from PyQt6.QtWidgets import QApplication
        clipboard = QApplication.clipboard()
        clipboard.setText(self.error_text.toPlainText())
        QMessageBox.information(
            self,
            "Copied",
            "Error details have been copied to clipboard.\n\n"
            f"You can now paste them into an email to:\n{settings.bug_report_email}"
        )

    def open_logs_folder(self):
        """Open the logs folder in file explorer"""
        import subprocess
        import sys

        logs_path = self.log_file.parent

        try:
            if sys.platform == 'win32':
                subprocess.run(['explorer', str(logs_path)])
            elif sys.platform == 'darwin':
                subprocess.run(['open', str(logs_path)])
            else:  # linux
                subprocess.run(['xdg-open', str(logs_path)])
        except Exception as e:
            QMessageBox.warning(
                self,
                "Error",
                f"Could not open logs folder:\n{e}\n\n"
                f"Please navigate to:\n{logs_path}"
            )


def show_crash_report(exception: Exception, log_file: Path):
    """Show crash report dialog"""
    logger.error(f"Application crash: {exception}")
    logger.error(f"Traceback: {traceback.format_exc()}")

    if settings.enable_crash_reporting:
        dialog = CrashReportDialog(exception, log_file)
        dialog.exec()
