from __future__ import annotations

import json
from pathlib import Path
from types import SimpleNamespace

import poly_data.cli as cli
from poly_data.cli import main as poly_data_main


def test_poly_data_completeness_builds_manifest_with_overrides(
    tmp_path: Path,
    monkeypatch,
    capsys,
) -> None:
    config_path = tmp_path / "completeness.yaml"
    data_root = tmp_path / "data-root"
    synthetic_bbo = tmp_path / "synthetic_bbo.parquet"
    spot_prices = tmp_path / "spot.parquet"
    out_dir = tmp_path / "out"
    fake_config = object()
    calls: dict[str, object] = {}

    def fake_load(path: Path) -> object:
        calls["config_path"] = path
        return fake_config

    def fake_build_manifest(**kwargs):
        calls["build_kwargs"] = kwargs
        return SimpleNamespace(
            allowlist_csv=out_dir / "allowlist.csv",
            exclusions_csv=out_dir / "exclusions.csv",
            summary_json=out_dir / "summary.json",
        )

    monkeypatch.setattr(cli.DataCompletenessConfig, "load", staticmethod(fake_load))
    monkeypatch.setattr(cli, "build_manifest", fake_build_manifest)

    result = poly_data_main(
        [
            "completeness",
            "--date",
            "2026-02-20",
            "--config",
            str(config_path),
            "--data-root",
            str(data_root),
            "--synthetic-bbo",
            str(synthetic_bbo),
            "--spot-prices",
            str(spot_prices),
            "--out-dir",
            str(out_dir),
        ]
    )

    assert result == 0
    assert calls["config_path"] == config_path
    kwargs = calls["build_kwargs"]
    assert kwargs["config"] is fake_config
    assert kwargs["config_path"] == config_path
    assert kwargs["date"] == "2026-02-20"
    assert kwargs["data_root"] == data_root
    assert kwargs["synthetic_bbo"] == synthetic_bbo
    assert kwargs["spot_prices"] == spot_prices
    assert kwargs["out_dir"] == out_dir

    output = capsys.readouterr().out
    assert "Wrote:" in output
    assert str(out_dir / "allowlist.csv") in output


def test_poly_data_completeness_json_output(tmp_path: Path, monkeypatch, capsys) -> None:
    config_path = tmp_path / "completeness.yaml"
    out_dir = tmp_path / "out"

    monkeypatch.setattr(cli.DataCompletenessConfig, "load", staticmethod(lambda _path: object()))
    monkeypatch.setattr(
        cli,
        "build_manifest",
        lambda **_kwargs: SimpleNamespace(
            allowlist_csv=out_dir / "allowlist.csv",
            exclusions_csv=out_dir / "exclusions.csv",
            summary_json=out_dir / "summary.json",
        ),
    )

    result = poly_data_main(
        [
            "completeness",
            "--date",
            "2026-02-21",
            "--config",
            str(config_path),
            "--json",
        ]
    )

    assert result == 0
    payload = json.loads(capsys.readouterr().out)
    assert payload == {
        "date": "2026-02-21",
        "allowlist_csv": str(out_dir / "allowlist.csv"),
        "exclusions_csv": str(out_dir / "exclusions.csv"),
        "summary_json": str(out_dir / "summary.json"),
    }
