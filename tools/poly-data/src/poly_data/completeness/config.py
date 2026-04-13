from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any

import yaml


@dataclass(frozen=True)
class InputsConfig:
    data_root: Path
    synthetic_bbo_path: str
    synthetic_bbo_template: str
    spot_prices_path: str
    spot_prices_template: str


@dataclass(frozen=True)
class UniverseConfig:
    ticker_regexes: list[str]
    required_outcomes: list[str]


@dataclass(frozen=True)
class EventTimeConfig:
    horizons_s: list[float]
    max_gap_ms: int
    require_forward_freshness_for_max_horizon: bool


@dataclass(frozen=True)
class ThresholdsConfig:
    min_usable_events: int
    min_usable_ratio: float
    max_end_gap_ms: int
    max_inter_event_gap_ms: int


@dataclass(frozen=True)
class ValidationConfig:
    require_best_bid_ask_non_null: bool
    require_non_negative_spread: bool
    enforce_price_bounds: bool
    min_price: float
    max_price: float


@dataclass(frozen=True)
class SpotConfig:
    enabled: bool
    symbol: str
    max_gap_ms: int
    min_usable_ratio: float
    min_usable_events: int


@dataclass(frozen=True)
class OutputsConfig:
    out_dir_template: str
    manifest_csv: str
    exclusions_csv: str
    summary_json: str


@dataclass(frozen=True)
class DataCompletenessConfig:
    version: int
    inputs: InputsConfig
    universe: UniverseConfig
    event_time: EventTimeConfig
    thresholds: ThresholdsConfig
    validation: ValidationConfig
    spot: SpotConfig
    outputs: OutputsConfig

    @staticmethod
    def load(path: Path) -> "DataCompletenessConfig":
        raw = yaml.safe_load(path.read_text())
        if not isinstance(raw, dict):
            raise ValueError(f"Invalid config (expected mapping): {path}")

        version = _expect_int(raw, "version")
        inputs = raw.get("inputs", {})
        universe = raw.get("universe", {})
        event_time = raw.get("event_time", {})
        thresholds = raw.get("thresholds", {})
        validation = raw.get("validation", {})
        spot = raw.get("spot", {})
        outputs = raw.get("outputs", {})

        inputs_cfg = InputsConfig(
            data_root=Path(_expect_str(inputs, "data_root")),
            synthetic_bbo_path=_expect_str(inputs, "synthetic_bbo_path"),
            synthetic_bbo_template=_expect_str(inputs, "synthetic_bbo_template"),
            spot_prices_path=_expect_str(inputs, "spot_prices_path"),
            spot_prices_template=_expect_str(inputs, "spot_prices_template"),
        )
        universe_cfg = UniverseConfig(
            ticker_regexes=_expect_str_list(universe, "ticker_regexes"),
            required_outcomes=_expect_str_list(universe, "required_outcomes"),
        )
        event_time_cfg = EventTimeConfig(
            horizons_s=_expect_float_list(event_time, "horizons_s"),
            max_gap_ms=_expect_int(event_time, "max_gap_ms"),
            require_forward_freshness_for_max_horizon=_expect_bool(
                event_time, "require_forward_freshness_for_max_horizon"
            ),
        )
        thresholds_cfg = ThresholdsConfig(
            min_usable_events=_expect_int(thresholds, "min_usable_events"),
            min_usable_ratio=_expect_float(thresholds, "min_usable_ratio"),
            max_end_gap_ms=_expect_int(thresholds, "max_end_gap_ms"),
            max_inter_event_gap_ms=_expect_int(thresholds, "max_inter_event_gap_ms"),
        )
        validation_cfg = ValidationConfig(
            require_best_bid_ask_non_null=_expect_bool(validation, "require_best_bid_ask_non_null"),
            require_non_negative_spread=_expect_bool(validation, "require_non_negative_spread"),
            enforce_price_bounds=_expect_bool(validation, "enforce_price_bounds"),
            min_price=_expect_float(validation, "min_price"),
            max_price=_expect_float(validation, "max_price"),
        )
        spot_cfg = SpotConfig(
            enabled=_expect_bool(spot, "enabled"),
            symbol=_expect_str(spot, "symbol"),
            max_gap_ms=_expect_int(spot, "max_gap_ms"),
            min_usable_ratio=_expect_float(spot, "min_usable_ratio"),
            min_usable_events=_expect_int(spot, "min_usable_events"),
        )
        outputs_cfg = OutputsConfig(
            out_dir_template=_expect_str(outputs, "out_dir_template"),
            manifest_csv=_expect_str(outputs, "manifest_csv"),
            exclusions_csv=_expect_str(outputs, "exclusions_csv"),
            summary_json=_expect_str(outputs, "summary_json"),
        )

        return DataCompletenessConfig(
            version=version,
            inputs=inputs_cfg,
            universe=universe_cfg,
            event_time=event_time_cfg,
            thresholds=thresholds_cfg,
            validation=validation_cfg,
            spot=spot_cfg,
            outputs=outputs_cfg,
        )


def _expect_mapping(obj: dict[str, Any], key: str) -> dict[str, Any]:
    val = obj.get(key)
    if not isinstance(val, dict):
        raise ValueError(f"Missing/invalid '{key}' (expected mapping)")
    return val


def _expect_str(obj: dict[str, Any], key: str) -> str:
    val = obj.get(key)
    if not isinstance(val, str):
        raise ValueError(f"Missing/invalid '{key}' (expected string)")
    return val


def _expect_int(obj: dict[str, Any], key: str) -> int:
    val = obj.get(key)
    if not isinstance(val, int):
        raise ValueError(f"Missing/invalid '{key}' (expected int)")
    return val


def _expect_float(obj: dict[str, Any], key: str) -> float:
    val = obj.get(key)
    if not isinstance(val, (int, float)):
        raise ValueError(f"Missing/invalid '{key}' (expected float)")
    return float(val)


def _expect_bool(obj: dict[str, Any], key: str) -> bool:
    val = obj.get(key)
    if not isinstance(val, bool):
        raise ValueError(f"Missing/invalid '{key}' (expected bool)")
    return val


def _expect_str_list(obj: dict[str, Any], key: str) -> list[str]:
    val = obj.get(key)
    if not isinstance(val, list) or any(not isinstance(x, str) for x in val):
        raise ValueError(f"Missing/invalid '{key}' (expected list[str])")
    return list(val)


def _expect_float_list(obj: dict[str, Any], key: str) -> list[float]:
    val = obj.get(key)
    if not isinstance(val, list) or any(not isinstance(x, (int, float)) for x in val):
        raise ValueError(f"Missing/invalid '{key}' (expected list[float])")
    return [float(x) for x in val]
