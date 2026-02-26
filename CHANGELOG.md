# Changelog

All notable changes to this project are documented in this file.

## [1.0.0] - 2026-02-24

This release marks whogitit as production-ready with stabilized machine output contracts, stronger operational safeguards, and improved CI/runtime robustness.

### Added

- Stable machine-readable JSON schemas for:
  - `blame --format json` (`whogitit.blame.v1`)
  - `prompt --format json` (`whogitit.prompt.v1`)
  - `show --format json` (`whogitit.show.v1`)
  - `summary --format json` (`whogitit.summary.v1`)
  - `annotations --format json` (`whogitit.annotations.v1`)
- Structured `source` objects for line attribution in machine outputs.
- `retention preview --show <N>` to bound preview output in large repositories.
- Git note payload guardrails:
  - warning at 512 KiB
  - hard limit at 4 MiB
- Config override via `WHOGITIT_CONFIG`, with explicit precedence and error context.

### Changed

- `show --format json` no longer emits `null` when attribution is missing.
  - It now emits a schema envelope with `"has_attribution": false`.
- Export date filters are now explicitly day-inclusive:
  - `--since` includes commits from `00:00:00` of the date.
  - `--until` includes commits through `23:59:59` of the date.
- Export prompt truncation is now Unicode-safe.
- Internal maintenance commands are hidden from top-level help output.

### Fixed

- Hook prompt extraction for edge cases where JSON string escaping could corrupt prompt capture.
- Pending-buffer/session handling to better preserve uncommitted histories across partial commits.
- Post-commit retention and CI scripts now handle modern `show --format json` output reliably.
- Setup/doctor checks hardened for Claude hook phase validation and required tool checks.

### Notes for Upgrades

- If you parse machine JSON, switch to `schema` + `schema_version` contracts where available.
- If you previously treated missing attribution as `null` in `show --format json`, update consumers to check `"has_attribution": false`.
