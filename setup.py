import sys
from cx_Freeze import setup, Executable

# Dependencies are automatically detected, but it might need fine-tuning.
build_exe_options = {
    "packages": ["os", "pydantic", "duckdb", "pandas", "PyQt5"],
    "include_files": [
        ("src/schemas.py", "schemas.py"),
        ("src/query_revisor.py", "query_revisor.py"),
        ("src/static/loading-thinking.gif", "static/loading-thinking.gif"),
        ("src/settings/settings.json", "settings/settings.json"),
        ("src/settings/default_settings.json", "settings/default_settings.json"),
        ("src/history/recents.json", "history/recents.json"),
    ],
    "excludes": ["tkinter"],
}

# GUI applications require a different base on Windows (the default is for a console application).
base = None
if sys.platform == "win32":
    base = "Win32GUI"

setup(
    name="ParquetSQLApp",
    version="0.1",
    description="Parquet SQL Executor",
    options={"build_exe": build_exe_options},
    executables=[Executable("src/main.py", base=base)],
)
