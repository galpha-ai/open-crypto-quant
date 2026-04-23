"""Per-ticker input loader.

A company YAML looks like:

    ticker: NVDA
    name: NVIDIA
    as_of: 2025-01-15
    notes: "initial score"

    signals:
      jurisdiction_risk: 0.9
      single_point_of_failure: true
      ...

    # Downside inputs (consumed by sizing.py)
    downside:
      revenue_concentration_top_client: 0.18
      next_gen_replacement_risk: 0.2
      ...

    # Price-action inputs (consumed by price_action.py)
    price_action:
      iv_regime: 0.6
      ...
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import yaml


@dataclass
class CompanyInputs:
    ticker: str
    name: str
    as_of: str | None = None
    notes: str = ""
    signals: dict[str, Any] = field(default_factory=dict)
    downside: dict[str, Any] = field(default_factory=dict)
    price_action: dict[str, Any] = field(default_factory=dict)
    # Free-form extras passed through to sizing (e.g. conviction, meme_dip).
    meta: dict[str, Any] = field(default_factory=dict)


def load_company(path: str | Path) -> CompanyInputs:
    data = yaml.safe_load(Path(path).read_text())
    if not isinstance(data, dict):
        raise ValueError(f"{path}: root must be a mapping")
    try:
        ticker = str(data["ticker"]).upper()
        name = str(data.get("name", ticker))
    except KeyError as exc:
        raise ValueError(f"{path}: missing required field {exc}") from exc
    return CompanyInputs(
        ticker=ticker,
        name=name,
        as_of=data.get("as_of"),
        notes=str(data.get("notes", "")),
        signals=dict(data.get("signals") or {}),
        downside=dict(data.get("downside") or {}),
        price_action=dict(data.get("price_action") or {}),
        meta={
            k: v
            for k, v in data.items()
            if k not in {"ticker", "name", "as_of", "notes", "signals", "downside", "price_action"}
        },
    )


def load_universe(directory: str | Path) -> list[CompanyInputs]:
    """Load every *.yaml file under `directory` as a CompanyInputs."""
    root = Path(directory)
    if not root.exists():
        raise FileNotFoundError(f"universe dir not found: {root}")
    companies: list[CompanyInputs] = []
    for path in sorted(root.glob("*.yaml")):
        companies.append(load_company(path))
    return companies
