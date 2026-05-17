"""
Shared type aliases and utilities for ParVu.
"""
from __future__ import annotations

from typing import TypeVar, Callable, Any

# Generic type variable
T = TypeVar("T")

# Common callback types
ErrorHandler = Callable[[Exception], None]
ProgressCallback = Callable[[int, int], None]  # current, total
VoidCallback = Callable[[], None]

# Event handler type
EventHandler = Callable[[Any], None]

# Theme-related types
ColorHex = str
CSSStylesheet = str
