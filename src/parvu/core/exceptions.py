"""
Domain exceptions for ParVu.

All custom exceptions inherit from ParVuError for easy catching.
"""
from __future__ import annotations


class ParVuError(Exception):
    """Base exception for all ParVu errors."""
    pass


class QueryError(ParVuError):
    """Raised when a SQL query fails."""
    pass


class BadQueryError(ParVuError):
    """Raised when a query has semantic errors (e.g., invalid table name)."""

    def __init__(self, message: str, suggested_fix: str | None = None):
        super().__init__(message)
        self.suggested_fix = suggested_fix


class FileFormatError(ParVuError):
    """Raised when a file format is unsupported or invalid."""
    pass


class FileNotFoundError(ParVuError):
    """Raised when a data file does not exist."""
    pass


class PaginationError(ParVuError):
    """Raised when pagination parameters are invalid."""
    pass


class ThemeError(ParVuError):
    """Raised when theme operations fail."""
    pass


class SettingsError(ParVuError):
    """Raised when settings operations fail."""
    pass


class PluginError(ParVuError):
    """Raised when plugin operations fail."""
    pass


class ExportError(ParVuError):
    """Raised when data export fails."""
    pass


class UniqueValuesError(ParVuError):
    """Raised when unique value calculation fails."""
    pass
