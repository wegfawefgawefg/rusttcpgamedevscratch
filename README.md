# rusttcpgamedevscratch

Educational Rust multiplayer sandbox using:
- `raylib` for client rendering/input
- a simple TCP server for state relay
- JSON line messages between client/server

This repo is intentionally a learning project, not production netcode.

## Current status

- `server` binary accepts multiple clients, assigns IDs, tracks latest positions, and broadcasts updates.
- `client` binary opens a Raylib window, moves a local circle, and renders remote circles.
- Remote movement currently snaps to last received state (no interpolation yet).

## Run

1. Start server:
```bash
cargo run --bin server
```

2. Start one or more clients (in separate terminals):
```bash
cargo run --bin client
```

3. Optional custom address:
```bash
cargo run --bin server -- 127.0.0.1:8091
cargo run --bin client -- 127.0.0.1:8091
```

## Project layout

- `src/bin/server.rs`: TCP server, client bookkeeping, position broadcast.
- `src/bin/client.rs`: client entrypoint, launches sketch.
- `src/sketch.rs`: Raylib simulation/render loop + network send/receive integration.
- `src/*_connection_handling*.rs`, `src/dumb_client*.rs`: earlier experiments kept for reference.

## Networking notes

See `docs/multiplayer-sync-notes.md` for a concise explanation of:
- why this prototype works but is naive,
- common multiplayer sync patterns,
- practical next steps for interpolation, ticked snapshots, and prediction.

## Why this repo exists

To iterate quickly on game networking concepts in Rust while keeping the code small and hackable.
