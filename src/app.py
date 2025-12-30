#!/usr/bin/env python3
"""
ParVu - Parquet/CSV/JSON File Viewer
Entry point for the application
"""
import sys
import uuid
from datetime import datetime
from PyQt6.QtWidgets import QApplication
from PyQt6.QtGui import QIcon
from loguru import logger

from main_window import MainWindow
from schemas import settings
from i18n import get_i18n
from crash_reporter import show_crash_report


# Global session log file for crash reporting
_session_log_file = None


class WindowManager:
    """Manages multiple application windows"""

    def __init__(self, app):
        self.app = app
        self.windows = []

    def create_window(self, file_path: str = None):
        """Create and show a new window"""
        window = MainWindow(file_path, self)
        window.show()
        self.windows.append(window)
        logger.info(f"Created new window. Total windows: {len(self.windows)}")
        return window

    def remove_window(self, window):
        """Remove window from tracking"""
        if window in self.windows:
            self.windows.remove(window)
            logger.info(f"Window closed. Remaining windows: {len(self.windows)}")


def excepthook(exc_type, exc_value, exc_traceback):
    """Global exception handler for unhandled exceptions"""
    global _session_log_file

    # Log the exception
    logger.error("Uncaught exception", exc_info=(exc_type, exc_value, exc_traceback))

    # Show crash report dialog if crash reporting is enabled
    if _session_log_file and settings.enable_crash_reporting:
        show_crash_report(exc_value, _session_log_file)


def main():
    """Main application entry point"""
    global _session_log_file

    # Generate unique session ID
    session_id = datetime.now().strftime("%Y%m%d_%H%M%S") + "_" + str(uuid.uuid4())[:8]

    # Create logs directory if it doesn't exist
    settings.user_logs_dir.mkdir(parents=True, exist_ok=True)

    # Setup session-based logging
    _session_log_file = settings.user_logs_dir / f"parvu_session_{session_id}.log"
    logger.add(
        _session_log_file,
        rotation="10 MB",
        retention="30 days",
        level="DEBUG",
        backtrace=True,
        diagnose=True,
        format="{time:YYYY-MM-DD HH:mm:ss.SSS} | {level: <8} | {name}:{function}:{line} - {message}"
    )

    logger.info(f"Starting ParVu application - Session ID: {session_id}")
    logger.info(f"Session log file: {_session_log_file}")

    # Install global exception handler
    sys.excepthook = excepthook

    # Initialize i18n with saved language
    i18n = get_i18n()
    i18n.set_locale(settings.current_language)
    logger.info(f"Language set to: {settings.current_language}")

    # Create Qt application
    app = QApplication(sys.argv)
    app.setApplicationName("ParVu")
    app.setOrganizationName("ParVu")

    # Set application icon if available
    icon_path = settings.static_dir / "logo.jpg"
    if icon_path.exists():
        app.setWindowIcon(QIcon(str(icon_path)))

    try:
        # Check for file path in command line args
        file_path = sys.argv[1] if len(sys.argv) > 1 else None

        # Create window manager and initial window
        window_manager = WindowManager(app)
        window_manager.create_window(file_path)

        # Start event loop
        exit_code = app.exec()

        logger.info(f"ParVu application exiting with code {exit_code}")
        sys.exit(exit_code)

    except Exception as e:
        # Catch any exceptions during application runtime
        logger.exception(f"Fatal error in main application: {e}")
        if settings.enable_crash_reporting:
            show_crash_report(e, _session_log_file)
        sys.exit(1)


if __name__ == "__main__":
    main()
