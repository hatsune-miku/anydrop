# AnyDrop 移动端技术方案（首版范围已确认）

更新日期：2026-09-19。状态：D01–D14 已由维护者确认；已创建 apps/mobile 原生工程，P0 开发中。本文件以维护者的最新选择为准，替代此前配对、特权增强、先 Android 后 iOS、云端及内测发布渠道的建议。

依据：维护者提供的 `/Users/miku/Downloads/cross_platform_airdrop_clipboard_architecture.md`、当前 AnyDrop / CakeUI 源码及本次范围确认。详细状态见 [实施问题清单](mobile-implementation-questions.md)。

## 确定的交付目标

| 项目 | 首版约定 |
| --- | --- |
| 技术栈 | React Native + TypeScript，新架构与 Codegen；Kotlin / Swift 实现原生能力 |
| 系统与设备 | Android 15+、iOS 18+；仅手机，当前验收基线为 Android 15 与 iPhone / iOS 18 Simulator |
| 交付节奏 | 两端同一阶段交付；Android 产出可安装 APK，iOS 工程和应用在 Simulator 运行 |
| 项目性质 | 个人业余项目，无人员、专用真机或固定工期的前置要求 |
| 传输内容 | 单文件 / 多文件、纯文本；不做目录传输、剪贴板图片或富文本 |
| 网络与身份 | 仅局域网，按频段互通，无账号、配对、可信设备列表或云端服务 |
| 发送对象 | 文件由用户选择具体设备；剪贴板纯文本向当前可达的同频段设备群发 |
| Android 能力 | 仅普通权限；不实现 Shizuku、root、LSPosed 或其他特权增强 |
| 系统入口 | 应用内操作 + 通知 + Widget；首版不做接收外部分享的扩展、用户快捷指令动作或快捷设置磁贴 |
| CakeUI | RN 基础组件子集，样式以 AnyDrop 为准；不移植完整 Web 组件库 |
| 存储 | 接收的原始文件优先保存在 App 持久数据目录；支持完整导出；图片、视频同时导入相册 |
| 数据保留 | 不按时间自动删除已接收文件；不做剪贴板历史库或历史保留天数功能；必要任务元信息服务于恢复和文件管理 |
| 分发 | 不要求 TestFlight、商店账号或上架流程；只采用公开 API 和正常权限路径，为未来上架保留条件 |
| 桌面联动 | 接受要求桌面升级；桌面补充新协议，保留原有桌面能力；自动更新继续暂缓 |

“Android 15 / iOS 18”按最低支持版本及首轮验证基线落地；不扩展到平板、旧系统兼容或所有新系统 / ROM 的验证承诺。

## 产品语义

### 频段与接收

频段采用与桌面一致的数值及发现筛选规则。同频段设备可以互相发现和交换纯文本，设备名和稳定 device ID 用于展示、去重和任务路由，不作为认证。频段是公开的筛选条件，不是密码；首版不声称已认证对方身份或提供端到端保密。

文件沿用现有桌面的接收确认：发送者选择目标并提供文件列表，接收者接受后开始拉取。这是单次文件操作确认，不建立配对或持久信任。剪贴板不逐条请求配对或选人；用户开启接收后，按各平台当前可用能力接收同频段群发。

“群发”指向当前可达的同频段接收者分发同一事件，不要求使用 UDP 大包广播。用 origin ID、message ID 和去重机制防止多设备回环；离线设备不补发旧剪贴板。不为了群发保存长期正文历史。

### 文件与相册

图片、视频属于普通文件，可以多选发送；它们与“不做图片剪贴板”不冲突。接收时先把完整原文件保存在持久收件目录，再向系统相册写入受支持的媒体格式。导入失败、格式不受相册支持或权限被拒绝时保留原文件，显示结果并允许重试 / 导出。

完整导出包括单选、多选及全部已接收文件，保留原始字节和可用的原文件名，遇到重名明确处理。通过系统文档导出 / 分享面板交给用户选择目标；这是已有文件的出站导出，不等于把接收外部分享的扩展纳入首版。传输中未完成文件不作为完整文件导出。

Android 使用 MediaStore 添加图片 / 视频，不为本 App 新增媒体申请全相册读取权限；用户选择发送媒体优先使用系统选择器。[Android 媒体存储](https://developer.android.com/training/data-storage/shared/media)

iOS 使用 PhotoKit 的 add-only 授权和用途声明；收到媒体时未授权则在前台解释用途并请求，不强行在后台弹权限框。[Apple 相册授权](https://developer.apple.com/documentation/photokit/delivering-an-enhanced-privacy-experience-in-your-photos-app)

不设置收到后自动删除、保留天数或定期回收已接收内容。取消任务的未完成临时数据和已接收文件分开管理；完成落盘、任务恢复记录和显式删除不能相互混淆。App 私有数据不能被描述为卸载后仍会保留，因此完整导出路径必须可用。

## 平台能力与系统入口

| 能力 | Android 15 普通版 | iOS 18 普通版 |
| --- | --- | --- |
| 发现设备 | 系统 NSD / mDNS；前台服务按实际运行需求维持发现 / 接收 | 前台 Bonjour；不承诺后台持续发现 |
| 发文件 | 选择目标和多文件，发送期间保持前台及原生文件服务可用 | 同左；离开前台按中断 / 恢复处理 |
| 收文件 | 接受后由 connectedDevice FGS 和原生 HTTP 下载任务处理 | 接受后创建原生 background URLSession 下载任务 |
| 发纯文本 | 应用前台明确动作读取；通知 RemoteInput 可输入 / 粘贴并群发 | 应用中由系统 UIPasteControl 接收用户粘贴，再群发 |
| 收纯文本 | 原生后台写入；不可用时通知打开前台再复制 | 前台接收 / 写入；挂起时不保证收到 LAN 新消息 |
| 通知 | 中继服务状态、文件邀请 / 进度 / 完成 / 失败、文本输入和前台复制入口 | App 已获执行机会时的本地事件通知；不提供 APNs，不承诺挂起时收到新 LAN 邀请 |
| Widget | 原生 AppWidget，显示频段 / 最近状态；打开剪贴板发送页、发送文件页或收件页 | 原生 WidgetKit，同样通过链接打开指定页面；不在 Widget 中读取剪贴板或常驻 LAN |

Widget 的状态是最近快照，不能伪装成实时在线证明。Android 打开发送页后在 Activity 获得焦点时处理剪贴板；iOS 点击 Widget 后在 App 内点击系统粘贴控件。Widget 只导航，不通过后台轮询实现“自动同步”。苹果提供 `widgetURL` / `Link` 打开对应 App 场景。[Apple Widget 导航](https://developer.apple.com/documentation/widgetkit/linking-to-specific-app-scenes-from-your-widget-or-live-activity)

首版无需面向用户的 Shortcuts / App Intent 动作。Widget 使用系统所需的原生扩展、配置或生命周期 API，不因此扩展成一套快捷指令功能。

文件传输与剪贴板中继分别管理运行需求：文件完成可以释放本任务资源，但不能误停仍由用户开启的中继；关闭中继也不能误删已接收文件。通知权限拒绝、用户停止前台服务和系统回收都要有明确状态。[Android FGS 类型](https://developer.android.com/develop/background-work/services/fgs/service-types)

## 桌面协议扩展

现有桌面为 UDP / mDNS 发现、TCP 剪贴板、自定义 QUIC 文件传输。移动端不能只复用 Web UI，桌面必须增加兼容的新通道。

1. **能力协商**：分开定义应用版本、协议版本、`filePull` / `clipboardText` 能力与端点。当前源码注释已有 `transfer_v2`，而 ALPN 为 `anydrop/1`，新协议版本应明确定义而不是复用口头命名。
2. **按频段过滤**：发现、offer 和控制消息都检查频段；设备去重不能仅用 IP。改频段后停止旧频段的发现和文本分发；已接受文件任务的处理采用明确规则，避免静默把它变成新频段任务。
3. **接收方拉取**：发送端开放任务限定的文件端点；接收方接受后下载。首版不做目录，manifest 是独立文件列表，包含稳定项 ID、原名、大小和内容版本 / 摘要。
4. **不增加配对**：移除短码、扫码身份确认、可信设备列表和持久配对密钥。每个已接受文件任务仍可以有临时、限文件范围的随机下载凭据，用于限定被分享的文件；它是内部任务控制，不是身份认证或新的用户步骤。
5. **传输方案**：先验证符合当前无配对语义的局域网 HTTP 拉取及 iOS 后台支持；不把配对 HTTPS 作为门槛，也不用跳过证书验证来伪装成可信 HTTPS。iOS 仅配置所需 LAN ATS 例外，Android 使用范围明确的网络配置；不做全网任意 URL 代理。具体本地名称 / IPv4 / IPv6 配置经运行验证后锁定。[Apple 局域网 ATS](https://developer.apple.com/documentation/bundleresources/information-property-list/nsapptransportsecurity/nsallowslocalnetworking)、[地址与域名例外](https://developer.apple.com/documentation/bundleresources/information-property-list/nsapptransportsecurity/nsexceptiondomains)
6. **完整性与恢复**：Range、Content-Range、ETag / 内容版本、落盘字节和摘要一致；暂停、取消、超时及完成回执幂等；源文件变化使旧断点失效；64 位 ID / 大小在 JS 边界不能丢精度。
7. **输入边界**：无配对不改变对长度、文件名、路径穿越、manifest、内存和并发的检查；发送服务仅能访问本次选定的文件，不暴露整个目录。
8. **桌面兼容**：新桌面保留原有 QUIC / TCP 使用场景，并实现移动所需能力；旧桌面缺少新能力时在移动端提示升级。跨新旧通道的剪贴板回环应纳入桌面回归。

HTTP 不被描述为加密传输；后续若增加其他传输机制，不能擅自重新引入已排除的配对要求。无配对传输的技术验证聚焦可达性、授权范围和数据完整性，不宣称抵御同网段身份冒充。

## RN 与原生模块

使用社区 CLI 管理的 RN 原生工程，Android / iOS 工程入库；版本在创建工程时按 Android 15 / iOS 18 与本机工具链的兼容性锁定。首版不引入 Expo Go 或额外云端构建依赖。

```text
apps/mobile/                    RN 页面、导航、能力说明
  src/specs/NativeAnyDrop.ts         Codegen 命令、事件、快照规范
  android/                      Kotlin NSD、FGS、通知、AppWidget、存储与剪贴板
  ios/                          Swift Bonjour、URLSession、通知、PhotoKit、WidgetKit
apps/mobile/src/model/          初期状态类型；有共享需求时再提取公共包
core/                           桌面协议扩展与跨端协议测试
CakeUI 仓库
  scripts/native-tokens.mjs     同源设计变量生成器
  packages/native/             RN 基础子集、示例与测试
```

原生层拥有网络、任务状态、持久存储和系统回调；RN 负责交互。UI 使用 `discover / offer / accept / pause / resume / cancel / broadcastText / exportFiles` 等类型化命令。进度节流，重新挂载时读取完整快照；文件字节不经过 JS base64。[RN Turbo Native Modules](https://reactnative.dev/docs/turbo-native-modules-introduction)

Android 使用 Kotlin coroutines 和原生流式下载；iOS 使用 Swift + URLSession。数据库保存恢复所需任务与文件索引，不建设剪贴板历史产品。Widget 与宿主只交换必要快照和任务引用，网络任务仍在宿主的原生模块中。

现有 Rust 首先复用协议定义、校验语义和测试向量；不把整个 QUIC / tokio 运行时搬到 iOS 以替代系统后台下载。是否增加薄 FFI 由实现中的实际复用量决定。

## CakeUI RN 基础子集

首批 Provider、Button、Card、TextBox、CheckBox / Switch、ProgressBar、Spinner、Separator、提示 / 确认层；按实际页面需要落实。设备卡片、传输列表、频段设置等业务组件留在 AnyDrop。

倾向在 CakeUI 仓库提供独立原生包，共享结构化 tokens；原生包名、Metro / 类型导出和 React peerDependencies 在小样阶段验证。Web 用户不能被迫安装 RN，当前 Web 0.3.0 的视觉基线必须保持。

原生组件使用 `onPress`、`onChangeText` 等平台 API。色彩以 AnyDrop 为准；字号、触摸范围、键盘避让、安全区和动态字体按手机适配。Android AppWidget 与 iOS WidgetKit 需要原生视图，复用设计变量而非直接挂载 CakeUI React 组件。

## 交付阶段与验收

| 阶段 | 交付 | 验收范围 |
| --- | --- | --- |
| P0 可运行骨架 | RN 两端工程、2–3 个 CakeUI 原生组件、LAN 发现 / 文件拉取小样 | Android 15 模拟环境可运行并构建 APK；iOS 18 Simulator 可运行；记录环境缺口，不要求先购置真机或商店账号 |
| P1 互通协议 | 频段、能力协商、桌面拉取端点、纯文本群发、接受 / 取消 / 恢复 | 桌面 ↔ 移动可实际互传，完成文件内容校验、重复消息与回环检查 |
| P2 首版功能 | 两端多文件、文本、通知、Widget、收件箱、完整导出、媒体进相册 | 所有首版功能在各自可用环境验证；拒绝权限和失败有恢复入口 |
| P3 同阶段交付 | Android APK + iOS Simulator 构建 / 启动说明，配套桌面兼容版本 | 从干净构建到启动 / 传输 / 导出可复现；标记测试环境及未验证的真机行为 |

不设 Android 先发布或 iOS 真机认证后才开始的串行门槛。开发时先让 iOS 启动，但同时交付的目标不变。无固定人周或日期承诺。

iOS Simulator 可以验证界面、原生桥接和可用的网络 / 存储流程，不能替代真实设备的锁屏、挂起、后台调度或 OEM 行为证据。真机测试是将来可补充的验证，不是本次交付阻塞项。iOS 强退后的系统行为按官方限制处理，不能声称强退后仍继续接收。[Apple 后台 session](https://developer.apple.com/documentation/foundation/urlsessionconfiguration/background(withidentifier:))

只使用公开 API、正常通知 / 相册 / LAN 权限和系统扩展机制。不采用私有 API、特权读取或伪装后台用途的保活手段；这保留未来上架的技术条件，但不等于已经完成商店审核。Android minSdk 与 targetSdk 分开管理，targetSdk 提升时根据官方规则补充对应权限适配。[Android LAN 权限](https://developer.android.com/privacy-and-security/local-network-permission)

## 当前不需要的资源

不要求提供团队、截止日期、专用真机、APNs 服务、域名、配对密钥、Apple Developer 付费账号或 TestFlight 资料。实现阶段可选定开发应用标识；Android 的持久签名方式、SDK / Simulator runtime 缺口在实际构建时处理，不再作为一轮产品问卷。

工程和首个原生文本 / 发现增量已开始实现，完成状态与环境差异见 [开发记录](mobile-development.md)。
