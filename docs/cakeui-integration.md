# CakeUI 接入与外观验证

更新：2026-11-12。

## 样式来源与实现

**AnyDrop 是样式基准。** 遇到颜色、密度或组件默认值冲突，直接修改 `/Users/miku/projects/cakeui`；AnyDrop 不用另一套覆盖颜色抵消 CakeUI 的默认主题。CakeDesign 提供设计原则，现有 AnyDrop 界面提供实际配色、尺寸和排版。Figma 与已退役项目不作为当前设计来源。

主窗口、接收浮窗和预览窗口接入 `CakeProvider`，默认使用粉色主题和 compact 密度。更多设置中的外观区使用细边框分组，接入 CakeUI 官方 `ThemeSettings`：提供蓝、粉、金、豆沙、绿豆沙、葡萄六种预设和自定义颜色，支持颜色选择器、HEX / RGB 输入、浅色 / 深色 / 跟随系统以及自动 / 浅色 / 深色文字。编辑时在当前窗口预览，应用后保存至原生 `settings.json` 并同步到三个窗口；取消、Esc 或关闭撤销预览。不同方案的明暗与前景独立记忆，切换预设保留自定义颜色。界面密度仍可单独选择紧凑 / 舒适，跟随系统时实时响应系统明暗变化。按钮、卡片、复选框、输入框、进度条、日志及悬浮提示使用真实 CakeUI 组件；新增更新详情使用 CakeUI Dialog。设备行、传输行、窗口控制与文件发送确认保留应用业务布局。

CakeUI 本次修改包括：

- 校准粉色主题浅深模式的背景、卡片、文字、强调色、边线、焦点与阴影；强调色为 `#d15776`。
- 将 AnyDrop 的字体、按钮高度、输入间距、圆角、日志行高和提示间距整理为 compact 主题变量。复选框保留各浏览器的原生边距，避免跨引擎位置偏移。
- 区分透明卡片与不透明浮层；进度轨道只绘制一次半透明背景，避免叠加后变深。提示按实际触发元素居中、重置标题字距，并保留原界面的小数像素位移定位。
- 同步公开 API / 设计文档、生成的 llms 文档和组件浏览器检查。

AnyDrop 的 SCSS 负责应用布局和业务状态；旧变量通过语义映射引用 CakeUI 变量，原来重复的通用控件样式已移除。CakeUI 的 blue / gold 主题保留各自配色。粉色主题改动会影响其他使用粉色主题的消费者，发布时应明确说明。

## 安装包与维护

当前使用官方 npm 注册表上的 **`@a1knla/cakeui@0.6.1`**，精确版本、下载地址和 integrity 写入 `yarn.lock`。临时 vendored 安装包已移除，克隆 AnyDrop 后直接 `yarn install --frozen-lockfile` 即可安装。

历史版本 0.3.0 的 CakeUI 提交为 `4b6adeb5b5d9ddf222fdcc3b0b0cd11f32b01ec6`，发布标签为 `v0.3.0`。[发布工作流](https://github.com/hatsune-miku/cakeui/actions/runs/35437433832)已成功完成，注册表包摘要与经测试的安装包一致，见 [发布核验记录](verification/cakeui-npm-release.json)。这份核验记录保留为 0.3.0 的历史记录。

后续样式修正继续在 CakeUI 源码完成，经文档、构建、浏览器和安装包检查后发布新版本，再更新 AnyDrop 的精确版本与锁文件。不能只修改 `node_modules`。0.3.0 的 JS / CSS 产物已核对与先前通过迁移检查的本地修订包逐字节一致；原始迁移报告保留当时包名和摘要作为历史记录。

## 0.4.0 主题配置

按照 [CakeUI 官方文档](https://gallery.vanillacake.cn/llms.txt)的 `CakeProvider`、`ThemeSettings` 和 `@a1knla/cakeui/theme` 接入；主题计算、颜色校验、预设与编辑器均由组件库提供。TypeScript 使用 Bundler 模块解析以读取官方 `theme` 子入口。

原生 `appearanceConfig` 保存完整 version=2 的 `ThemeConfig`，包括 `customColor` 和各方案的 `options`。`appearanceTheme` 与 `darkMode` 保持为当前方案和模式的兼容入口，原生保存时同步到主题文档；现有主界面深色开关和 HTTP / MCP 调用继续有效。旧设置没有 `appearanceConfig` 时，按已有配色与明暗选择生成配置。前端使用 CakeUI 的 `validateThemeConfig` 校验后读取，损坏的扩展配置回退到兼容字段，不影响窗口加载。

设置读取沿用宿主快照；`ThemeSettings.storage` 的保存方法写入原生设置，不使用 localStorage。未应用的草稿只在当前 Provider 内预览。存储失败显示具体原因，并通过中文“重试”按钮再次保存当前配置。CakeUI 统一聚焦按钮，使 WebKit 关闭嵌套弹窗后也能恢复入口焦点；写入期间沿用组件的 aria-disabled 状态，保留焦点并防止重复打开。

`yarn test:desktop` 与 `VISUAL_ENGINE=webkit yarn test:desktop` 覆盖六种预设、颜色校验、RGB 规范化、预览取消、异步保存期间的焦点和禁用、各方案记忆、取消重置、系统明暗变化和三个窗口重新加载 / 快照同步。Rust 设置测试覆盖旧配置读取、完整主题文档往返、兼容字段更新及显式清除配置。

2026-11-10 升级至 0.4.0 后，主窗口采用组件库新的紧凑卡片圆角。旧基线比较结果见 [Chromium](verification/cakeui-0.4-chromium-comparison.json) 与 [WebKit](verification/cakeui-0.4-webkit-comparison.json)，这两份报告记录的是更新前差异，不能视为旧基线通过。人工检查后更新 `tests/visual/baseline/`；接收窗口基线同时对齐现有测试场景的 540 × 690 视口，旧文件仍为 400 × 500，无法直接逐像素比较。原始迁移基线 `before-cakeui` 保留。更新后 Chromium / WebKit 共 36 个场景通过，见 [0.4.0 回归报告](verification/cakeui-0.4-regression.json)。

## 0.4.2 无边框分区

主界面 Card 在所有配色和两种密度下均不绘制默认边框或阴影，分区只依靠卡片与页面的底色差异。修正在 CakeUI 完成，AnyDrop 不额外覆盖组件库样式；更多设置中的外观区仍是业务显式设置的边界。输入框、按钮与键盘焦点维持各自边线。

`yarn test:theme` 和 `VISUAL_ENGINE=webkit yarn test:theme` 检查六种预设加自定义颜色的浅深模式，逐个验证主面板没有 border / box-shadow，并保存真实截图。该检查补充原有以粉色为主的视觉回归，覆盖此次特殊配色问题。

此次移除旧卡片轮廓造成的基线差异已人工检查，见 [Chromium](verification/cakeui-0.4.2-chromium-comparison.json) 与 [WebKit](verification/cakeui-0.4.2-webkit-comparison.json)。接收浮窗和预览窗口与旧基线一致；主窗口基线更新为无边框效果，原始迁移基线保持不变。更新后两个引擎的 36 个场景均无超阈值变化像素；Chromium 最小窗口场景合计有 4 个原始像素差异，其余场景逐像素一致，见 [0.4.2 回归报告](verification/cakeui-0.4.2-regression.json)。

## 0.5.0 组件与主题更新

升级到官方 npm 的 `@a1knla/cakeui@0.5.0`，锁文件固定下载地址和完整性摘要。上游变更见 [0.5.0 源码提交](https://github.com/hatsune-miku/cakeui/commit/0e3822054d6757137e072f213873ad3e5da39346)。现有 `CakeProvider`、`ThemeSettings.storage` 与 version=2 主题配置兼容，无需迁移原生设置；新增 `labels.retry` 使用中文“重试”。

沿用组件库新版外观：所有配色下自绘选择控件，两种密度统一字体栈，紧凑日志与提示字号提高到 12px，弹窗增加退出过渡，粉色辅助文字与状态色改善对比度。主面板仍然无边框、无阴影，强调色色值保持不变。AnyDrop 未增加样式覆盖来抵消这些上游调整。

升级前后差异保存在 [Chromium](verification/cakeui-0.5.0-chromium-comparison.json) 与 [WebKit](verification/cakeui-0.5.0-webkit-comparison.json)。人工检查主界面、最小窗口与主题设置后，将完整产品视觉基线更新为 0.5.0；原始迁移基线不变。生产构建、两个引擎的全配色无边框检查、设置交互与 36 个视觉场景均通过，保存失败后的中文重试也已验证，见 [0.5.0 回归报告](verification/cakeui-0.5.0-regression.json)。

## 迁移验证

原始截图来自 AnyDrop 提交 `a6e0d5de3b1c70a6a70d165449516c29d589ff4d`，保存在 `tests/visual/before-cakeui/`。数据、日期、窗口、平台标识和视口固定；所有 Tauri 调用都经过 UI 测试替身，不读取真实剪贴板、不发送网络数据。

共 36 个场景：Chromium / WebKit × 浅色 / 深色 × 主界面、macOS 窗口布局、最小窗口、空状态、发送确认、按钮悬停、问号帮助浮层、接收浮窗、预览窗口。

迁移比较使用 Chromium 147.0.7727.15、WebKit 26.5（Playwright revision 2336），同一引擎内对比原始和迁移后的界面。为了隔离随后新增的功能，`--migration` 仅隐藏新增的桌面设置区域；被迁移的组件和样式仍为实际生产实现。原始截图禁止通过此模式覆盖。

最终迁移报告见 [verification/cakeui-migration.json](verification/cakeui-migration.json)：pixelmatch 阈值为 **0.02**，排除其识别出的抗锯齿差异；33 个场景为 0 个超阈值变化像素，3 个 Chromium 场景各有 7 个，单幅最多约 0.00086%，全部低于 0.01% 的通过上限。停止按钮图标边缘存在少量跨次渲染差异，按钮与 SVG 的坐标、尺寸、描边及计算颜色已核对一致。报告同时保留 `rawChangedPixels`，其值并非全部为零，因此结果代表设定条件下高度一致，不代表原始 RGBA 逐字节完全相同。系统字体、缩放比例和真实 Windows WebView2 仍需对应平台验收。

完整产品回归另存于 `tests/visual/baseline/`，**包含新增桌面设置**，使用 Playwright 1.62.1 默认 Chromium 151.0.7922.34 和 WebKit 26.5。从官方 npm 安装 0.3.0 后，36 个场景与完整界面基线的原始 RGBA 全部相同；详见 [完整回归报告](verification/desktop-regression.json)。这验证的是已校准界面切换安装来源后的稳定性，不代表原始迁移也逐字节相同。两组基线用途与 Chromium 版本不同，不能混用。设备像素比固定为 1。

2026-09-20 将软件更新、自启、键位绑定收入「更多设置」二级弹窗后，完整产品基线更新为新的设置布局；上面的安装来源比较报告保留为历史验证。此次 36 个场景中，设置区之外的超阈值差异为零，接收 / 预览窗口不变，详见 [设置调整的区域比较](verification/more-settings-visual.json)。`before-cakeui` 原始迁移基线没有改动，CakeUI npm 包与公开 API 也没有变化。

0.3.0 迁移时，粉色按钮白字和浅色弱文字未全部达到 WCAG AA 的 4.5:1 对比度：约为 3.93:1 和 3.42:1。当时按维护者要求保留 AnyDrop 配色；CakeUI 浏览器测试明确记录有限的已知颜色对比度例外，其余无障碍违规仍会失败。0.5.0 已修正辅助文字、危险色与选中文字，固定粉色强调底的白字仍约 3.94:1，不能描述为“完整 AA 通过”。

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


## 0.5.1 滚动与主题设置修正

圆角容器沿用原生滚动，使用 CakeUI 的轨道内缩变量，不改容器尺寸、内容高度或滚动范围。设备、传输、日志列表的纵向起点保留 4px，终点内缩一个大圆角；主设置区从大圆角内开始，底部保留 4px。没有引入虚拟列表。

浏览器默认焦点外框在 CakeUI 主题作用域统一移除，键盘控件保留内描边。主题弹窗固定外部尺寸，切换选项和自定义色时不重新居中；小窗口仅滚动正文，标题与底部操作保持可见。文字明暗只修改强调区文字和图标，不再调整背景或透明度。

`yarn test:styles` 与 `VISUAL_ENGINE=webkit yarn test:styles` 验证单次原始鼠标点击、主题弹窗尺寸、容器无外框和滚动范围；WebKit 额外从实际截图中定位滑块，检查四类容器在顶部 / 底部均避开圆角。Chromium 的 macOS 无头模式不绘制滚动条，因此不把其截图作为滑块像素验证。组件库测试覆盖全部预设和自定义色在 foreground 切换前后的背景一致性。

正式 npm 包与本地通过验证的 tgz 完整性摘要一致。生产构建、双引擎交互与样式检查、36 个视觉场景全部通过，见 [0.5.1 回归记录](verification/cakeui-0.5.1-regression.json)。视觉基线仅更新 WebKit 的浅色 / 深色小窗口滚动条位置，[更新前差异](verification/cakeui-0.5.1-webkit-comparison.json)保留供核查。


## 0.6.1 文字偏好与原生交互

文字颜色提供自动 / 浅色 / 深色与全局明暗阈值。局部调整可按普通按钮、强调按钮、选中项、正文、输入控件与状态文字分别覆盖，规则按当前配色的浅深模式保存。单个 CakeUI 组件支持长按右键 600ms，或聚焦后按 Alt+Shift+C；菜单可恢复跟随用途，也可独立选择自动 / 浅色 / 深色。单组件优先于用途规则，用途规则优先于全局偏好，背景保持不变。

组件身份由 CakeUI 结合类型、可访问名称、具名区域与结构自动识别。AnyDrop 不给普通组件逐个配置标识；完全同构的动态列表仍无法仅凭 DOM 保证业务身份，必要时可使用库的可选 data-cake-theme-key。主窗口沿用 ThemeSettings.storage；接收浮窗挂载隐藏设置入口来注册同一原生存储，菜单修改会保存并同步到其他窗口，不执行业务按钮动作。预览窗口读取已保存的全局和用途规则。

系统 WebKit 首次交互会重算 fieldset / legend 的高度，造成内部分组移动；CakeUI 改用带 radiogroup 语义的布局，保留原生 radio 键盘行为。焦点指示区分指针与键盘，鼠标点击及关闭弹窗后的恢复焦点不画内描边；键盘导航继续显示。普通按钮无常驻边框，背景略微加深。开关、状态圆点与滚动条使用独立的基础颜色，不随文字偏好变色。

AnyDrop 仅设置中文 locale 并复用库提供的交互；已删除原来的主题入口聚焦补丁。单组件菜单复用原生保存错误和重试逻辑；在主题设置中修改单组件时进入草稿，应用提交、取消撤销。新版自动文字按背景重新计算，所以视觉基线中的文字色与普通按钮底色会变化。

生产构建、两个引擎的设置 / 快捷键 / 嵌套弹窗检查、空白处首次点击、组件菜单原生保存、全配色无边框检查与 36 个视觉场景均通过。基线经过人工检查后更新；两端对新基线的超阈值变化像素均为零，少量原始像素差异来自抗锯齿。详见 [0.6.1 回归记录](verification/cakeui-0.6.1-regression.json)及更新前的 [Chromium](verification/cakeui-0.6.1-chromium-comparison.json) / [WebKit](verification/cakeui-0.6.1-webkit-comparison.json) 对比。
