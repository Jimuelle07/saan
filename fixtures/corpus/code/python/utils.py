"""Small helpers for dates and text used by the expense scripts."""

from __future__ import annotations

import re
import unicodedata
from datetime import date, datetime, timedelta

DATE_FORMATS = ("%Y-%m-%d", "%m/%d/%Y", "%d.%m.%Y", "%b %d, %Y")


def parse_date(text: str) -> date:
    """Parse a date written in any of the common formats we see in bank exports."""
    text = text.strip()
    for fmt in DATE_FORMATS:
        try:
            return datetime.strptime(text, fmt).date()
        except ValueError:
            continue
    raise ValueError(f"unrecognized date: {text!r}")


def month_bounds(d: date) -> tuple[date, date]:
    """Return the first and last day of the month containing d."""
    first = d.replace(day=1)
    next_month = (first + timedelta(days=32)).replace(day=1)
    return first, next_month - timedelta(days=1)


def strip_accents(text: str) -> str:
    normalized = unicodedata.normalize("NFKD", text)
    return "".join(ch for ch in normalized if not unicodedata.combining(ch))


def normalize_merchant(name: str) -> str:
    """'STARBUCKS #1234 SEATTLE WA' -> 'starbucks'"""
    name = strip_accents(name).lower()
    name = re.sub(r"#\d+.*$", "", name)
    name = re.sub(r"[^a-z ]", " ", name)
    return " ".join(name.split()[:2])


def parse_amount(text: str) -> float:
    """Handle '$1,234.50', '(45.00)' for negatives, and trailing minus signs."""
    t = text.strip().replace("$", "").replace(",", "")
    negative = t.startswith("(") and t.endswith(")") or t.endswith("-")
    t = t.strip("()-")
    value = float(t)
    return -value if negative else value
