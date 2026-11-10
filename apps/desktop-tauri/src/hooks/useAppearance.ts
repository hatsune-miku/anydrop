import { useEffect, useState } from 'react'

import type { CakeMode } from '@a1knla/cakeui'

import type { AppearanceSettings } from '../types'

export function useAppearance(settings: AppearanceSettings) {
  const [systemDark, setSystemDark] = useState(window.matchMedia?.('(prefers-color-scheme: dark)').matches ?? false)
  useEffect(() => {
    const media = window.matchMedia('(prefers-color-scheme: dark)')
    function changed(event: MediaQueryListEvent) {
      setSystemDark(event.matches)
    }
    media.addEventListener('change', changed)
    return () => media.removeEventListener('change', changed)
  }, [])
  const darkMode = settings.darkMode ?? systemDark
  useEffect(() => {
    document.documentElement.classList.toggle('dark', darkMode)
  }, [darkMode])
  return {
    darkMode,
    provider: {
      theme: settings.appearanceTheme ?? 'pink',
      density: settings.appearanceDensity ?? 'compact',
      mode: (darkMode ? 'dark' : 'light') as CakeMode,
    },
  }
}
