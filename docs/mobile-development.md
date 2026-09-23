# 移动端开发记录

2026-09-19，直接在当前 main 分支开始实现。产品范围以 mobile-plan.md 中 D01–D14 的确认结果为准。

## 已落地的代码

- apps/mobile：社区 CLI 原生工程，RN 0.87.1、React 19.2.3；Android minSdk 35，iOS deployment target 18.0，iPhone 设备族。
- TurboModule / Codegen：原生运行状态快照、配置持久化、开启 / 停止、文本发送与复制。原生层持有网络与设置，JS 在前台读取快照，不持有文件或网络服务生命周期。
- Kotlin NSD 与 Swift Bonjour：沿用 `_anydrop._udp` 服务类型以及 g / dn / did TXT；通过同频段解析结果展示设备。服务类型中的 udp 是既有发现名称，文本载荷仍通过 TCP 发送。
- 文本兼容适配器：长度受限的 TCP 帧、UTF-8 与现有校验规则；接收只允许来自已解析的同频段地址。它仍是旧协议，IP 筛选不是认证，尚未替代规划中的携带频段、origin ID 和 message ID 的新信封。
- iOS Fabric UIPasteControl：用户明确粘贴触发事件，普通模块读取接口在 iOS 拒绝调用。Android 读取检查 Activity 焦点。收到的文本只保留当前进程最新一条，不写正文历史数据库。
- CakeUI 仓库 packages/native：Provider、Button、Card、TextBox、CheckBox、Switch、ProgressBar、Spinner、Separator、Dialog；AnyDrop 安装其本地固定 tarball，颜色由 CakeUI Web 中 AnyDrop 校准后的粉色主题生成。
- 桌面与移动统一 React 19.2.3，避免 workspace 提升依赖产生两份 React；桌面 UI 代码没有因移动端新增而改写。

## 当前边界

这是开发首个增量，P0 的文件拉取验证尚未完成，不是首版全部交付。Android 前台服务、两端通知 / Widget、多文件与导出 / 相册、新桌面协议及任务恢复均未完成。iOS 没有后台常驻能力；Android 当前也不承诺应用退出后持续接收。

旧 TCP 发送结果表示载荷已交给连接，不代表接收方已确认写入剪贴板。旧桌面也尚未按移动端 `cap=clipboardText` 隐藏文件操作。正式移动端互通协议需要一起升级桌面，避免把该临时兼容层当作完整协议。

## 构建环境与验证

首次构建在 Apple Silicon macOS / Xcode 26.6 上执行。本机已安装 iOS 26.5 Simulator；Apple 下载工具没有提供请求的 iOS 18.5 runtime，因此保持最低版本 18.0，以已安装 runtime 验证启动，iOS 18 运行验证待补。Android SDK / NDK 和 CocoaPods 依赖由各官方工具安装。

已验证：

- iOS Release / arm64 Simulator 构建成功，自带 Hermes / JS 包，无需 Metro；在 iPhone 17 Pro / iOS 26.5 启动。
- Android ARM64 `dev` APK 构建成功，minSdk 35 / targetSdk 36，包 ID `com.anydrop.mobile.dev`；签名校验通过，确认包含 Hermes 字节码与原生运行库，无需 Metro。Kotlin 两项协议单测通过，覆盖同一组桌面字节向量与异常输入。本机仅有 API 33 模拟器，因此尚未运行 Android 15+ 真机 / 模拟器验证。
- iOS 设置保存、浅深主题、系统 UIPasteControl，以及 Rust 编码文本 → iOS 接收 / 写入剪贴板 → 系统粘贴 → iOS 群发 → Rust 解码通过；测试内容含中文和 emoji，隔离频段 4294967200。
- Swift 协议测试覆盖桌面生成的字节向量、截断、头部 / 校验损坏、非法 UTF-8、空输入和 65535 字节边界；TS 频段测试、类型检查、Lint 通过。
- CakeUI tokens 校验、完整 Web check / test:package 和 AnyDrop 桌面前端构建通过。移动 / 桌面 / CakeUI Web 均解析到同一份 React。
- 使用 react-native-safe-area-context 5.10.0；模板原有 5.5.2 在 RN 0.87 预编译 iOS Core 下缺少旧架构头文件，已升级并重新构建。

同机 mDNS 验证限制：Rust 探针能发现 Simulator，但 macOS 系统浏览器没有列出该 Rust 探针的原始公告。用系统 `dns-sd -R` 注册相同 TCP 端口后，Simulator 正常发现并双向收发。此项不作为真实桌面应用在所有 LAN 环境互发现的证明；真实跨设备 mDNS 与桌面按本机 IP 排除 peer 的行为仍需后续验证。

复现探针：

```sh
cargo run -p anydrop --example mobile_smoke_peer -- 4294967200
# 同机 Simulator 需要时，另开终端，把 PORT 替换成探针输出的 TCP 端口：
dns-sd -R 'Rust interoperability probe' _anydrop._udp local. PORT 'g=4294967200' 'dn=Rust interoperability probe' 'did=anydrop-mobile-smoke-probe' 'cap=clipboardText'
```

探针只传输固定测试文本，不访问宿主剪贴板；运行 15 分钟后退出。本次验证后已停止探针和 `dns-sd` 注册，App 服务恢复关闭。

## 本地交付产物

下列文件位于仓库内被 Git 忽略的 `test-results/mobile/`，不作为源码提交：

- `AnyDropMobile-0.1.0-dev-arm64.apk`：Android ARM64 开发签名安装包。
- `AnyDropMobile-0.1.0-ios-simulator.zip`：内含 arm64 Simulator Release `.app`，无需 Metro；不适用于 iPhone 真机安装。
- `ios-light.png`、`ios-dark.png`：实际 Simulator 截图。
- `text-interoperability.log`：Rust 测试探针的文本收发记录，仅含固定测试文本。
- `SHA256.json`：上述两个安装产物的 SHA-256 校验值。

复现构建和检查命令见 [移动端 README](../apps/mobile/README.md)。这些是首个开发增量的产物，不代表下列首版功能已经完成。

## Node 25 兼容验证（2026-09-20）

移动端 `engines.node` 增加 `^25.8.1`。锁定的 React Native / Metro 0.87.1 自身仍排除 Node 25，因此这个版本安装时使用 `yarn install --frozen-lockfile --ignore-engines`；没有改动上游依赖、全局 Yarn 配置或 CI 的 Node 22 版本。参数行为见 [Yarn 安装文档](https://classic.yarnpkg.com/lang/en/docs/cli/install/#toc-yarn-install-ignore-engines)。

已在本机 Node 25.8.1 / Yarn 1.22.22 下验证：

- 冻结锁文件安装成功；普通 workspace 命令通过项目自身的引擎检查。
- 移动端类型检查、Jest 频段测试通过；Lint 无错误（现有 Android 构建报告 JS 有两条警告）。
- Android / iOS 生产 JS bundle 与两端原生 Codegen 生成成功。
- `yarn mobile:start` 启动 Metro 成功；健康检查和 iOS 开发 bundle 请求返回成功，测试后停止服务。
- 桌面 TypeScript / Vite 前端构建成功。

这是当前锁定依赖的项目兼容验证，不代表上游承诺支持 Node 25。本次没有重跑完整 Xcode / Gradle 原生编译；上一节的安装包仍为首次交付时生成的产物。后续更新 RN / Metro 版本时需要重新核对引擎声明和工具链兼容性。

2026-09-23：将安装兼容参数写入仓库根目录 `.yarnrc`（`--install.ignore-engines true`），现在普通 `yarn` / `yarn install` 自动生效，无需每次手动传参。此配置仅覆盖本仓库的安装命令；其作用范围包含所有依赖的引擎声明检查，依赖版本与锁文件未改变。配置方式见 [Yarn 的命令参数配置文档](https://classic.yarnpkg.com/lang/en/docs/yarnrc/#toc-cli-arguments)。

使用 Node 25.8.1 / Yarn 1.22.22 复现原始错误后，应用配置并完成冻结锁文件安装、桌面 TypeScript / Vite 构建、移动端类型检查与 Jest 测试，以及 Android / iOS 生产 JS bundle 构建；均通过。本次未重跑 Xcode / Gradle 原生编译。

## 下一开发阶段

1. 完成 P0 的 HTTP 文件拉取与 iOS 后台下载验证，锁定局域网 ATS / 网络配置。
2. 新协议规范与桌面 capability / endpoint 协商；频段、消息身份、去重和回环验证。
3. 多文件发送、接受、取消、断点恢复、持久收件目录与完整导出，媒体导入相册。
4. Android connectedDevice 前台服务、通知动作和 AppWidget；iOS 本地通知与 WidgetKit 导航。
5. 两端首版验收和可安装 APK / Simulator 交付，补充拒绝权限和中断恢复用例。
