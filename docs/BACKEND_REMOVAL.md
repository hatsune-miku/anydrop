# Backend Removal Notes

The new AnyDrop tree does not build or call `airx-backend`.

Removed from the active architecture:

- account login and token renewal
- Google Sign-In helpers
- Cloud/WebSocket clipboard relay
- UID-based peer routing
- internet clipboard settings

Kept in the active architecture:

- LAN discovery
- LAN peer list
- LAN text/clipboard broadcast
- LAN file transfer protocol

The legacy WinUI and SwiftUI trees still contain backend code as migration reference, but they are outside the build graph and should not receive new features.

