# ParVu Architecture

## Overview

ParVu uses a **layered architecture** with clear separation of concerns:

```
┌─────────────────────────────────────────┐
│  Presentation Layer (PyQt6 UI)          │
│  - MainWindow, Widgets, Dialogs         │
├─────────────────────────────────────────┤
│  Service Layer (Orchestration)          │
│  - DI Container, WindowService,         │
│    QueryService, FileService            │
├─────────────────────────────────────────┤
│  Infrastructure Layer                   │
│  - i18n, Themes, Logging, Paths         │
├─────────────────────────────────────────┤
│  Config Layer                           │
│  - Settings, Recents, AppState          │
├─────────────────────────────────────────┤
│  Core Layer (Domain)                    │
│  - QueryEngine, FileAdapters,           │
│    Pagination, Models                   │
└─────────────────────────────────────────┘
```

## Dependency Flow

Dependencies flow **inward** - outer layers depend on inner layers, never the reverse:

- **Presentation** → **Services** → **Infrastructure/Config/Core**
- **Services** wire dependencies via **Dependency Injection**
- **Core** has **zero** external dependencies (except pandas/duckdb)

## Key Design Patterns

### 1. Dependency Injection
All components receive their dependencies rather than importing globals:

```python
container = ServiceContainer()
# container.settings, container.theme_manager, container.translator
```

### 2. Protocols (Structural Subtyping)
Interfaces are defined as `typing.Protocol` for flexibility:

```python
class IQueryEngine(Protocol):
    def execute_query(self, query: str) -> tuple[bool, str]: ...
```

### 3. Plugin System
Hook-based extensibility:

```python
class MyPlugin(Plugin):
    def activate(self, context: PluginContext) -> None:
        context.container.hook_registry.register(HookType.PRE_QUERY, my_callback)
```

## Data Flow

```
User Action
    ↓
MainWindow (UI Event)
    ↓
Service Layer (QueryService/FileService)
    ↓
Core Layer (QueryEngine → DuckDB)
    ↓
Paginated Results (pd.DataFrame)
    ↓
DataTableView (Display)
```

## Testing Strategy

| Layer | Test Type | Tools |
|-------|-----------|-------|
| Core | Unit | pytest (no Qt/DB) |
| Core | Integration | pytest + DuckDB |
| Services | Unit | pytest + mocks |
| Presentation | UI | pytest-qt |
| End-to-End | E2E | pytest-qt + fixtures |
