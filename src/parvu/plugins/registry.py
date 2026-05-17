"""
Plugin registry for ParVu.

Discovers, loads, and manages plugins.
"""
from __future__ import annotations

import importlib
import pkgutil
from pathlib import Path
from typing import Type

from loguru import logger

from parvu.plugins.base import Plugin, PluginContext
from parvu.plugins.hooks import HookRegistry, HookType


class PluginRegistry:
    """Manages plugin discovery and lifecycle."""

    def __init__(self):
        self._plugins: dict[str, Plugin] = {}
        self._hook_registry = HookRegistry()
        self._context: PluginContext | None = None

    @property
    def hook_registry(self) -> HookRegistry:
        return self._hook_registry

    def initialize(self, context: PluginContext) -> None:
        """Initialize the registry with application context."""
        self._context = context
        self._discover_builtin_plugins()

    def _discover_builtin_plugins(self) -> None:
        """Discover built-in plugins."""
        try:
            import parvu.plugins.builtin as builtin_pkg
            for importer, modname, ispkg in pkgutil.iter_modules(
                builtin_pkg.__path__, builtin_pkg.__name__ + "."
            ):
                try:
                    module = importlib.import_module(modname)
                    self._load_plugins_from_module(module)
                except Exception as e:
                    logger.warning(f"Failed to load built-in plugin module {modname}: {e}")
        except Exception as e:
            logger.warning(f"Failed to discover built-in plugins: {e}")

    def _load_plugins_from_module(self, module) -> None:
        """Load Plugin subclasses from a module."""
        for attr_name in dir(module):
            attr = getattr(module, attr_name)
            if (
                isinstance(attr, type)
                and issubclass(attr, Plugin)
                and attr is not Plugin
            ):
                self.register_plugin(attr)

    def register_plugin(self, plugin_class: Type[Plugin]) -> None:
        """Register and activate a plugin."""
        try:
            instance = plugin_class()
            if instance.name in self._plugins:
                logger.warning(f"Plugin '{instance.name}' already registered")
                return

            if self._context:
                instance.activate(self._context)

            # Register hooks
            for hook_name, callback in instance.get_hooks().items():
                try:
                    hook_type = HookType[hook_name]
                    self._hook_registry.register(hook_type, callback)
                except KeyError:
                    logger.warning(f"Unknown hook type: {hook_name}")

            self._plugins[instance.name] = instance
            logger.info(f"Plugin registered: {instance.name} v{instance.version}")
        except Exception as e:
            logger.error(f"Failed to register plugin {plugin_class.__name__}: {e}")

    def unregister_plugin(self, name: str) -> None:
        """Deactivate and remove a plugin."""
        if name not in self._plugins:
            return
        plugin = self._plugins.pop(name)
        try:
            plugin.deactivate()
        except Exception as e:
            logger.error(f"Error deactivating plugin {name}: {e}")

    def get_plugin(self, name: str) -> Plugin | None:
        """Get a loaded plugin by name."""
        return self._plugins.get(name)

    def list_plugins(self) -> list[str]:
        """List all loaded plugin names."""
        return list(self._plugins.keys())

    def shutdown(self) -> None:
        """Deactivate all plugins."""
        for name in list(self._plugins.keys()):
            self.unregister_plugin(name)
