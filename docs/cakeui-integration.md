# CakeUI 接入与外观验证

日期：2026-09-19。

## 样式来源与实现

**AnyDrop 是样式基准。** 遇到颜色、密度或组件默认值冲突，直接修改 `/Users/miku/projects/cakeui`；AnyDrop 不用另一套覆盖颜色抵消 CakeUI 的默认主题。CakeDesign 提供设计原则，现有 AnyDrop 界面提供实际配色、尺寸和排版。Figma 与已退役项目不作为当前设计来源。

主窗口、接收浮窗和预览窗口接入 `CakeProvider`，使用粉色主题和 compact 密度。按钮、卡片、复选框、输入框、进度条、日志及悬浮提示使用真实 CakeUI 组件；新增更新详情使用 CakeUI Dialog。设备行、传输行、窗口控制与文件发送确认保留应用业务布局。

CakeUI 本次修改包括：

- 校准粉色主题浅深模式的背景、卡片、文字、强调色、边线、焦点与阴影；强调色为 `#d15776`。
- 将 AnyDrop 的字体、按钮高度、输入间距、圆角、日志行高和提示间距整理为 compact 主题变量。复选框保留各浏览器的原生边距，避免跨引擎位置偏移。
- 区分透明卡片与不透明浮层；进度轨道只绘制一次半透明背景，避免叠加后变深。提示按实际触发元素居中、重置标题字距，并保留原界面的小数像素位移定位。
- 同步公开 API / 设计文档、生成的 llms 文档和组件浏览器检查。

AnyDrop 的 SCSS 负责应用布局和业务状态；旧变量通过语义映射引用 CakeUI 变量，原来重复的通用控件样式已移除。CakeUI 的 blue / gold 主题保留各自配色。粉色主题改动会影响其他使用粉色主题的消费者，发布时应明确说明。

## 安装包与维护

当前使用官方 npm 注册表上的 **`@a1knla/cakeui@0.3.0`**，精确版本、下载地址和 integrity 写入 `yarn.lock`。临时 vendored 安装包已移除，克隆 AnyDrop 后直接 `yarn install --frozen-lockfile` 即可安装。

CakeUI 提交为 `4b6adeb5b5d9ddf222fdcc3b0b0cd11f32b01ec6`，发布标签为 `v0.3.0`。[发布工作流](https://github.com/hatsune-miku/cakeui/actions/runs/35437433832)已成功完成，注册表包摘要与经测试的安装包一致，见 [发布核验记录](verification/cakeui-npm-release.json)。AnyDrop 本次接入改动仍在本地工作区。

后续样式修正继续在 CakeUI 源码完成，经文档、构建、浏览器和安装包检查后发布新版本，再更新 AnyDrop 的精确版本与锁文件。不能只修改 `node_modules`。0.3.0 的 JS / CSS 产物已核对与先前通过迁移检查的本地修订包逐字节一致；原始迁移报告保留当时包名和摘要作为历史记录。

## 迁移验证

原始截图来自 AnyDrop 提交 `a6e0d5de3b1c70a6a70d165449516c29d589ff4d`，保存在 `tests/visual/before-cakeui/`。数据、日期、窗口、平台标识和视口固定；所有 Tauri 调用都经过 UI 测试替身，不读取真实剪贴板、不发送网络数据。

共 36 个场景：Chromium / WebKit × 浅色 / 深色 × 主界面、macOS 窗口布局、最小窗口、空状态、发送确认、按钮悬停、问号帮助浮层、接收浮窗、预览窗口。

迁移比较使用 Chromium 147.0.7727.15、WebKit 26.5（Playwright revision 2336），同一引擎内对比原始和迁移后的界面。为了隔离随后新增的功能，`--migration` 仅隐藏新增的桌面设置区域；被迁移的组件和样式仍为实际生产实现。原始截图禁止通过此模式覆盖。

最终迁移报告见 [verification/cakeui-migration.json](verification/cakeui-migration.json)：pixelmatch 阈值为 **0.02**，排除其识别出的抗锯齿差异；33 个场景为 0 个超阈值变化像素，3 个 Chromium 场景各有 7 个，单幅最多约 0.00086%，全部低于 0.01% 的通过上限。停止按钮图标边缘存在少量跨次渲染差异，按钮与 SVG 的坐标、尺寸、描边及计算颜色已核对一致。报告同时保留 `rawChangedPixels`，其值并非全部为零，因此结果代表设定条件下高度一致，不代表原始 RGBA 逐字节完全相同。系统字体、缩放比例和真实 Windows WebView2 仍需对应平台验收。

完整产品回归另存于 `tests/visual/baseline/`，**包含新增桌面设置**，使用 Playwright 1.62.1 默认 Chromium 151.0.7922.34 和 WebKit 26.5。从官方 npm 安装 0.3.0 后，36 个场景与完整界面基线的原始 RGBA 全部相同；详见 [完整回归报告](verification/desktop-regression.json)。这验证的是已校准界面切换安装来源后的稳定性，不代表原始迁移也逐字节相同。两组基线用途与 Chromium 版本不同，不能混用。设备像素比固定为 1。

2026-09-20 将软件更新、自启、键位绑定收入「更多设置」二级弹窗后，完整产品基线更新为新的设置布局；上面的安装来源比较报告保留为历史验证。此次 36 个场景中，设置区之外的超阈值差异为零，接收 / 预览窗口不变，详见 [设置调整的区域比较](verification/more-settings-visual.json)。`before-cakeui` 原始迁移基线没有改动，CakeUI npm 包与公开 API 也没有变化。

已有粉色按钮白字和浅色弱文字未全部达到 WCAG AA 的 4.5:1 对比度：约为 3.93:1 和 3.42:1。本次按维护者要求保留 AnyDrop 配色；CakeUI 浏览器测试明确记录有限的已知颜色对比度例外，其余无障碍违规仍会失败。此项不能描述为“完整 AA 通过”。

## 复现

使用 Node 22、Yarn 1；从 AnyDrop 根目录启动前端，另开终端执行检查：

```sh
yarn install --frozen-lockfile
yarn workspace @anydrop/desktop-tauri dev --host 127.0.0.1
```

```sh
# 首次安装测试浏览器；避免清除用于原始迁移比较的旧浏览器
PLAYWRIGHT_SKIP_BROWSER_GC=1 yarn playwright install chromium webkit
yarn test:visual
VISUAL_ENGINE=webkit yarn test:visual
yarn test:desktop
yarn test:release
yarn workspace @anydrop/desktop-tauri build
cargo test --workspace --locked
```

本机若关闭了 Yarn workspaces，可在命令前加 `YARN_WORKSPACES_EXPERIMENTAL=true`。WebKit 测试在 macOS 执行；CI 使用匹配环境和字体，不将其他操作系统产生的差异直接覆盖为新基线。

原始迁移比较：

```sh
VISUAL_BROWSER="/path/to/Chromium-147-executable" yarn test:visual --migration
VISUAL_ENGINE=webkit yarn test:visual --migration
```

测试结果与差异图写入 `test-results/visual/`。只有有意改变完整产品外观并人工检查后，才使用 `--record` 更新完整回归基线。组件库独立执行 `npm run check`、`npm run test:package` 和 `npm run test:browser`。
