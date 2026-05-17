"""
File format adapters for ParVu.

Maps file extensions to DuckDB reader functions.
"""
from __future__ import annotations

from pathlib import Path

from parvu.core.interfaces import IFileAdapter
from parvu.core.exceptions import FileFormatError


class ParquetAdapter(IFileAdapter):
    """Adapter for Parquet files."""

    @property
    def supported_extensions(self) -> set[str]:
        return {".parquet", ".pq"}

    def can_handle(self, file_path: Path) -> bool:
        return file_path.suffix.lower() in self.supported_extensions

    def build_reader_query(self, file_path: Path) -> str:
        return f"SELECT * FROM read_parquet('{file_path}')"


class CsvAdapter(IFileAdapter):
    """Adapter for CSV files."""

    @property
    def supported_extensions(self) -> set[str]:
        return {".csv", ".tsv"}

    def can_handle(self, file_path: Path) -> bool:
        return file_path.suffix.lower() in self.supported_extensions

    def build_reader_query(self, file_path: Path) -> str:
        return f"SELECT * FROM read_csv('{file_path}')"


class JsonAdapter(IFileAdapter):
    """Adapter for JSON files."""

    @property
    def supported_extensions(self) -> set[str]:
        return {".json", ".jsonl", ".ndjson"}

    def can_handle(self, file_path: Path) -> bool:
        return file_path.suffix.lower() in self.supported_extensions

    def build_reader_query(self, file_path: Path) -> str:
        return f"SELECT * FROM read_json('{file_path}')"


class ExcelAdapter(IFileAdapter):
    """Adapter for Excel files (via DuckDB's spatial extension or pandas fallback)."""

    @property
    def supported_extensions(self) -> set[str]:
        return {".xlsx", ".xls"}

    def can_handle(self, file_path: Path) -> bool:
        return file_path.suffix.lower() in self.supported_extensions

    def build_reader_query(self, file_path: Path) -> str:
        # DuckDB can read Excel via extension; fallback to pandas if needed
        return f"SELECT * FROM read_xlsx('{file_path}')"


class FileAdapterRegistry:
    """Registry of file format adapters."""

    def __init__(self):
        self._adapters: list[IFileAdapter] = [
            ParquetAdapter(),
            CsvAdapter(),
            JsonAdapter(),
            ExcelAdapter(),
        ]

    def register(self, adapter: IFileAdapter) -> None:
        """Register a custom adapter."""
        self._adapters.append(adapter)

    def get_adapter(self, file_path: Path) -> IFileAdapter:
        """Find an adapter for the given file path."""
        for adapter in self._adapters:
            if adapter.can_handle(file_path):
                return adapter
        raise FileFormatError(
            f"Unsupported file format: {file_path.suffix}. "
            f"Supported: {', '.join(self.supported_extensions)}"
        )

    @property
    def supported_extensions(self) -> set[str]:
        """All supported file extensions."""
        exts: set[str] = set()
        for adapter in self._adapters:
            exts.update(adapter.supported_extensions)
        return exts

    def build_file_dialog_filter(self) -> str:
        """Build a QFileDialog filter string for supported formats."""
        parts = []
        all_exts = " ".join(f"*{e}" for e in sorted(self.supported_extensions))
        parts.append(f"Data Files ({all_exts})")
        for adapter in self._adapters:
            for ext in sorted(adapter.supported_extensions):
                parts.append(f"{ext.upper()[1:]} Files (*{ext})")
        parts.append("All Files (*)")
        return ";;".join(parts)

    def build_export_dialog_filter(self) -> str:
        """Build a QFileDialog filter string for export formats."""
        return ";;".join([
            "CSV Files (*.csv)",
            "Parquet Files (*.parquet)",
            "JSON Files (*.json)",
            "All Files (*)",
        ])


# Global registry instance (can be replaced in tests)
default_registry = FileAdapterRegistry()
