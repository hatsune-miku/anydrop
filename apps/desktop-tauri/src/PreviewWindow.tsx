import { useEffect, useState } from 'react'

import { FileText, X } from 'lucide-react'

import { convertFileSrc, invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'

type PreviewPayload = {
  path: string
  kind: 'image' | 'audio' | 'video' | 'other'
  name: string
}

function extOf(name: string): string {
  const dot = name.lastIndexOf('.')
  return dot >= 0 ? name.slice(dot + 1).toUpperCase() : ''
}

async function close() {
  // Hand focus back to the main window before closing, so the user isn't left
  // on the desktop / another app.
  try {
    await invoke('focus_main_window')
  } catch {}
  void getCurrentWindow().close()
}

export default function PreviewWindow() {
  const [payload, setPayload] = useState<PreviewPayload | null>(null)

  useEffect(() => {
    // Follow the OS theme for the chrome (the stage itself is always dark).
    document.documentElement.classList.toggle(
      'dark',
      window.matchMedia?.('(prefers-color-scheme: dark)').matches ?? false
    )

    let cancelled = false
    // The payload is set in the backend before this window is built, so the
    // first read normally succeeds. Retry a few times anyway to absorb any
    // mount/build race rather than getting stuck on a blank stage.
    let tries = 0
    const poll = () => {
      if (cancelled) return
      void invoke<PreviewPayload | null>('get_preview_payload')
        .then((p) => {
          if (cancelled) return
          if (p) setPayload(p)
          else if (tries++ < 20) setTimeout(poll, 100)
        })
        .catch(() => {
          if (!cancelled && tries++ < 20) setTimeout(poll, 100)
        })
    }
    poll()

    const unlisten = listen<PreviewPayload>('preview-load', (event) => setPayload(event.payload))
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') close()
    }
    window.addEventListener('keydown', onKey)
    return () => {
      cancelled = true
      window.removeEventListener('keydown', onKey)
      void unlisten.then((u) => u())
    }
  }, [])

  const src = payload ? convertFileSrc(payload.path) : ''

  return (
    <div className="preview-shell">
      <header className="preview-titlebar" data-tauri-drag-region>
        <span data-tauri-drag-region>{payload?.name ?? '速览'}</span>
        <button className="popup-close" type="button" aria-label="关闭" onClick={close}>
          <X size={14} />
        </button>
      </header>

      <div className="preview-stage">
        {!payload ? (
          <div className="preview-generic">加载中…</div>
        ) : payload.kind === 'image' ? (
          <img className="preview-media" src={src} alt={payload.name} />
        ) : payload.kind === 'video' ? (
          // eslint-disable-next-line jsx-a11y/media-has-caption
          <video className="preview-media" src={src} controls autoPlay />
        ) : payload.kind === 'audio' ? (
          <div className="preview-audio">
            <FileText size={48} strokeWidth={1.2} />
            {/* eslint-disable-next-line jsx-a11y/media-has-caption */}
            <audio src={src} controls autoPlay />
          </div>
        ) : (
          <div className="preview-generic">
            <FileText size={56} strokeWidth={1.1} />
            <strong>{payload.name}</strong>
            <span>{extOf(payload.name) || '文件'} · 此类型不支持预览</span>
          </div>
        )}
      </div>

      <footer className="preview-footer">
        <span title={payload?.path}>{payload?.path ?? ''}</span>
        <small>Esc 关闭</small>
      </footer>
    </div>
  )
}
