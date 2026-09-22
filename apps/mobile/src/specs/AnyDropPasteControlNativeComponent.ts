import {
  codegenNativeComponent,
  type CodegenTypes,
  type ColorValue,
  type HostComponent,
  type ViewProps,
} from 'react-native'

interface NativeProps extends ViewProps {
  tint?: ColorValue
  foreground?: ColorValue
  onPaste?: CodegenTypes.DirectEventHandler<Readonly<{ text: string }>>
}

export default codegenNativeComponent<NativeProps>('AnyDropPasteControl', {
  excludedPlatforms: ['android'],
}) as HostComponent<NativeProps>
