# poly-data

Read `docs/architecture.md` before making code changes or answering user questions. Keep `docs/architecture.md` up to date when the codebase changes.

## Quick Reference

- **Language**: Python 3.12+
- **Package manager**: uv
- **Entry point**: `src/poly_data/cli.py` → installed as `poly-data`
- **Run**: `uv run poly-data <subcommand>`
- **Test**: `uv run pytest`
- **Lint**: `uv run ruff check src/ tests/`
- **Format**: `uv run ruff format src/ tests/`

## Key Conventions

- Data source is GCS (Google Cloud Storage). The old ewr1-3 rsync-based sync is deprecated.
- Data lives under `~/.polysharp/` locally (see architecture.md for layout)
- Serves both backtesting and model training pipelines
- Parquet analysis uses DuckDB (in-memory SQL), not pandas
- Metadata index is SQLite at `~/.polysharp/index.db`
- Ticker window info is encoded in ticker name suffix (e.g., `btc-updown-15m-1704067200`)
- Completeness config lives in `configs/data_completeness.yaml`
