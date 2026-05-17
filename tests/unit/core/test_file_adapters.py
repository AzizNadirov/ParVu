"""
Unit tests for file adapters.
"""
from pathlib import Path

import pytest

from parvu.core.file_adapters import (
    FileAdapterRegistry,
    ParquetAdapter,
    CsvAdapter,
    JsonAdapter,
)
from parvu.core.exceptions import FileFormatError


def test_parquet_adapter():
    adapter = ParquetAdapter()
    assert adapter.can_handle(Path("test.parquet"))
    assert adapter.can_handle(Path("test.pq"))
    assert not adapter.can_handle(Path("test.csv"))


def test_csv_adapter():
    adapter = CsvAdapter()
    assert adapter.can_handle(Path("test.csv"))
    assert not adapter.can_handle(Path("test.parquet"))


def test_json_adapter():
    adapter = JsonAdapter()
    assert adapter.can_handle(Path("test.json"))
    assert adapter.can_handle(Path("test.jsonl"))
    assert not adapter.can_handle(Path("test.csv"))


def test_registry_get_adapter():
    registry = FileAdapterRegistry()
    adapter = registry.get_adapter(Path("test.parquet"))
    assert isinstance(adapter, ParquetAdapter)

    adapter = registry.get_adapter(Path("test.csv"))
    assert isinstance(adapter, CsvAdapter)


def test_registry_unsupported():
    registry = FileAdapterRegistry()
    with pytest.raises(FileFormatError):
        registry.get_adapter(Path("test.txt"))
