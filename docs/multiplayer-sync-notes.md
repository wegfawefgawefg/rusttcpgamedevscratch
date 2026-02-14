# Multiplayer Sync Notes

This document captures the current approach in this repo and how it differs from common real-world multiplayer movement sync.

## What the current prototype does

- Client sends current position (`x`, `y`) frequently.
- Server stores latest position per client and broadcasts updates to other clients.
- Remote clients render the most recent received position immediately.

This is good for learning and quick experiments.

## Known limitations

- Uses TCP for high-frequency movement updates:
  - head-of-line blocking can delay fresh state behind old packets.
- No tick or sequence numbers:
  - cannot reason about staleness or ordering beyond arrival order.
- No interpolation buffer:
  - remote entities jitter/snap when packets arrive unevenly.
- No prediction/reconciliation:
  - not enough for responsive authoritative movement at scale.
- Sends raw position directly:
  - easier to cheat and harder to validate than input-driven simulation.

## Typical production-ish pattern (high level)

1. Fixed server simulation tick (for example 20/30/60 Hz).
2. Clients send input commands with sequence/tick metadata.
3. Server simulates authoritative state from inputs.
4. Server sends periodic snapshots (with tick IDs).
5. Clients render slightly behind real time using interpolation between snapshots.
6. Local player uses prediction + reconciliation.
7. State-stream transport is usually UDP-style unreliable for frequent movement; reliable channel is used for critical events.

## Practical next steps for this repo

1. Add tick IDs to movement messages.
2. Add snapshot timestamps/ticks on server broadcast.
3. Keep a small client interpolation buffer (for example ~100 ms).
4. Render remote players at `render_time = now - interpolation_delay`.
5. Interpolate between bracketing snapshots instead of snapping to latest.
6. Add send-rate cap (for example 20-30 Hz) instead of per-frame spam.

## Interpolation pseudo-flow

```text
on snapshot received:
  buffer[entity].push(snapshot_with_tick)
  drop old snapshots

each render frame:
  target_tick = latest_tick - interpolation_delay_ticks
  find snapshots A and B where A.tick <= target_tick <= B.tick
  alpha = (target_tick - A.tick) / (B.tick - A.tick)
  render_pos = lerp(A.pos, B.pos, alpha)
```

This gives smoother remote motion without changing the entire architecture at once.
