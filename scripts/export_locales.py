"""Export the Python locale dictionaries to flat JSON for the Rust crate.

Usage: uv run python scripts/export_locales.py

Writes crates/parvu-config/locales/{en,ru,az}.json. Keys contain dots and are
kept flat (no nesting). Placeholder syntax ({name}, {rows:,}, {size:.1f}) is
kept verbatim; the Rust translator implements the same formatting.
"""
from __future__ import annotations

import json
from pathlib import Path

from parvu.infrastructure.i18n.locales import (
    TRANSLATIONS_AZ,
    TRANSLATIONS_EN,
    TRANSLATIONS_RU,
)

ROOT = Path(__file__).resolve().parent.parent
OUT_DIR = ROOT / "crates" / "parvu-config" / "locales"


def main() -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    for code, table in (
        ("en", TRANSLATIONS_EN),
        ("ru", TRANSLATIONS_RU),
        ("az", TRANSLATIONS_AZ),
    ):
        out = OUT_DIR / f"{code}.json"
        data = dict(sorted(table.items()))
        out.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        print(f"wrote {len(data)} keys to {out.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
