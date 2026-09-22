import type { TurboModule } from 'react-native'
import { TurboModuleRegistry } from 'react-native'

export interface Spec extends TurboModule {
  getSnapshot(): Promise<string>
  configure(settings: string): Promise<string>
  start(): Promise<string>
  stop(): Promise<string>
  sendText(text: string): Promise<string>
  getClipboard(): Promise<string>
  copyText(text: string): Promise<void>
}

export default TurboModuleRegistry.getEnforcing<Spec>('NativeAnyDrop')
