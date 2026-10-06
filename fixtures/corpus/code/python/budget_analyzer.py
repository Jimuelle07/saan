"""Read a bank CSV export and summarize spending per category per month."""

import csv
from collections import defaultdict
from pathlib import Path

from utils import normalize_merchant, parse_amount, parse_date

CATEGORIES = {
    "groceries": ["safeway", "trader joe", "whole foods", "costco"],
    "dining": ["starbucks", "chipotle", "doordash", "pizza"],
    "transport": ["shell", "chevron", "uber", "lyft", "metro"],
    "utilities": ["city light", "comcast", "pse"],
    "subscriptions": ["netflix", "spotify", "icloud"],
}


def categorize(merchant: str) -> str:
    m = normalize_merchant(merchant)
    for category, keywords in CATEGORIES.items():
        if any(k in m for k in keywords):
            return category
    return "other"


def summarize(csv_path: Path) -> dict[str, dict[str, float]]:
    totals: dict[str, dict[str, float]] = defaultdict(lambda: defaultdict(float))
    with csv_path.open(newline="") as f:
        for row in csv.DictReader(f):
            amount = parse_amount(row["Amount"])
            if amount >= 0:
                continue  # deposits and refunds
            month = parse_date(row["Date"]).strftime("%Y-%m")
            totals[month][categorize(row["Description"])] += -amount
    return totals


def print_report(totals) -> None:
    for month in sorted(totals):
        print(month)
        for category, spent in sorted(totals[month].items(), key=lambda kv: -kv[1]):
            print(f"  {category:<14} ${spent:>9,.2f}")


if __name__ == "__main__":
    import sys

    print_report(summarize(Path(sys.argv[1])))
