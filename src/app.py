#!/usr/bin/env python3
"""
ParVu - Parquet/CSV/JSON File Viewer
Entry point for the application
"""
import sys
from PyQt6.QtWidgets import QApplication
from PyQt6.QtGui import QIcon
from loguru import logger

from main_window import MainWindow
from schemas import settings
from i18n import get_i18n


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


def main():
    """Main application entry point"""
    # Setup logging
    logger.add(
        settings.user_logs_dir / "parvu_{time}.log",
        rotation="10 MB",
        retention="7 days",
        level="INFO"
    )

    logger.info("Starting ParVu application")

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

    # Check for file path in command line args
    file_path = sys.argv[1] if len(sys.argv) > 1 else None

    # Create window manager and initial window
    window_manager = WindowManager(app)
    window_manager.create_window(file_path)

    # Start event loop
    exit_code = app.exec()

    logger.info(f"ParVu application exiting with code {exit_code}")
    sys.exit(exit_code)


if __name__ == "__main__":
    main()
