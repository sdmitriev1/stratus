# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [0.1.0] - 2026-03-09

### Added
- Workspace with 8 crates: stratusd, stratus-cli, stratus-store, stratus-vm,
  stratus-net, stratus-services, stratus-images, stratus-resources
- Rust toolchain pinned to 1.94.0
- Protocol Buffer definitions for `StratusService` with `GetStatus` RPC
- `stratusd` daemon: listens on Unix socket, serves gRPC `GetStatus` (version + uptime)
- `stratus` CLI: `stratus status` connects to daemon and prints version/uptime
- systemd unit files (`stratusd.service`, `stratusd.socket`)
- CI pipeline: check, clippy, fmt, unit tests, cargo-deny
- Release pipeline: builds amd64/arm64 static binaries on version tags
- Unit tests for daemon socket binding, gRPC status, and CLI error handling
