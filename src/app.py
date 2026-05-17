#!/usr/bin/env python3
"""
ParVu - Backward-compatible entry point.

This file delegates to the new package structure.
For the main entry point, use: python -m parvu
"""
import sys
import warnings

warnings.warn(
    "src/app.py is deprecated. Use 'python -m parvu' instead.",
    DeprecationWarning,
    stacklevel=2,
)

from parvu.__main__ import main

if __name__ == "__main__":
    sys.exit(main())
