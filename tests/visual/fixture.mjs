// Deterministic UI-only data. No discovery, clipboard, filesystem or network side effects.
export const epoch = 1789812000000
export function snapshot(darkMode = false) {
  return {
    running: true,
    settings: {
      sendClipboardEnabled: true,
      receiveClipboardEnabled: true,
      sendOnlyOnDoubleCopy: false,
      groupIdentity: 7,
      discoveryPort: 9818,
      dataPort: 9819,
      displayName: '工作电脑',
      syncImageEnabled: true,
      darkMode,
      defaultSaveDir: '/Users/example/Downloads/AnyDrop',
      clipboardPopupEnabled: true,
      suppressPopupInGame: true,
    },
    peers: [
      { name: 'dev:desktop', label: '书房电脑', hosts: ['192.168.1.12'], remark: '书房' },
      { name: 'dev:laptop', label: 'MacBook (2个地址)', hosts: ['192.168.1.21', '10.0.0.21'] },
    ],
    transfers: [
      {
        key: '103',
        fileId: 0,
        fileName: '项目资料.zip',
        remotePath: '',
        localPath: '',
        peer: '书房电脑',
        host: '192.168.1.12',
        direction: 'outgoing',
        progress: 26214400,
        total: 104857600,
        status: 4,
        speedBps: 5242880,
        createdAt: epoch,
        startedAt: epoch,
      },
      {
        key: '102',
        fileId: 0,
        fileName: '会议录音.m4a',
        remotePath: '',
        localPath: '/Users/example/Downloads/AnyDrop',
        peer: 'MacBook',
        host: '192.168.1.21',
        direction: 'incoming',
        progress: 6291456,
        total: 12582912,
        status: 9,
        speedBps: 0,
        createdAt: epoch - 10000,
      },
      {
        key: '101',
        fileId: 0,
        fileName: '阅读笔记.txt',
        remotePath: '',
        localPath: '/Users/example/Downloads/AnyDrop',
        peer: 'MacBook',
        host: '192.168.1.21',
        direction: 'incoming',
        progress: 2048,
        total: 2048,
        status: 7,
        speedBps: 0,
        createdAt: epoch - 20000,
        startedAt: epoch - 20000,
        completedAt: epoch - 19000,
      },
    ],
    lastClipboardText: '准备共享的文字',
    lastReceivedText: '',
    statusText: 'Online on LAN Group #7',
    logs: [
      '[10:00:00.000] AnyDrop 0.1.1',
      '[10:00:00.010] Proudly Crafted With CakeDesign.',
      '[10:00:00.350] service started (discovery_port=9818 data_port=9819 group=7)',
    ],
  }
}
export function installMock({ snap, label, platform, now, updaterConfigured = false }) {
  Object.defineProperty(navigator, 'platform', { value: platform })
  Object.defineProperty(navigator, 'userAgent', {
    value:
      platform === 'MacIntel'
        ? 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)'
        : 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)',
  })
  Date.now = () => now
  let preferences = {
    autostart: false,
    clipboardShortcut: '',
    shortcutRegistered: false,
    shortcutError: null,
    updaterConfigured,
  }
  const callbacks = new Map()
  const listeners = new Map()
  let id = 0
  window.__testCalls = []
  window.__emit = (event, payload) => {
    for (const [eventId, item] of listeners) {
      if (item.event === event) callbacks.get(item.handler)?.({ event, id: eventId, payload })
    }
  }
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: (_event, eventId) => listeners.delete(eventId) }
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label }, currentWebview: { label } },
    transformCallback: (callback) => {
      callbacks.set(++id, callback)
      return id
    },
    unregisterCallback: (key) => callbacks.delete(key),
    convertFileSrc: () =>
      'data:image/svg+xml,%3Csvg xmlns="http://www.w3.org/2000/svg" width="320" height="200"%3E%3Crect width="320" height="200" fill="%23d15776"/%3E%3C/svg%3E',
    invoke: async (cmd, args = {}) => {
      window.__testCalls.push({ cmd, args })
      if (cmd === 'plugin:event|listen') {
        listeners.set(++id, args)
        return id
      }
      if (cmd === 'plugin:event|unlisten') {
        listeners.delete(args.eventId)
        return
      }
      if (cmd === 'plugin:app|version') return '0.1.1'
      if (cmd === 'plugin:dialog|open') return ['/Users/example/Documents/阅读笔记.txt']
      if (cmd === 'get_preview_payload') return { path: '/example/image.png', kind: 'image', name: '图片.png' }
      if (cmd === 'save_settings') {
        snap.settings = args.settings
        window.__emit('snapshot', snap)
      }
      if (cmd === 'get_desktop_preferences') return structuredClone(preferences)
      if (cmd === 'set_autostart') {
        preferences.autostart = args.enabled
        return structuredClone(preferences)
      }
      if (cmd === 'set_clipboard_shortcut') {
        if (window.__shortcutFailure) throw new Error('快捷键已被占用')
        preferences.clipboardShortcut = args.shortcut
        preferences.shortcutRegistered = Boolean(args.shortcut)
        return structuredClone(preferences)
      }
      if (cmd === 'check_app_update') return { version: '0.1.3', notes: '更新说明：修复传输问题。' }
      if (cmd === 'download_app_update') {
        if (window.__updateDownloadFailure) throw new Error('更新下载或签名校验失败')
        if (window.__holdUpdateDownload)
          await new Promise((resolve) => {
            window.__finishUpdateDownload = resolve
          })
        window.__emit('app-update-progress', { downloaded: 1024, total: 1024 })
        return
      }
      if (cmd === 'install_app_update') return
      if (cmd === 'set_shortcut_recording') return
      if (cmd === 'accept_transfer') {
        snap.transfers = snap.transfers.map((t) => (t.key === args.transferKey ? { ...t, status: 4 } : t))
        window.__emit('snapshot', snap)
      }
      if (
        cmd === 'get_snapshot' ||
        cmd === 'refresh_peers' ||
        cmd === 'push_log' ||
        cmd === 'save_settings' ||
        cmd === 'accept_transfer' ||
        cmd === 'send_paths' ||
        cmd === 'send_clipboard_now'
      )
        return structuredClone(snap)
      if (cmd.startsWith('plugin:window|') || cmd.startsWith('plugin:webview|')) return
      throw new Error('Unhandled visual-test command: ' + cmd)
    },
  }
}
