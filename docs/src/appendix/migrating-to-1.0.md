# Migrating to 1.0

This guide covers compatibility-relevant changes when upgrading to whogitit `1.0.0`.

## Machine JSON Output

Machine outputs now use explicit schema metadata where supported:

- `schema_version`
- `schema` (for example, `whogitit.blame.v1`)

Prefer routing by `schema` instead of inferring shape from command names.

## `show --format json` No-Attribution Behavior

In older versions, some integrations treated no-attribution results as `null`.

In `1.0.0`, no-attribution output is a structured envelope:

```json
{
  "schema_version": 1,
  "schema": "whogitit.show.v1",
  "has_attribution": false,
  "commit": "<sha>",
  "commit_short": "<short-sha>"
}
```

Update consumers to check `has_attribution == false`.

## `annotations --format json`

`annotations --format json` now returns an object envelope:

```json
{
  "schema_version": 1,
  "schema": "whogitit.annotations.v1",
  "annotations": [],
  "summary": {}
}
```

If you previously expected a top-level array, read from `.annotations`.

## Export Date Filters

Date filters are now explicitly day-inclusive:

- `--since YYYY-MM-DD` includes commits from `00:00:00` on that date.
- `--until YYYY-MM-DD` includes commits through `23:59:59` on that date.

## Config Loading Precedence

`WHOGITIT_CONFIG` now has highest precedence over repo/global config discovery.

When set, whogitit loads only that file path and returns an error if it is missing or invalid.
