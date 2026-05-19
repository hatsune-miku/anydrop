## libanydrop

`libanydrop` is the Rust core for AnyDrop. It provides LAN discovery and LAN data transmission for desktop clients.

### Features

- LAN discovery with group identity
- UTF-8 text transfer over LAN
- file transfer over LAN
- Rust APIs for the Tauri desktop shell

### Usage

```shell
cargo test -p anydrop
cargo build -p anydrop --release
```

Android/JNI support was removed from the active core. Future Android work should live in a separate adapter crate.
