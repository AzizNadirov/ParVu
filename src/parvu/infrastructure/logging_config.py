"""
Logging configuration for ParVu.

Sets up loguru with session-based log files.
"""
from __future__ import annotations

from pathlib import Path

from loguru import logger


def setup_logging(log_file: Path) -> None:
    """Configure loguru with file and console handlers."""
    logger.add(
        log_file,
        rotation="10 MB",
        retention="30 days",
        level="DEBUG",
        backtrace=True,
        diagnose=True,
        format="{time:YYYY-MM-DD HH:mm:ss.SSS} | {level: <8} | {name}:{function}:{line} - {message}",
    )
