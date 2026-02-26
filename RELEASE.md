# Release Process

This project ships via GitHub tags and the `release.yml` workflow.

## Pre-Release Checklist

Run locally before tagging:

```bash
cargo fmt --all -- --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

Validate docs and command help consistency:

```bash
cargo run -- --help
cargo run -- retention preview --help
```

## Cut a Release

1. Ensure `Cargo.toml` version is correct.
2. Update `CHANGELOG.md`.
3. Create and push the tag:

```bash
git tag v1.0.0
git push origin v1.0.0
```

## What CI Publishes

`release.yml` builds artifacts for:

- `aarch64-apple-darwin`
- `x86_64-apple-darwin`
- `aarch64-unknown-linux-gnu`
- `x86_64-unknown-linux-gnu`
- `x86_64-pc-windows-msvc`

CI then creates:

- GitHub Release assets (`.tar.gz` / `.zip` + checksums)
- install scripts (`install.sh`, `install.ps1`)

## Post-Release Verification

1. Validate installers from the release page.
2. Confirm Homebrew formula update (if enabled for the tag).
3. Confirm `whogitit --version` matches the tagged version.
