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
├─────────────────────────────────────────┤
│  DSL Layer (Expression Language)        │
│  - Parser, Resolver, Compiler, Registry │
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

### SQL Mode
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

### Expression Mode
```
User types expression
    ↓
Parser (Lark grammar)
    ↓
Resolver (Tree → IR: Assignment, DropDuplicates, Call, etc.)
    ↓
Compiler (IR → sqlglot → DuckDB SQL)
    ↓
QueryEngine.execute_query()
    ↓
Paginated Results
```

## Undo & History

The `QueryEngine` maintains a `_history` stack of previous queries. Every transform pushes the current query before applying the new one:

```python
engine.apply_transform(new_query)  # pushes old query to history
engine.undo()                      # pops and restores previous query
```

Cell edits are tracked separately in `TableTab._undo_stack` (parallel to `applied_steps`). The `MainWindow._undo_last_step()` dispatcher routes undo to either:
- **SQL transforms** → `QueryEngine.undo()`
- **Cell edits** → `EditQueue.remove()`

## DSL Architecture

ParVu includes a small expression language (DSL) compiled to DuckDB SQL:

1. **Parser** (`dsl/parser.py`) — Lark grammar parses text into AST
2. **Resolver** (`dsl/resolver.py`) — Transforms AST into typed IR nodes (`ColumnRef`, `Call`, `Assignment`, `DropDuplicates`, etc.)
3. **Compiler** (`dsl/compiler.py`) — Converts IR to sqlglot expressions, then to SQL string
4. **Registry** (`dsl/registry.py`) — Function definitions for auto-completion and validation

Example compilation chain:
```
data[total] = price * quantity
  → Assignment(table="data", column="total", value=BinaryOp(...))
  → SELECT *, price * quantity AS "total" FROM data
```

## Testing Strategy

| Layer | Test Type | Tools |
|-------|-----------|-------|
| Core | Unit | pytest (no Qt/DB) |
| Core | Integration | pytest + DuckDB |
| Services | Unit | pytest + mocks |
| Presentation | UI | pytest-qt |
| End-to-End | E2E | pytest-qt + fixtures |
