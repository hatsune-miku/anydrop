# Local MCP and HTTP API

The desktop app automatically starts these interfaces:

- MCP (Streamable HTTP): `http://localhost:26937/mcp`
- HTTP API and operation catalogue: `http://localhost:26937/api`
- OpenAPI 3.1: `http://localhost:26937/api/openapi.json`

They operate on the running app's settings, peers and transfers. The local server remains available after `stop_service`; exiting AnyDrop stops it. It binds only to IPv4/IPv6 loopback, validates Host and Origin, and requires no token. If the port is occupied, the app reports the error in its log and retries every five seconds.

## MCP

Add a Streamable HTTP server with URL `http://localhost:26937/mcp` in your MCP client. The server supports initialization, `tools/list`, `tools/call` and cancellation through the MCP Rust SDK. Tool names and argument schemas match the HTTP catalogue below. Successful results expose `{ "ok": true, "result": ... }` in `structuredContent`; operation failures set `isError` and return `{ "ok": false, "error": { "code": "...", "message": "..." } }`.

## HTTP

Use `POST /api/<operation>` with the operation arguments as JSON, or `POST /api` with `{ "method": "<operation>", "params": {...} }`. Operations without arguments accept `{}`.

```sh
curl http://localhost:26937/api

curl http://localhost:26937/api/get_snapshot \
  -H 'Content-Type: application/json' -d '{}'

curl http://localhost:26937/api \
  -H 'Content-Type: application/json' \
  -d '{"method":"save_settings","params":{"settings":{"darkMode":null}}}'
```

Success returns `{ "ok": true, "result": ... }`. Operation errors return `{ "ok": false, "error": { "code": "...", "message": "..." } }`: HTTP 400 for invalid arguments, 404 for unknown operations, 409 for operations that cannot complete, or 500 for internal failures. Malformed HTTP/JSON can also be rejected by the transport before dispatch.

| GUI operation | API / MCP names |
| --- | --- |
| App, peers and service | `get_app_info`, `get_snapshot`, `refresh_peers`, `start_service`, `stop_service` |
| Settings and nicknames | `save_settings`, `set_peer_remark` |
| Send clipboard, text, files or folders | `send_clipboard_now`, `send_text_to_peer`, `send_paths` |
| Pending send confirmations | `get_outgoing_requests`, `confirm_outgoing_request`, `dismiss_outgoing_request` |
| Transfer controls | `accept_transfer`, `reject_transfer`, `cancel_transfer`, `pause_transfer`, `resume_transfer` |
| History and logs | `dismiss_transfer`, `clear_transfers`, `clear_logs` |
| Clipboard, folders and preview | `copy_text`, `open_transfer_folder`, `open_directory`, `preview_file` |
| Desktop integration | `get_desktop_preferences`, `set_autostart`, `set_auto_receive_files`, `set_file_context_menu`, `set_clipboard_shortcut` |
| Updates | `check_app_update`, `download_app_update`, `install_app_update` |
| Windows, popup cards and exit | `control_window`, `dismiss_popup`, `quit_app` |
| Incoming events | `get_event_cursor`, `wait_for_file`, `wait_for_clipboard_text` |

Read `GET /api` for complete input schemas. `save_settings` accepts a partial `settings` object; omitted fields retain their values and `darkMode: null` restores the system theme. File and directory arguments must be absolute local paths, replacing GUI pickers. Use one peer's `hosts` from `get_snapshot` for `send_paths` and `send_text_to_peer`; pending confirmations use its `peerName`. `send_paths` returns the new `transferKey` and a snapshot.

`dismiss_popup` with no arguments dismisses informational cards while retaining pending send/receive confirmations; it also accepts a clipboard `receiptId` or completed `transferKey`. `control_window` accepts `window` (`main`, `receive`, `preview`) and `action` (`show`, `hide`, `minimize`, `toggle_maximize`, `close`). Closing the main window hides it as in the GUI.

## Waiting for files and clipboard text

`wait_for_file` defaults to **completed reception** and returns local paths. It does not accept offers or change auto-receive preferences. Accept through the GUI, `accept_transfer`, or the existing auto-receive setting.

```sh
curl http://localhost:26937/api/wait_for_file \
  -H 'Content-Type: application/json' -d '{"timeoutMs":60000}'

curl http://localhost:26937/api/wait_for_clipboard_text \
  -H 'Content-Type: application/json' -d '{"timeoutMs":60000}'
```

Example completed-file response:

```json
{
  "ok": true,
  "result": {
    "timedOut": false,
    "cursor": "session:2",
    "event": {
      "cursor": "session:2",
      "kind": "file_received",
      "receivedAt": 1793170000000,
      "data": {
        "transferKey": "123",
        "peer": "Other device@192.168.1.2:24680",
        "fileName": "2 items",
        "localPath": "/Users/me/Downloads/report.txt",
        "paths": ["/Users/me/Downloads/report.txt", "/Users/me/Downloads/photos"],
        "saveDir": "/Users/me/Downloads",
        "bytes": 4096
      }
    }
  }
}
```

`paths` contains each selected top-level file or folder; `localPath` is the GUI reveal target. Clipboard events have kind `clipboard_text` and data `{ "text": "complete received text", "peer": "..." }`. Only text received from another device satisfies that wait, with receive-clipboard enabled; local copies do not.

To decide whether to receive a file programmatically:

1. Call `get_event_cursor` and save its `cursor`.
2. Call `wait_for_file` with `{ "stage": "offered", "after": "<cursor>" }`.
3. Inspect `transferKey`, `entries`, `peer` and `bytes`, then call `accept_transfer` with `{ "transferKey": "...", "saveDir": "/absolute/destination" }` or `reject_transfer`.
4. Call `wait_for_file` with `{ "after": "<offer cursor>", "transferKey": "..." }`. Completion is returned even if it occurred between steps 3 and 4.

Both waits accept `after`, `timeoutMs` and an exact `peer` filter; file waits also accept `transferKey` and `stage` (`completed` by default, or `offered`). Without `after`, only events arriving after the call starts qualify. Timeout defaults to 60 seconds; `0` waits indefinitely; the maximum nonzero value is 24 hours. Set the client request timeout accordingly.

A timeout returns `{ "timedOut": true, "cursor": "...", "event": null }`. Pass this cursor as `after` when retrying the same wait to avoid gaps. Clients do not consume events; repeated identical received texts remain distinct. The in-memory journal holds up to 128 events / 16 MiB, with up to 64 concurrent waits. Cursors expire explicitly after app restart or history eviction; obtain a new cursor in those cases.

## Verification

`cargo test -p anydrop-desktop-tauri --lib` includes loopback HTTP and MCP SDK client tests for schemas, errors, Host/Origin checks, concurrent waits, replay, timeouts and cancellation.

The native probe receives a real multi-selection QUIC transfer and verifies its contents and returned paths, settings changes and LAN service stop/start:

```sh
yarn workspace @anydrop/desktop-tauri build
cargo build -p anydrop-desktop-tauri --features tauri/custom-protocol
cargo build -p anydrop --example desktop_file_probe
node scripts/local-api-check.mjs
```

It uses temporary configuration and ports, disables clipboard synchronization and file-menu registration, and writes reports under `test-results/local-api/`.
