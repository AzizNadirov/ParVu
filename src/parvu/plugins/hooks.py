"""
Hook definitions for ParVu plugin system.

Plugins can register callbacks for these hooks to extend functionality.
"""
from __future__ import annotations

from enum import Enum, auto
from typing import Callable, Any


class HookType(Enum):
    """Available hook types."""

    PRE_FILE_LOAD = auto()
    POST_FILE_LOAD = auto()

    PRE_QUERY = auto()
    POST_QUERY = auto()

    PRE_EXPORT = auto()
    POST_EXPORT = auto()

    PRE_PAGE_LOAD = auto()
    POST_PAGE_LOAD = auto()

    THEME_APPLY = auto()
    SETTINGS_CHANGE = auto()

    APP_STARTUP = auto()
    APP_SHUTDOWN = auto()


class HookRegistry:
    """Registry for plugin hooks."""

    def __init__(self):
        self._hooks: dict[HookType, list[Callable]] = {
            hook: [] for hook in HookType
        }

    def register(self, hook_type: HookType, callback: Callable) -> None:
        """Register a callback for a hook type."""
        self._hooks[hook_type].append(callback)

    def unregister(self, hook_type: HookType, callback: Callable) -> None:
        """Unregister a callback."""
        if callback in self._hooks[hook_type]:
            self._hooks[hook_type].remove(callback)

    def emit(self, hook_type: HookType, *args, **kwargs) -> list[Any]:
        """Emit a hook, calling all registered callbacks. Returns results."""
        results = []
        for callback in self._hooks[hook_type]:
            try:
                result = callback(*args, **kwargs)
                results.append(result)
            except Exception as e:
                from loguru import logger
                logger.error(f"Hook error in {hook_type.name}: {e}")
        return results

    def emit_first(self, hook_type: HookType, *args, **kwargs) -> Any | None:
        """Emit a hook and return the first non-None result."""
        for callback in self._hooks[hook_type]:
            try:
                result = callback(*args, **kwargs)
                if result is not None:
                    return result
            except Exception as e:
                from loguru import logger
                logger.error(f"Hook error in {hook_type.name}: {e}")
        return None
