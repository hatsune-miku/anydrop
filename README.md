# AnyDrop

AnyDrop is a LAN-first text, clipboard, and file sharing app.

AnyDrop evolved from AirX. Desktop development now targets a shared Rust core plus a Tauri 2 application built with TypeScript, React, Vite, and Yarn. Former Windows and macOS clients are retained under `reference/` as historical source only; retired projects and their external documentation are not required for current development.

## Layout

- `core/`: Rust LAN discovery and data-transfer library, renamed to `anydrop`.
- `apps/desktop-tauri/`: Tauri 2 desktop shell.
- `apps/mobile/`: React Native Android / iOS application in development.
- `reference/windows-winui/`: old Windows client, not part of the build.
- `reference/macos-swiftui/`: old macOS client, not part of the build.
- `docs/`: refactor and migration notes.

## Design Guidance

UI and UX follow [CakeDesign (CD)](https://raw.githubusercontent.com/hatsune-miku/cakedesign-skill/refs/heads/main/SKILL.md), alongside the existing application implementation.

The current AnyDrop project has never used Figma for preliminary design. Figma references in legacy records are not design sources for this project.

AnyDrop is the visual source of truth for CakeUI. The desktop app uses CakeUI 0.3.0 from npm with its calibrated pink / compact theme; visual conflicts are resolved in CakeUI itself. The exact package version and integrity are recorded in `yarn.lock`.

## Current integration notes

- [CakeUI integration and visual verification](docs/cakeui-integration.md)
- [Mobile development and build instructions](apps/mobile/README.md)
- [Confirmed mobile scope: React Native, Android APK, and iOS Simulator](docs/mobile-plan.md)
- [Mobile implementation checklist: confirmed decisions and remaining engineering work](docs/mobile-implementation-questions.md)
- [Desktop autostart, global shortcut and updater configuration](docs/desktop-integration.md)

Desktop updates use independent AnyDrop signatures and the RC endpoint at https://anydrop-api.vanillacake.cn/rc/latest.json. GitHub builds signed installers; the server verifies and mirrors complete releases before publishing the manifest. See [updater deployment](deploy/updater/README.md).

## Build

Build the core only:

```powershell
cargo build -p anydrop --release
```

Install the Tauri frontend dependencies:

```powershell
yarn install
```

Node 25.8.1+ (25.x) uses the same install command. The repository's `.yarnrc` automatically skips engine checks during `yarn install` because the pinned React Native / Metro dependencies exclude Node 25 in their declarations. This project provides a tested compatibility path; CI remains on Node 22. See the [mobile development instructions](apps/mobile/README.md) for details.

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
