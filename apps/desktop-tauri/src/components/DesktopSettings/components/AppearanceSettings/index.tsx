import { Palette } from 'lucide-react'

import { ComboBox } from '@a1knla/cakeui'

import './index.scss'

import type { AppearanceSettings as AppearanceValues } from '../../../../types'

export default function AppearanceSettings({
  settings,
  disabled,
  onChange,
}: {
  settings: AppearanceValues
  disabled: boolean
  onChange: (change: Partial<AppearanceValues>) => void
}) {
  return (
    <section className="desktop-appearance" aria-labelledby="desktop-appearance-title">
      <div className="desktop-setting-row">
        <Palette className="desktop-setting-icon" size={19} aria-hidden="true" />
        <h3 className="desktop-setting-title" id="desktop-appearance-title">
          外观
        </h3>
      </div>
      <div className="desktop-appearance-fields">
        <div className="desktop-appearance-field">
          <label className="desktop-label" htmlFor="appearance-theme">
            主题色
          </label>
          <ComboBox
            id="appearance-theme"
            value={settings.appearanceTheme ?? 'pink'}
            disabled={disabled}
            onChange={(event) =>
              onChange({ appearanceTheme: event.currentTarget.value as AppearanceValues['appearanceTheme'] })
            }
          >
            <option value="blue">蓝色</option>
            <option value="pink">粉色</option>
            <option value="gold">金色</option>
          </ComboBox>
        </div>
        <div className="desktop-appearance-field">
          <label className="desktop-label" htmlFor="appearance-mode">
            明暗模式
          </label>
          <ComboBox
            id="appearance-mode"
            value={settings.darkMode === null ? 'system' : settings.darkMode ? 'dark' : 'light'}
            disabled={disabled}
            onChange={(event) =>
              onChange({
                darkMode: event.currentTarget.value === 'system' ? null : event.currentTarget.value === 'dark',
              })
            }
          >
            <option value="system">跟随系统</option>
            <option value="light">浅色</option>
            <option value="dark">深色</option>
          </ComboBox>
        </div>
        <div className="desktop-appearance-field">
          <label className="desktop-label" htmlFor="appearance-density">
            界面密度
          </label>
          <ComboBox
            id="appearance-density"
            value={settings.appearanceDensity ?? 'compact'}
            disabled={disabled}
            onChange={(event) =>
              onChange({ appearanceDensity: event.currentTarget.value as AppearanceValues['appearanceDensity'] })
            }
          >
            <option value="compact">紧凑</option>
            <option value="comfortable">舒适</option>
          </ComboBox>
        </div>
      </div>
    </section>
  )
}
