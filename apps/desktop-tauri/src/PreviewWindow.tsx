import { useEffect, useState } from 'react'

import { FileText } from 'lucide-react'

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

export default function PreviewWindow() {
  const [payload, setPayload] = useState<PreviewPayload | null>(null)

  useEffect(() => {
    void invoke<PreviewPayload | null>('get_preview_payload')
      .then((p) => p && setPayload(p))
      .catch(() => {})
    const unlisten = listen<PreviewPayload>('preview-load', (event) => setPayload(event.payload))

    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') void getCurrentWindow().close()
    }
    window.addEventListener('keydown', onKey)
    return () => {
      window.removeEventListener('keydown', onKey)
      void unlisten.then((u) => u())
    }
  }, [])

  if (!payload) {
    return <div className="preview-shell preview-shell--empty">加载中…</div>
  }

  const src = convertFileSrc(payload.path)

  return (
    <div className="preview-shell">
      <div className="preview-stage">
        {payload.kind === 'image' ? (
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
            <code title={payload.path}>{payload.path}</code>
          </div>
        )}
      </div>
      <footer className="preview-footer">
        <span title={payload.path}>{payload.name}</span>
        <small>按 Esc 关闭</small>
      </footer>
    </div>
  )
}
