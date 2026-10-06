"""Fetch a 7 day forecast and warn me if it will freeze overnight (to cover the garden)."""

import json
import sys
import urllib.request

LAT, LON = 47.61, -122.33
URL = (
    "https://api.open-meteo.com/v1/forecast"
    f"?latitude={LAT}&longitude={LON}"
    "&daily=temperature_2m_min,temperature_2m_max,precipitation_sum"
    "&temperature_unit=fahrenheit&timezone=auto"
)
FREEZE_F = 33.0


def fetch_forecast(url: str = URL) -> dict:
    with urllib.request.urlopen(url, timeout=10) as resp:
        return json.load(resp)


def frost_nights(daily: dict) -> list[tuple[str, float]]:
    return [
        (day, low)
        for day, low in zip(daily["time"], daily["temperature_2m_min"])
        if low <= FREEZE_F
    ]


def main() -> int:
    try:
        data = fetch_forecast()
    except OSError as err:
        print(f"could not reach the weather service: {err}", file=sys.stderr)
        return 1
    daily = data["daily"]
    for day, hi, lo, rain in zip(
        daily["time"],
        daily["temperature_2m_max"],
        daily["temperature_2m_min"],
        daily["precipitation_sum"],
    ):
        print(f"{day}: high {hi:.0f}F low {lo:.0f}F rain {rain:.1f}mm")
    cold = frost_nights(daily)
    if cold:
        print("\nFrost warning! Cover the tomatoes on:")
        for day, low in cold:
            print(f"  {day} (low {low:.0f}F)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
