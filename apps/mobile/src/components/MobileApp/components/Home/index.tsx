import { useEffect, useState } from 'react'
import { Keyboard, KeyboardAvoidingView, Platform, ScrollView, StatusBar, StyleSheet, Text, View } from 'react-native'
import { SafeAreaView } from 'react-native-safe-area-context'
import { Button, Card, CheckBox, Dialog, Separator, TextBox, useCakeTheme } from '@a1knla/cakeui-native'
import NativeAnyDrop from '../../../../specs/NativeAnyDrop'
import AnyDropPasteControl from '../../../../specs/AnyDropPasteControlNativeComponent'
import { parseGroup, type Snapshot } from '../../../../model'

interface HomeProps {
  snapshot?: Snapshot
  update: (value: Snapshot) => void
  error: string
  setError: (value: string) => void
}
export function Home({ snapshot, update, error, setError }: HomeProps) {
  const { colors, dark } = useCakeTheme()
  const [page, setPage] = useState<'nearby' | 'settings'>('nearby')
  const [text, setText] = useState('')
  const [group, setGroup] = useState('0')
  const [name, setName] = useState('')
  const [busy, setBusy] = useState(false)
  const storedGroup = snapshot?.group
  const storedName = snapshot?.displayName
  useEffect(() => {
    if (storedGroup !== undefined) setGroup(String(storedGroup))
    if (storedName !== undefined) setName(storedName)
  }, [storedGroup, storedName])
  async function run(operation: () => Promise<string | void>, changesSnapshot = true) {
    if (busy) return
    setBusy(true)
    try {
      const result = await operation()
      if (changesSnapshot && result) update(JSON.parse(result) as Snapshot)
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure))
    } finally {
      setBusy(false)
    }
  }
  async function save() {
    if (!snapshot) return
    await run(() =>
      NativeAnyDrop.configure(
        JSON.stringify({
          group: parseGroup(group),
          displayName: name.trim(),
          mode: snapshot.mode,
          receiveText: snapshot.receiveText,
        })
      )
    )
    Keyboard.dismiss()
  }
  async function changeSetting(key: 'mode' | 'receiveText', value: string | boolean) {
    if (snapshot)
      await run(() =>
        NativeAnyDrop.configure(
          JSON.stringify({
            group: snapshot.group,
            displayName: snapshot.displayName,
            mode: snapshot.mode,
            receiveText: snapshot.receiveText,
            [key]: value,
          })
        )
      )
  }
  const label = [styles.label, { color: colors.text }]
  const muted = [styles.body, { color: colors.muted }]
  return (
    <SafeAreaView style={[styles.safe, { backgroundColor: colors.background }]} edges={['top', 'bottom']}>
      <StatusBar barStyle={dark ? 'light-content' : 'dark-content'} />
      <View style={styles.header}>
        <View style={styles.heading}>
          <View style={[styles.dot, { backgroundColor: snapshot?.running ? colors.accent : colors.muted }]} />
          <View>
            <Text style={label}>{snapshot?.running ? '服务运行中' : '服务未开启'}</Text>
            <Text style={muted}>频段 {snapshot?.group ?? '—'}</Text>
          </View>
        </View>
        <Button
          loading={busy}
          disabled={!snapshot}
          onPress={() => {
            void run(() => (snapshot?.running ? NativeAnyDrop.stop() : NativeAnyDrop.start()))
          }}
        >
          {snapshot?.running ? '停止' : '开启'}
        </Button>
      </View>
      <KeyboardAvoidingView style={styles.main} behavior={Platform.OS === 'ios' ? 'padding' : undefined}>
        <ScrollView
          contentContainerStyle={styles.content}
          keyboardShouldPersistTaps="handled"
          keyboardDismissMode="on-drag"
        >
          {page === 'nearby' ? (
            <>
              <View style={styles.sectionHeading}>
                <Text accessibilityRole="header" style={label}>
                  设备
                </Text>
                <Text style={muted}>{snapshot?.peers.length ?? 0} 台</Text>
              </View>
              <Card>
                {snapshot?.peers.length ? (
                  snapshot.peers.map((peer) => (
                    <View key={peer.id + peer.host} style={styles.peer}>
                      <View style={styles.peerDescription}>
                        <Text style={label}>{peer.name}</Text>
                        <Text selectable style={muted}>
                          {peer.host}
                        </Text>
                      </View>
                      <Text style={[styles.caption, { color: colors.accent }]}>同频段</Text>
                    </View>
                  ))
                ) : (
                  <Text style={muted}>
                    {snapshot?.running ? '等待同一局域网、同频段的设备出现。' : '开启服务后发现附近设备。'}
                  </Text>
                )}
              </Card>
              <Text accessibilityRole="header" style={label}>
                剪贴板
              </Text>
              <Card>
                <TextBox
                  accessibilityLabel="发送文本"
                  multiline
                  textAlignVertical="top"
                  style={styles.editor}
                  placeholder="输入或粘贴文本"
                  value={text}
                  onChangeText={setText}
                />
                <View style={styles.actions}>
                  {Platform.OS === 'ios' ? (
                    <AnyDropPasteControl
                      tint={colors.popup}
                      foreground={colors.text}
                      style={[styles.action, styles.paste]}
                      onPaste={(event) => setText(event.nativeEvent.text)}
                    />
                  ) : (
                    <Button
                      style={styles.action}
                      disabled={busy}
                      onPress={() => {
                        void run(async () => {
                          setText(await NativeAnyDrop.getClipboard())
                        }, false)
                      }}
                    >
                      粘贴
                    </Button>
                  )}
                  <Button
                    style={styles.action}
                    variant="primary"
                    disabled={busy || !snapshot?.running || !snapshot.peers.length || !text.length}
                    onPress={() => {
                      void run(() => NativeAnyDrop.sendText(text))
                    }}
                  >
                    同频段群发
                  </Button>
                </View>
                {!!snapshot?.status && (
                  <Text accessibilityLiveRegion="polite" style={muted}>
                    {snapshot.status}
                  </Text>
                )}
              </Card>
              {!!snapshot?.lastReceived && (
                <Card>
                  <Text style={muted}>收到的文本</Text>
                  <Text selectable style={label}>
                    {snapshot.lastReceived}
                  </Text>
                  <Button
                    onPress={() => {
                      void run(() => NativeAnyDrop.copyText(snapshot.lastReceived), false)
                    }}
                  >
                    复制
                  </Button>
                </Card>
              )}
            </>
          ) : (
            <>
              <Text accessibilityRole="header" style={label}>
                设置
              </Text>
              <Card>
                <Text style={muted}>本机外显名</Text>
                <TextBox accessibilityLabel="本机外显名" value={name} onChangeText={setName} maxLength={48} />
                <Text style={muted}>频段</Text>
                <TextBox
                  accessibilityLabel="频段"
                  keyboardType="number-pad"
                  value={group}
                  onChangeText={setGroup}
                  maxLength={10}
                />
                <Button
                  variant="primary"
                  loading={busy}
                  onPress={() => {
                    void save()
                  }}
                >
                  保存设置
                </Button>
              </Card>
              <Card>
                <CheckBox
                  disabled={busy}
                  checked={snapshot?.receiveText ?? true}
                  onValueChange={(value) => {
                    void changeSetting('receiveText', value)
                  }}
                >
                  接收同频段文本
                </CheckBox>
                <Separator />
                <Text style={muted}>外观</Text>
                <View style={styles.actions}>
                  {(['system', 'light', 'dark'] as const).map((mode, index) => (
                    <Button
                      key={mode}
                      disabled={busy}
                      variant={snapshot?.mode === mode ? 'primary' : 'default'}
                      style={styles.action}
                      onPress={() => {
                        void changeSetting('mode', mode)
                      }}
                    >
                      {['系统', '浅色', '深色'][index]}
                    </Button>
                  ))}
                </View>
              </Card>
            </>
          )}
        </ScrollView>
      </KeyboardAvoidingView>
      <View style={[styles.navigation, { borderTopColor: colors.line }]}>
        <Button
          style={styles.action}
          variant={page === 'nearby' ? 'primary' : 'ghost'}
          onPress={() => setPage('nearby')}
        >
          附近
        </Button>
        <Button
          style={styles.action}
          variant={page === 'settings' ? 'primary' : 'ghost'}
          onPress={() => setPage('settings')}
        >
          设置
        </Button>
      </View>
      <Dialog visible={!!error} title="操作未完成" onRequestClose={() => setError('')}>
        <Text selectable style={label}>
          {error}
        </Text>
        <Button onPress={() => setError('')}>关闭</Button>
      </Dialog>
    </SafeAreaView>
  )
}
const styles = StyleSheet.create({
  safe: { flex: 1 },
  main: { flex: 1 },
  header: {
    paddingHorizontal: 20,
    paddingVertical: 14,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
  },
  heading: { flexDirection: 'row', gap: 10, alignItems: 'center' },
  dot: { width: 8, height: 8, borderRadius: 4 },
  content: { paddingHorizontal: 20, paddingTop: 12, paddingBottom: 28, gap: 16 },
  label: { fontSize: 16, lineHeight: 23, fontWeight: '500' },
  body: { fontSize: 13, lineHeight: 20 },
  caption: { fontSize: 12 },
  sectionHeading: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between' },
  peer: { minHeight: 48, flexDirection: 'row', gap: 12, alignItems: 'center' },
  peerDescription: { flex: 1, gap: 3 },
  editor: { minHeight: 136 },
  actions: { flexDirection: 'row', gap: 10 },
  action: { flex: 1 },
  paste: { height: 46 },
  navigation: {
    borderTopWidth: StyleSheet.hairlineWidth,
    flexDirection: 'row',
    gap: 12,
    paddingHorizontal: 20,
    paddingTop: 12,
    paddingBottom: 8,
  },
})
