import { useEffect, useMemo, useState } from 'react'

import { createThemeConfig, updateThemeConfig, validateThemeConfig } from '@a1knla/cakeui/theme'

import type { AppearanceSettings } from '../types'

export function useAppearance(settings: AppearanceSettings) {
  const { appearanceConfig, appearanceTheme, darkMode: savedDarkMode } = settings
  const config = useMemo(() => {
    const saved = validateThemeConfig(appearanceConfig)
    // Existing fields remain the active scheme/mode controls for the main
    // window and local API. CakeUI retains all other scheme options.
    return updateThemeConfig(saved.valid ? saved.config : createThemeConfig(), {
      scheme: appearanceTheme ?? 'pink',
      mode: savedDarkMode == null ? 'system' : savedDarkMode ? 'dark' : 'light',
    })
  }, [appearanceConfig, appearanceTheme, savedDarkMode])
  const [systemDark, setSystemDark] = useState(window.matchMedia?.('(prefers-color-scheme: dark)').matches ?? false)
  useEffect(() => {
    const media = window.matchMedia('(prefers-color-scheme: dark)')
    function changed(event: MediaQueryListEvent) {
      setSystemDark(event.matches)
    }
    media.addEventListener('change', changed)
    return () => media.removeEventListener('change', changed)
  }, [])
  const darkMode = savedDarkMode ?? systemDark
  useEffect(() => {
    document.documentElement.classList.toggle('dark', darkMode)
  }, [darkMode])
  return {
    darkMode,
    provider: {
      defaultConfig: config,
      density: settings.appearanceDensity ?? 'compact',
    },
  }
}
