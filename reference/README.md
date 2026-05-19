# Legacy Reference

The projects in this directory are intentionally not included in the AnyDrop build:

- `windows-winui/`: former WinUI 3 client.
- `macos-swiftui/`: former SwiftUI client.

They were imported without their original Git metadata so the new repository can keep useful migration context while moving to a Rust core plus Tauri desktop architecture. Backend, login, Google Sign-In, and WebSocket code in these directories is historical reference only.
