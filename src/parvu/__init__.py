"""
ParVu - Parquet/CSV/JSON File Viewer

A desktop GUI application for viewing and querying large tabular data files
without loading them entirely into memory.

Architecture:
    - core: Domain logic (query engine, pagination, file adapters)
    - config: Application configuration and state
    - infrastructure: External concerns (i18n, themes, logging)
    - services: Application services and DI container
    - presentation: PyQt6 UI layer
    - plugins: Extensibility system
    - utils: Shared utilities

Version: 0.4.0
"""

from pathlib import Path

__version__ = "0.4.0"
__app_name__ = "ParVu"

PACKAGE_ROOT = Path(__file__).parent.resolve()
RESOURCES_DIR = PACKAGE_ROOT / "resources"
