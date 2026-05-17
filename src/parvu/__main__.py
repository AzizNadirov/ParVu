"""
ParVu entry point.

Usage: python -m parvu [file_path]
"""
from __future__ import annotations

import sys

from PyQt6.QtWidgets import QApplication
from PyQt6.QtGui import QIcon

from parvu.services.container import ServiceContainer
from parvu.services.window_service import WindowService
from parvu.services.session_service import SessionService
from parvu.infrastructure.paths import resolve_static_path


def main() -> int:
    """Main application entry point."""
    container = ServiceContainer()

    app = QApplication(sys.argv)
    app.setApplicationName("ParVu")
    app.setOrganizationName("ParVu")

    # Set icon if available
    icon_path = resolve_static_path() / "logo.png"
    if icon_path.exists():
        app.setWindowIcon(QIcon(str(icon_path)))

    session = SessionService(container)
    session.install_crash_handler()

    window_service = WindowService(container)

    # Check for file path in command line args
    file_path = sys.argv[1] if len(sys.argv) > 1 else None

    window = window_service.create_window(file_path)
    window.set_window_service(window_service)

    exit_code = app.exec()
    session.shutdown()
    return exit_code


if __name__ == "__main__":
    sys.exit(main())
