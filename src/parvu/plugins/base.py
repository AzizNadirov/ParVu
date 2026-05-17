"""
Plugin base classes for ParVu.
"""
from __future__ import annotations

from abc import ABC, abstractmethod
from dataclasses import dataclass
from typing import Any, Callable


@dataclass
class PluginContext:
    """Context passed to plugins on activation."""

    container: Any  # ServiceContainer
    settings: Any  # Settings
    event_bus: Any  # EventBus (if used)


class Plugin(ABC):
    """Base class for all ParVu plugins."""

    name: str = ""
    version: str = "1.0"
    author: str = ""
    description: str = ""

    @abstractmethod
    def activate(self, context: PluginContext) -> None:
        """Called when the plugin is activated."""
        ...

    @abstractmethod
    def deactivate(self) -> None:
        """Called when the plugin is deactivated."""
        ...

    def get_hooks(self) -> dict[str, Callable]:
        """Return hooks provided by this plugin. Override if needed."""
        return {}
