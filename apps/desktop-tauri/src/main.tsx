import React from 'react'
import ReactDOM from 'react-dom/client'

import '@a1knla/cakeui/style.css'

import App from './App'
import PreviewWindow from './PreviewWindow'
import ReceivePopup from './ReceivePopup'
import './styles.scss'
import { windowKind } from './types'

// One HTML entry, three surfaces. The native receive popup and the preview
// window load `index.html?window=...`; the main window has no query param.
function Root() {
  switch (windowKind()) {
    case 'receive':
      return <ReceivePopup />
    case 'preview':
      return <PreviewWindow />
    default:
      return <App />
  }
}

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(<Root />)
