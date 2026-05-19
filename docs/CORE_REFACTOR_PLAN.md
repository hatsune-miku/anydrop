# Core Refactor Plan

## Stability And Robustness

1. Replace ad-hoc thread spawning with owned service handles. Each service should expose `start`, `stop`, `join`, and a bounded shutdown timeout.
2. Return typed errors with `thiserror` instead of stringly `io::ErrorKind::Other` wrappers.
3. Add frame limits for all incoming packets. Discovery and data services should reject oversized payloads before allocating buffers.
4. Replace callback-only APIs with event channels. Desktop shells can subscribe to events without sharing callback state across network threads.
5. Add integration tests for two local service instances using random ports, covering discovery, text transfer, file accept, file reject, and interrupted shutdown.
6. Make transfer sessions resumable by file id plus content hash, not only by offset.

## LAN Discovery

1. Stop sending packet size and packet body as two UDP datagrams. UDP datagrams are already message-framed; split sends can reorder or drop independently.
2. Enumerate interface broadcast addresses with netmask data instead of filtering for `255.255.255.255`, which misses normal subnet broadcasts such as `192.168.1.255`.
3. Add peer expiry with `last_seen` timestamps so stale peers disappear automatically.
4. Include protocol version, device id, instance id, hostname, OS, and service port in one signed discovery payload.
5. Add mDNS/DNS-SD as an optional discovery backend for networks that suppress broadcast.
6. Rate-limit discovery bursts and use jitter to avoid synchronized broadcasts on busy LANs.

## Android NDK Removal

1. Keep the core crate focused on desktop/server-safe Rust. Android JNI dependencies have been removed from default and active features.
2. Delete JNI bridge maintenance from release scripts.
3. Treat any future Android work as a separate adapter crate, not a feature inside the core crate.

## API Shape

Target modules:

- `anydrop-core`: protocol, discovery, transfer, typed events.
- `anydrop-desktop-tauri`: Tauri UI, clipboard, settings, tray, notifications.

This keeps platform concerns out of the network core and makes the app easier to test.
