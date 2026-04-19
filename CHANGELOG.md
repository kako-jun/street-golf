# Changelog

All notable changes to street-golf are documented in this file. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Project skeleton on top of termray 0.3 and rapier3d 0.32
  ([#1](https://github.com/kako-jun/street-golf/issues/1)). `cargo run` blanks the
  framebuffer to a meadow green for a brief moment and exits cleanly — Phase 1+ will
  turn this into a playable street golf game.
- MIT `LICENSE`, `README.md`, `CLAUDE.md`, and `docs/architecture.md` describing the
  termray + rapier3d design and the Phase 0 → 5+ roadmap.
- GitHub Actions workflows: `ci.yml` (fmt / clippy / test / release build on every push
  to `main` and every PR) and `release.yml` (five target binaries — linux x86_64 /
  aarch64, macOS x86_64 / aarch64, windows x86_64 — on `v*` tags, uploaded via
  `softprops/action-gh-release@v2`).
