# AnyDrop Mobile

React Native 0.87.1 / React 19.2.3 / TypeScript，Android 15+、iOS 18+，仅手机。工程直接维护在 AnyDrop workspace 中。

当前开发增量：CakeUI Native 页面、频段 / 名称 / 外观持久化、原生 Bonjour / NSD 发现、兼容桌面 TCP 格式的纯文本群发与接收。iOS 使用系统 UIPasteControl，Android 只在应用获得焦点时读取剪贴板。服务默认关闭；此阶段的运行状态不代表系统保证后台保活。

后续范围保持不变：新协议与能力协商、多文件接收确认和拉取、任务恢复、完整导出、媒体入相册、通知和 Widget。当前没有文件入口，也没有这些能力的完成声明。详见 [技术方案](../../docs/mobile-plan.md) 和 [开发记录](../../docs/mobile-development.md)。

## 开发

需要 Node 22.13+（22.x）、24.3+（24.x）、25.8.1+（25.x）或 26+，以及 Yarn 1.22、JDK 17、Android SDK / NDK；iOS 需要 Xcode、Ruby 3.2+ 和 Gemfile 中锁定的 CocoaPods。原生工程与锁文件均入库。不要在子目录单独安装一份 JS 依赖。

```sh
# 仓库根目录
YARN_WORKSPACES_EXPERIMENTAL=true yarn install --frozen-lockfile
yarn mobile:start
# 另开终端，连接 Android 15+ 设备或模拟器
yarn mobile:android
```

Node 25 的安装命令：

```sh
# 仓库根目录；此参数也适用于后续重新安装依赖
yarn install --frozen-lockfile --ignore-engines
yarn mobile:start
```

项目允许 Node 25.8.1+，但当前锁定的 React Native / Metro 0.87.1 依赖仍在 `engines.node` 中排除了整个 25.x，因此只修改项目声明不足以让 Yarn 1 安装通过。`--ignore-engines` 仅用于这次安装，不修改全局配置；启动、检查和构建命令保持不变。这是项目的兼容路径，不代表上游对 Node 25 的支持承诺；CI 继续使用 Node 22。普通安装继续保留引擎检查。

Android SDK 位置通过 ANDROID_HOME 或未入库的 android/local.properties 指定；构建要求见 android/build.gradle。iOS 首次安装依赖：

```sh
cd apps/mobile
bundle install
cd ios
bundle exec pod install
cd ../../..
yarn mobile:ios --simulator 'iPhone 17 Pro'
```

Simulator 名称按本机 `xcrun simctl list devices available` 的结果选择。最低部署版本是 18.0；使用更高 runtime 启动不等于完成 iOS 18 验证。

## 独立安装包

```sh
# 仓库根目录，Android ARM64 开发 APK，自带 JS / Hermes，无需 Metro
yarn mobile:apk
```

输出 apps/mobile/android/app/build/outputs/apk/dev/app-dev.apk。开发包 ID 为 com.anydrop.mobile.dev，使用公开开发密钥，只用于本地安装验证。release 构建不会自动使用开发密钥；正式分发签名尚未配置。

iOS 在安装 Pods 后，从仓库根目录构建独立 Simulator 应用：

```sh
# 将 SIMULATOR_UDID 设置为 xcrun simctl list devices available 中的设备 ID
xcodebuild -workspace apps/mobile/ios/AnyDropMobile.xcworkspace \
  -scheme AnyDropMobile -configuration Release -sdk iphonesimulator \
  -destination "id=$SIMULATOR_UDID" \
  -derivedDataPath test-results/mobile/ios-build \
  CODE_SIGNING_ALLOWED=NO ONLY_ACTIVE_ARCH=YES ARCHS=arm64 build

# 先在 Simulator 中启动对应设备，再安装和启动
xcrun simctl install "$SIMULATOR_UDID" \
  test-results/mobile/ios-build/Build/Products/Release-iphonesimulator/AnyDropMobile.app
xcrun simctl launch "$SIMULATOR_UDID" com.anydrop.mobile
```

该命令面向 Apple Silicon Simulator，Release 包自带 JS，无需 Metro；不用于 iPhone 真机分发。

## 检查

```sh
yarn mobile:typecheck
yarn workspace @anydrop/mobile lint
yarn workspace @anydrop/mobile test --runInBand
cd apps/mobile/android
./gradlew :app:testDevUnitTest -PreactNativeArchitectures=arm64-v8a
```

桌面 Rust 生成的文本测试向量在 test-fixtures/text-wire.json；Swift 与 Kotlin 用同一组内容校验 Unicode、帧长度、校验值和损坏数据。Swift 检查在 macOS 仓库根执行：

```sh
mkdir -p test-results/mobile
xcrun swiftc apps/mobile/ios/AnyDropMobile/Native/TextWire.swift scripts/check-mobile-wire.swift -o test-results/mobile/check-text-wire
test-results/mobile/check-text-wire apps/mobile/test-fixtures/text-wire.json
```

## CakeUI

使用 CakeUI 仓库 packages/native 中的 @a1knla/cakeui-native；当前通过 vendor 内的固定 tarball 安装，源码与 Metro 入口已包含在包中，不要求本机存在 CakeUI checkout。Web 与 RN 颜色由同一个主题源生成。原生包暂未发布 npm，后续改动先同步 CakeUI 源码，再重新打包并更新锁文件。
