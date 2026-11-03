"""Print what Pluto recorded for one stretch of today, with the gaps between sessions.

Read-only: it opens Pluto's database without changing it, and prints only app names and times
(no window titles, no notes, nothing from the diary). Safe to run while Pluto is open.

    python scripts/tracking-report.py              # today 22:00-23:00
    python scripts/tracking-report.py 21:30 23:00  # another stretch today
    python scripts/tracking-report.py 21:30 23:00 2026-10-31

Sessions still in Pluto's memory (the app in front right now, and up to a minute of finished
ones) are not in the database yet, so run it a couple of minutes after the stretch ends.
"""

import datetime as dt
import os
import sqlite3
import sys
from pathlib import Path


def database() -> Path:
    if sys.platform == "win32":
        base = Path(os.environ["APPDATA"])
    elif sys.platform == "darwin":
        base = Path.home() / "Library" / "Application Support"
    else:
        base = Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local" / "share"))
    return base / "app.pluto.desktop" / "pluto.db"


def ms(day: dt.date, hhmm: str) -> int:
    h, m = (int(x) for x in hhmm.split(":"))
    return int(dt.datetime.combine(day, dt.time(h, m)).timestamp() * 1000)


def clock(t: int) -> str:
    return dt.datetime.fromtimestamp(t / 1000).strftime("%H:%M:%S")


def mins(t: int) -> str:
    return f"{t / 60000:5.1f} min"


def main() -> None:
    start_s, end_s = (sys.argv[1], sys.argv[2]) if len(sys.argv) > 2 else ("22:00", "23:00")
    day = dt.date.fromisoformat(sys.argv[3]) if len(sys.argv) > 3 else dt.date.today()
    start, end = ms(day, start_s), ms(day, end_s)
    path = database()
    if not path.exists():
        sys.exit(f"Pluto's database is not at {path}")
    conn = sqlite3.connect(f"file:{path.as_posix()}?mode=ro", uri=True)
    rows = conn.execute(
        "SELECT app, MAX(started_at, ?1), MIN(ended_at, ?2) FROM app_sessions"
        " WHERE started_at < ?2 AND ended_at > ?1 ORDER BY started_at",
        (start, end),
    ).fetchall()
    print(f"{day} {start_s}-{end_s}: {len(rows)} sessions")
    print(f"{'from':>8}  {'to':>8}  {'length':>9}  {'gap before':>10}  app")
    counted, gaps, previous = 0, 0, start
    for app, s, e in rows:
        gap = max(0, s - previous)
        gaps += gap
        counted += e - s
        mark = "  <-- gap" if gap >= 30_000 else ""
        print(f"{clock(s)}  {clock(e)}  {mins(e - s)}  {mins(gap):>10}  {app}{mark}")
        previous = max(previous, e)
    tail = max(0, end - previous)
    gaps += tail
    print(f"\ncounted {mins(counted)}, not counted {mins(gaps)} (of which {mins(tail)} after the last session)")
    trace(path.parent / "tracking-trace.txt", start, end)


def trace(path: Path, start: int, end: int) -> None:
    """What Windows said was in front (Pluto 0.5.7 and later): program and window class names."""
    if not path.exists():
        print("\n(no tracking trace yet: it is written by Pluto 0.5.7 and later, on Windows)")
        return
    lines = []
    for raw in path.read_text(encoding="utf-8", errors="replace").splitlines():
        parts = raw.split("\t")
        if len(parts) == 5 and parts[0].isdigit() and start <= int(parts[0]) < end:
            lines.append(parts)
    print(f"\nWhat Windows said was in front ({len(lines)} lines):")
    for t, kind, event, front, recorded in lines:
        print(f"{clock(int(t))}  {kind:<7}  {event:<45}  {front:<45}  {recorded}")


if __name__ == "__main__":
    main()
