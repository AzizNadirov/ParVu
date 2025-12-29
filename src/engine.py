"""
DuckDB Query Engine with Pagination Support

This engine uses lazy loading to handle huge files (8GB+) efficiently:
- Files are NOT loaded into memory as tables
- Queries directly access files using read_parquet/read_csv/read_json
- DuckDB's lazy evaluation only processes data when needed
- Only the current page is materialized in memory
- Memory usage remains constant regardless of file size
"""
from pathlib import Path
from typing import Optional, List, Tuple
import duckdb
import pandas as pd
from loguru import logger

from schemas import settings


class QueryEngine:
    """
    Manages DuckDB queries with efficient pagination for large datasets
    """

    def __init__(self, file_path: Path, page_size: int = 100):
        """
        Initialize query engine

        Args:
            file_path: Path to parquet/csv/json file
            page_size: Number of rows per page
        """
        self.file_path = Path(file_path)
        self.page_size = page_size
        self.table_name = settings.render_vars(settings.default_data_var_name)

        # DuckDB connection
        self.conn = duckdb.connect(':memory:')

        # Build base query that reads file directly (lazy evaluation)
        self.file_reader_query = self._build_file_reader_query()

        # Query state
        self.current_query = self.file_reader_query
        self.total_rows = self._count_rows()
        self.total_pages = (self.total_rows + page_size - 1) // page_size

        logger.info(f"QueryEngine initialized: {file_path}, {self.total_rows} rows, {self.total_pages} pages")

    def _build_file_reader_query(self) -> str:
        """
        Build query that reads file directly without materializing it.
        DuckDB will read the file lazily, only loading necessary data.

        Returns:
            SQL query string that reads the file
        """
        suffix = self.file_path.suffix.lower()
        path_str = str(self.file_path)

        if suffix == '.parquet':
            reader_query = f"SELECT * FROM read_parquet('{path_str}')"
        elif suffix == '.csv':
            reader_query = f"SELECT * FROM read_csv('{path_str}')"
        elif suffix == '.json':
            reader_query = f"SELECT * FROM read_json('{path_str}')"
        else:
            raise ValueError(f"Unsupported file format: {suffix}")

        logger.debug(f"Built lazy file reader query for {path_str}")
        return reader_query

    def _count_rows(self, query: Optional[str] = None) -> int:
        """Count total rows in current query result"""
        if query is None:
            query = self.current_query

        count_query = f"SELECT COUNT(*) as cnt FROM ({query})"
        result = self.conn.execute(count_query).fetchone()
        return result[0] if result else 0

    def _substitute_table_name(self, query: str) -> str:
        """
        Replace table variable name with actual file reader query.
        This allows users to write 'SELECT * FROM data' which gets converted
        to 'SELECT * FROM read_parquet(...)' for lazy evaluation.

        Args:
            query: User's SQL query with table variable

        Returns:
            Query with table name substituted
        """
        import re
        # Case-insensitive replacement of table name
        pattern = re.compile(rf'\b{re.escape(self.table_name)}\b', re.IGNORECASE)
        substituted = pattern.sub(f'({self.file_reader_query})', query)
        return substituted

    def _wrap_query(self, query: str) -> str:
        """
        Wrap query in a subquery to avoid LIMIT conflicts

        Args:
            query: Original SQL query

        Returns:
            Wrapped query safe for adding LIMIT/OFFSET
        """
        return f"SELECT * FROM ({query})"

    def get_columns(self) -> List[str]:
        """Get column names from current query result"""
        try:
            wrapped_query = self._wrap_query(self.current_query)
            result = self.conn.execute(f"{wrapped_query} LIMIT 0")
            return [desc[0] for desc in result.description]
        except Exception as e:
            logger.error(f"Error getting columns: {e}")
            return []

    def get_column_types(self) -> List[Tuple[str, str]]:
        """Get column names and types"""
        try:
            wrapped_query = self._wrap_query(self.current_query)
            result = self.conn.execute(f"{wrapped_query} LIMIT 0")
            return [(desc[0], str(desc[1])) for desc in result.description]
        except Exception as e:
            logger.error(f"Error getting column types: {e}")
            return []

    def execute_query(self, query: str) -> tuple[bool, str]:
        """
        Execute a SQL query and update pagination state

        Args:
            query: SQL query to execute (can reference table variable name)

        Returns:
            Tuple of (success: bool, error_message: str or empty)
        """
        try:
            # Substitute table name with file reader query
            substituted_query = self._substitute_table_name(query)

            # Validate query by executing it with LIMIT 0
            wrapped_query = self._wrap_query(substituted_query)
            self.conn.execute(f"{wrapped_query} LIMIT 0")

            # Update state (store substituted query for actual execution)
            self.current_query = substituted_query
            self.total_rows = self._count_rows()
            self.total_pages = max(1, (self.total_rows + self.page_size - 1) // self.page_size)

            logger.info(f"Query executed: {self.total_rows} rows, {self.total_pages} pages")
            return True, ""

        except Exception as e:
            error_msg = str(e)
            logger.error(f"Query execution failed: {error_msg}")
            return False, error_msg

    def get_page(self, page_num: int) -> pd.DataFrame:
        """
        Get a specific page of results

        Args:
            page_num: Page number (1-indexed)

        Returns:
            DataFrame with page data
        """
        if page_num < 1 or page_num > self.total_pages:
            logger.warning(f"Invalid page number: {page_num}")
            return pd.DataFrame()

        offset = (page_num - 1) * self.page_size
        wrapped_query = self._wrap_query(self.current_query)
        paginated_query = f"{wrapped_query} LIMIT {self.page_size} OFFSET {offset}"

        try:
            return self.conn.execute(paginated_query).df()
        except Exception as e:
            logger.error(f"Error fetching page {page_num}: {e}")
            return pd.DataFrame()

    def get_unique_values(self, column: str) -> List:
        """
        Get unique values for a column with efficiency warning for large datasets

        Args:
            column: Column name

        Returns:
            List of unique values (limited to 10000 for performance)
        """
        try:
            # Warn if dataset is large
            if self.total_rows > 1_000_000:
                logger.warning(f"Large dataset ({self.total_rows} rows) - unique value calculation may be slow")

            # Limit unique values to 10000 for performance
            query = f"SELECT DISTINCT {column} FROM ({self.current_query}) LIMIT 10000"
            result = self.conn.execute(query).fetchall()
            return [row[0] for row in result]

        except Exception as e:
            logger.error(f"Error getting unique values for {column}: {e}")
            return []

    def sort_by_column(self, column: str, ascending: bool = True) -> tuple[bool, str]:
        """
        Sort current results by column

        Args:
            column: Column name to sort by
            ascending: Sort direction

        Returns:
            Tuple of (success: bool, error_message: str or empty)
        """
        try:
            order = "ASC" if ascending else "DESC"
            # Wrap current query and add ORDER BY
            new_query = f"SELECT * FROM ({self.current_query}) ORDER BY {column} {order}"

            # Validate the new query
            wrapped_query = self._wrap_query(new_query)
            self.conn.execute(f"{wrapped_query} LIMIT 0")

            # Update state
            self.current_query = new_query
            self.total_rows = self._count_rows()
            self.total_pages = max(1, (self.total_rows + self.page_size - 1) // self.page_size)

            return True, ""
        except Exception as e:
            error_msg = str(e)
            logger.error(f"Error sorting by {column}: {error_msg}")
            return False, error_msg

    def export_results(self, output_path: str) -> bool:
        """
        Export current query results to file

        Args:
            output_path: Output file path

        Returns:
            True if successful
        """
        try:
            output_path = Path(output_path)
            suffix = output_path.suffix.lower()

            if suffix == '.csv':
                self.conn.execute(f"COPY ({self.current_query}) TO '{output_path}' (FORMAT CSV, HEADER)")
            elif suffix == '.parquet':
                self.conn.execute(f"COPY ({self.current_query}) TO '{output_path}' (FORMAT PARQUET)")
            elif suffix == '.json':
                self.conn.execute(f"COPY ({self.current_query}) TO '{output_path}' (FORMAT JSON)")
            else:
                logger.error(f"Unsupported export format: {suffix}")
                return False

            logger.info(f"Exported results to {output_path}")
            return True

        except Exception as e:
            logger.error(f"Export failed: {e}")
            return False

    def reset_query(self):
        """Reset to original file reader query"""
        self.current_query = self.file_reader_query
        self.total_rows = self._count_rows()
        self.total_pages = (self.total_rows + self.page_size - 1) // self.page_size
        logger.info("Query reset to original file")

    def get_table_info(self) -> dict:
        """Get table metadata"""
        return {
            'total_rows': self.total_rows,
            'total_pages': self.total_pages,
            'page_size': self.page_size,
            'columns': self.get_columns(),
            'file_path': str(self.file_path)
        }

    def close(self):
        """Close database connection"""
        self.conn.close()
        logger.info("QueryEngine closed")
