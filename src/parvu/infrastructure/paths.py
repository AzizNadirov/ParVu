"""
Path utilities for ParVu.

Handles XDG directories, resource discovery, and cross-platform paths.
"""
from __future__ import annotations

import os
import sys
from pathlib import Path

from parvu import PACKAGE_ROOT, RESOURCES_DIR


def get_user_data_dir() -> Path:
    """Get the user data directory (XDG_DATA_HOME or ~/.local/share/ParVu)."""
    xdg = os.environ.get("XDG_DATA_HOME")
    if xdg:
        return Path(xdg) / "ParVu"
    return Path.home() / ".local" / "share" / "ParVu"


def get_user_config_dir() -> Path:
    """Get the user config directory (XDG_CONFIG_HOME or ~/.config/ParVu)."""
    xdg = os.environ.get("XDG_CONFIG_HOME")
    if xdg:
        return Path(xdg) / "ParVu"
    return Path.home() / ".config" / "ParVu"


def get_user_cache_dir() -> Path:
    """Get the user cache directory (XDG_CACHE_HOME or ~/.cache/ParVu)."""
    xdg = os.environ.get("XDG_CACHE_HOME")
    if xdg:
        return Path(xdg) / "ParVu"
    return Path.home() / ".cache" / "ParVu"


def get_legacy_app_dir() -> Path:
    """Legacy app directory for backward compatibility (~/.ParVu)."""
    return Path.home() / ".ParVu"


def resolve_resource_path(relative_path: str) -> Path:
    """Resolve a resource path relative to the package resources directory."""
    return RESOURCES_DIR / relative_path


def resolve_static_path() -> Path:
    """Get the static resources directory."""
    return RESOURCES_DIR / "static"


def resolve_settings_path() -> Path:
    """Get the bundled settings directory."""
    return RESOURCES_DIR / "settings"


def resolve_history_path() -> Path:
    """Get the bundled history directory."""
    return RESOURCES_DIR / "history"


def get_log_dir() -> Path:
    """Get the directory for log files."""
    log_dir = get_legacy_app_dir() / "logs"
    log_dir.mkdir(parents=True, exist_ok=True)
    return log_dir


def get_themes_dir() -> Path:
    """Get the directory for custom themes."""
    themes_dir = get_legacy_app_dir() / "themes"
    themes_dir.mkdir(parents=True, exist_ok=True)
    return themes_dir
