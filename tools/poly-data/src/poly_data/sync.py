from __future__ import annotations

import shutil
import subprocess
import re
from datetime import datetime, timedelta
from pathlib import Path
from typing import Any, Sequence

from .index import data_home, delete_cache_entries, index_date

COMPONENT_TO_FILE = {
    "snapshots": "snapshots.parquet",
    "updates": "updates.parquet",
    "trades": "trades.parquet",
}

DEFAULT_COMPONENTS = tuple(COMPONENT_TO_FILE.keys())
DEFAULT_HOST = "ewr1-3"
DEFAULT_USER = "ubuntu"
DEFAULT_REMOTE_ROOT = "/mnt/local-storage/airflow-data"
DATE_DIR_PATTERN = re.compile(r"date=(\d{4}-\d{2}-\d{2})")


def ensure_rsync() -> None:
    if shutil.which("rsync") is None:
        raise RuntimeError("rsync is required but not found in PATH.")


def run_cmd(cmd: list[str], dry_run: bool) -> None:
    print("[cmd]", " ".join(cmd))
    if dry_run:
        return
    subprocess.run(cmd, check=True)


def parse_date_range(date_range: str) -> list[str]:
    if ".." not in date_range:
        raise ValueError(f"Invalid --date-range '{date_range}'; expected YYYY-MM-DD..YYYY-MM-DD")
    start_s, end_s = date_range.split("..", 1)
    start = datetime.strptime(start_s, "%Y-%m-%d").date()
    end = datetime.strptime(end_s, "%Y-%m-%d").date()
    if end < start:
        raise ValueError(f"Invalid --date-range '{date_range}'; end date is before start date")

    dates: list[str] = []
    current = start
    while current <= end:
        dates.append(current.isoformat())
        current += timedelta(days=1)
    return dates


def sync_date(
    date: str,
    *,
    host: str = DEFAULT_HOST,
    user: str = DEFAULT_USER,
    remote_root: str = DEFAULT_REMOTE_ROOT,
    components: Sequence[str] = DEFAULT_COMPONENTS,
    data_root: Path | None = None,
    skip_existing: bool = False,
    dry_run: bool = False,
) -> dict[str, Any]:
    ensure_rsync()
    root = (data_root or (data_home() / "data")).expanduser()
    remote_host = f"{user}@{host}"

    normalized_components = list(components)
    for component in normalized_components:
        if component not in COMPONENT_TO_FILE:
            raise ValueError(f"Unsupported component '{component}'")

    for component in normalized_components:
        filename = COMPONENT_TO_FILE[component]
        remote_file = (
            f"{remote_host}:{remote_root}/polymarket/{component}/date={date}/{filename}"
        )
        local_dir = root / "polymarket" / component / f"date={date}"
        local_dir.mkdir(parents=True, exist_ok=True)
        local_file = local_dir / filename
        if skip_existing and local_file.exists():
            print(f"[skip] existing file: {local_file}")
            continue

        command = ["rsync", "-av", "--progress"]
        if skip_existing:
            command.append("--ignore-existing")
        command.extend([remote_file, str(local_dir)])
        run_cmd(command, dry_run)

    remote_spot = f"{remote_host}:{remote_root}/spot_prices/daily/date={date}/spot_prices.parquet"
    local_spot_dir = root / "polymarket" / "spot" / f"date={date}"
    local_spot_dir.mkdir(parents=True, exist_ok=True)
    local_spot_file = local_spot_dir / "spot_prices.parquet"
    if skip_existing and local_spot_file.exists():
        print(f"[skip] existing file: {local_spot_file}")
    else:
        spot_command = ["rsync", "-av", "--progress"]
        if skip_existing:
            spot_command.append("--ignore-existing")
        spot_command.extend([remote_spot, str(local_spot_dir)])
        run_cmd(spot_command, dry_run)

    if dry_run:
        return {
            "date": date,
            "components": normalized_components,
            "data_root": str(root),
            "indexed": False,
            "cache_entries_deleted": 0,
        }

    index_result = index_date(date, root)
    deleted = delete_cache_entries(date)
    return {
        "date": date,
        "components": normalized_components,
        "data_root": str(root),
        "indexed": True,
        "cache_entries_deleted": deleted,
        "index_result": index_result,
    }


def discover_remote_dates(
    host: str = DEFAULT_HOST,
    user: str = DEFAULT_USER,
    remote_root: str = DEFAULT_REMOTE_ROOT,
) -> list[str]:
    remote_host = f"{user}@{host}"
    remote_snapshots_root = f"{remote_root}/polymarket/snapshots/"
    command = ["ssh", remote_host, "ls", remote_snapshots_root]
    process = subprocess.run(command, check=False, capture_output=True, text=True)
    if process.returncode != 0:
        raise RuntimeError(
            f"Failed to discover remote dates via {' '.join(command)}\n"
            f"stdout:\n{process.stdout}\n"
            f"stderr:\n{process.stderr}"
        )

    dates: set[str] = set()
    for line in process.stdout.splitlines():
        match = DATE_DIR_PATTERN.search(line.strip())
        if match:
            dates.add(match.group(1))
    return sorted(dates)
