# AnyDrop

AnyDrop is a LAN-first text, clipboard, and file sharing app.

This repository merges the old AirX core, Windows client, and macOS client into one new codebase. The old WinUI and SwiftUI projects are kept under `reference/` only as migration material. New desktop development targets a shared Rust core plus a Tauri 2 application built with TypeScript, React, Vite, and Yarn.

## Layout

- `core/`: Rust LAN discovery and data-transfer library, renamed to `anydrop`.
- `apps/desktop-tauri/`: Tauri 2 desktop shell.
- `reference/windows-winui/`: old Windows client, not part of the build.
- `reference/macos-swiftui/`: old macOS client, not part of the build.
- `docs/`: refactor and migration notes.

## Build

Build the core only:

```powershell
cargo build -p anydrop --release
```

Install the Tauri frontend dependencies:

```powershell
yarn install
```

Run the Tauri app in development:

```powershell
yarn dev
```

Build the frontend only:

```powershell
yarn workspace @anydrop/desktop-tauri build
```

Build the Windows desktop bundle:

```powershell
yarn build
```

The Tauri app links the Rust core directly through the workspace crate dependency.
