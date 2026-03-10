# Stratus

Single-node VM orchestrator for local development, testing, and lab environments.
Manages QEMU/KVM virtual machines with eBPF-based networking on a single Linux
host — a desktop, laptop, or small server.

Stratus is a simplified, single-node subset of the Aero cloud platform. The eBPF
data plane, QEMU management, IMDS, DNS, DHCP, security groups, and image caching
are architecturally identical — code written for Stratus is directly portable to
Aero's hypervisor agent.

## Use cases

- Local development environments with realistic network topologies
- Test environments for Aero (including multi-node Aero-in-VM deployments)
- BGP lab environments (peering between VMs running BIRD/FRR)
- Network topology testing (isolated subnets, NAT, port forwarding, security groups)
- CI/CD environments (declarative VM definitions in YAML, reproducible)

## Architecture

```
┌──────────────┐         ┌──────────────────────────────────┐
│  stratus CLI │  Unix   │  stratusd (daemon)               │
│              │ socket  │                                   │
│  apply       ├────────►│  ├── gRPC API (Unix socket)      │
│  get         │         │  ├── Resource manager             │
│  status      │         │  ├── QEMU process supervisor      │
│  logs        │         │  ├── eBPF data plane              │
│              │         │  ├── DHCP / DNS / IMDS            │
└──────────────┘         └──────────────────────────────────┘
```

All resources (networks, subnets, instances, security groups, images) are
declared in YAML and managed via `stratus apply`.

## Prerequisites

- Linux with KVM (`/dev/kvm`)
- Rust toolchain (pinned in `rust-toolchain.toml`)
- `protoc` (Protocol Buffers compiler)
- QEMU, OVMF (runtime, not needed for building)

### Install build dependencies

```bash
# Debian/Ubuntu
sudo apt-get install -y protobuf-compiler

# Runtime (needed to actually run VMs)
sudo apt-get install -y qemu-system-x86 ovmf cloud-image-utils
```

## Build

```bash
cargo build
```

Release build with optimizations:

```bash
cargo build --release
```

Binaries are placed in `target/debug/` (or `target/release/`):
- `stratusd` — the daemon
- `stratus` — the CLI

## Run

```bash
# Start the daemon (requires root for /run/stratus/)
sudo target/debug/stratusd

# In another terminal, check status
target/debug/stratus status
```

Or with debug logging:

```bash
RUST_LOG=debug sudo target/debug/stratusd
```

### systemd

```bash
sudo cp systemd/stratusd.service systemd/stratusd.socket /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now stratusd.socket
```

## Test

```bash
# Run all tests (no privileges needed)
cargo test

# Run tests for a specific crate
cargo test -p stratusd
cargo test -p stratus-cli

# Run a single test
cargo test -p stratusd daemon_binds_unix_socket
```

## Lint

```bash
cargo fmt --all
cargo clippy --all-targets
```

## Project layout

```
stratus/
├── proto/stratus/v1/       Protocol Buffer definitions (CLI ↔ daemon)
├── crates/
│   ├── stratusd/           Daemon binary
│   ├── stratus-cli/        CLI binary
│   ├── stratus-store/      State persistence (redb)
│   ├── stratus-vm/         QEMU/QMP management
│   ├── stratus-net/        eBPF networking
│   ├── stratus-services/   DHCP, DNS, IMDS
│   ├── stratus-images/     Image download and cache
│   └── stratus-resources/  Resource types, YAML parsing, validation
├── bpf/                    eBPF C programs
├── systemd/                systemd unit files
└── tests/                  Integration and E2E tests
```

## License

See [LICENSE](LICENSE).
