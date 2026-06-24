# AnyDrop 功能批次设计（2026-06-24）

本设计覆盖一次性提出的 11 项需求。目标是给出每一项的明确实现方案、受影响文件、边界情况，以及贯穿性的架构决策（多窗口、原生配置、通知、单例）。实现将按文末的依赖顺序进行。

## 决策摘要（已与用户确认）

- 交付方式：一次性完整设计，按依赖顺序实现。
- Peer 备注 key：按主机名 `hostname`。
- 保存位置：全局默认 + 每次接收可改。
- 文件速览：独立速览窗口（类似 macOS Quick Look）。

---

## 一、贯穿性架构

### 1. 多窗口模型

当前是单窗口（label=`main`）单 React 应用。新增两个原生窗口：

- `receive`：接收弹窗。承担「接收确认」+「接收中实时进度」+（可选）「剪贴板接收提示」。位于主屏右下角。
- `preview`：文件速览窗口，按需创建/复用。

实现方式：所有窗口加载同一个 `index.html`，在 [main.tsx](apps/desktop-tauri/src/main.tsx) 里按 `getCurrentWindow().label` 分流挂载不同根组件：

```
main    → <App/>            （现有主界面）
receive → <ReceivePopup/>   （新增）
preview → <PreviewWindow/>  （新增）
```

`receive` / `preview` 窗口在 `tauri.conf.json` 中**不**预先创建，而是由 Rust 在需要时用 `WebviewWindowBuilder` 动态创建（`visible:false` 起步，定位后再 `show`），关闭时 `hide` 复用，避免反复创建销毁。

### 2. 通信架构（单一事实源）

Rust 端 `Backend` 仍是唯一事实源。规则：

- **状态广播**：Rust 用 `app.emit(...)`（全局广播，所有窗口都收到）发送 `snapshot` / `incoming-file` / `transfer-updated` / `text-received` / `image-received`。`receive` 与 `main` 窗口订阅同一批事件，各自渲染所需视图——主界面保留传输列表进度，弹窗展示接收项进度。二者天然一致，无需额外同步通道。
- **命令**：弹窗里的「接收/拒绝/更改位置」直接 `invoke` 现有命令（`accept_transfer`/`reject_transfer` 等），与主界面同源。
- 这样「主界面与弹窗同时展示进度」不需要新协议，只是多一个事件订阅者。

### 3. 原生配置（替换浏览器 storage）

沿用现有原生文件方案（`config_dir/AnyDrop/*.json`，与现有 `settings.json` 一致，无需引入新依赖）：

- `settings.json`：扩展 `AppSettings`，新增标量偏好：`dark_mode: bool`、`default_save_dir: String`、`clipboard_popup_enabled: bool`。
- `peer_remarks.json`：`{ hostname: remark }` 映射，单独文件，仅本地。
- 迁移 `darkMode`：从 `localStorage` 改为 `AppSettings.dark_mode`。首次启动若 `settings.json` 无该字段，默认跟随系统。

> 备选：引入 `tauri-plugin-store`。不推荐，因为会与现有 `settings.json` 手写方案产生两套持久化，增加不一致风险。保持单一手写 JSON 方案更一致。

### 4. 通知与提醒（#6 的一部分）

- **图标跳动/任务栏高亮**：用 Tauri v2 `Window::request_user_attention(Some(UserAttentionType::Critical))`——Windows 下闪烁任务栏、macOS 下弹跳 Dock。收到文件 offer 时对 `main` 窗口调用。
- **系统通知横幅**：新增 `tauri-plugin-notification`，收到 offer 时弹一条「AnyDrop 收到文件请求：<名称>」。

### 5. 单例限制（#10）

新增 `tauri-plugin-single-instance`。第二次启动时不再新建进程，而是聚焦并显示已有主窗口。这同时根治「多个重复托盘图标 / 多个 AnyDrop 同时运行」——托盘图标在每个进程内构建，单例后只剩一个进程一个托盘。

### 6. 新增 capabilities

为 `receive` / `preview` 窗口新增 capability（或将现有 `default` 的 `windows` 扩展为通配）。需要的权限：`event:default`、`dialog:default`、`opener:default`、`notification:default`、窗口控制、以及速览所需的 `core:asset` / `assetProtocol` 作用域（限定到保存目录）。

---

## 二、逐项设计

### #2 原生配置（基建，先做）

- `AppSettings` 增 `dark_mode`、`default_save_dir`、`clipboard_popup_enabled` 三字段，`#[serde(default)]` 保证旧文件兼容。
- `normalize_settings` 为 `default_save_dir` 填默认值 `download_dir()/AnyDrop`。
- 前端 `SettingsModel` 同步加字段；`darkMode` 从 `localStorage` 改读 `snapshot.settings.dark_mode`，切换时走 `setBoolSetting`。
- 文件：[lib.rs](apps/desktop-tauri/src-tauri/src/lib.rs)、[App.tsx](apps/desktop-tauri/src/App.tsx)。

### #1 Peer 本地备注名

- 新增 `peer_remarks.json` 读写函数 + `Mutex<HashMap<String,String>>` 存于 `Backend`。
- 新增命令 `set_peer_remark(hostname, remark)`（空串=清除），持久化并 `emit_snapshot`。
- `PeerGroup` 增 `remark: Option<String>`，`group_peers` 时查表填充。
- 前端设备行：展示 `remark ?? label`；提供编辑入口（设备行 hover 出现小铅笔图标，点开行内输入框，回车保存）。
- **隐私**：备注仅存本地、绝不进入发现包；「对方看不到被备注成什么」自动满足。
- 文件：lib.rs、App.tsx、新增 styles。

### #3 发送确认框（发送端）

- 现状：`sendFiles`/`sendFolder` 选完路径立即 `send_paths`。
- 改为：选完后置入 `pendingSend` 状态，弹出应用内确认 Modal，展示「目标设备 + 文件名列表 + 数量」，提供「确认发送/取消」。确认才 `invoke('send_paths')`。
- 体量较小，纯前端；不阻塞主流程。文件：App.tsx、styles。

### #4 暂停协议修复（核心）

**根因**（已定位）：暂停只是本地 `cancel` 掉任务并 `conn.close(0,"cancelled")`，对端把连接中断当作错误：
- 发送端暂停 → 接收端 `receive_one_file` 读流报错 → 走 Abort 路径 → 接收端显示「错误」且 `active.remove()` 删除续传状态。
- 接收端暂停 → 接收端 cancel-token 触发 → 返回 Err → Abort → 给发送端发 `Status::Abort` → 发送端显示「错误」；接收端同样删除续传状态。

两个方向都报错，且都销毁了续传所需状态。

**修复方案：用 QUIC 应用层关闭码区分 暂停/取消/完成，暂停走「保留状态」路径。**

1. 定义关闭码：`CLOSE_DONE=0`、`CLOSE_CANCEL=1`、`CLOSE_PAUSE=2`。
2. 在 `ServerHandle` 为每个 transfer 维护独立的 **pause token**（与 cancel token 分开）。`pause_transfer` 触发 pause token；`cancel_transfer` 仍触发 cancel token。
3. **发送端暂停**：发送循环检测到 pause → `conn.close(CLOSE_PAUSE, b"paused")`，emit `Paused`，**保留 `send_args`**，跳出重试循环（不自动重连）。
4. **接收端暂停**：`receive_one_file` 的 `select!` 区分 pause/cancel：pause 时返回一个「Paused 哨兵」而非 Err；`handle_connection` 见 Paused → **保留 `active` 状态**、emit `Paused`、`conn.close(CLOSE_PAUSE)` 通知发送端。
5. **对端识别**：接收/发送在 `accept_uni`/`read`/`write`/读最终状态时，若错误是 `ConnectionError::ApplicationClosed{ error_code == CLOSE_PAUSE }`，则判定为「对端暂停」：emit `Paused`、保留各自续传状态、**不**进入 error/abort/retry 路径。`CLOSE_CANCEL` 仍按取消处理（删状态、emit Cancelled）。
6. **恢复**：复用现有 `resume_transfer` → 用同一 `transfer_id` 重连 → 接收端 `items_match` 命中 → 回 `resume_offsets` → 续传。该机制已存在且工作，暂停修复后状态不再被销毁，恢复即可用。
7. 前端乐观状态：`pause_transfer` 命令把行置为 9（Paused）后，后端 emit 的也是 `Paused`（status 9），不再被 Cancelled(5)/Error(6) 覆盖。

- 文件：[protocol.rs](core/src/transfer/protocol.rs)（新增关闭码常量/必要的控制语义）、[mod.rs](core/src/transfer/mod.rs)（pause token、pause_transfer/resume 语义）、[client.rs](core/src/transfer/client.rs)、[server.rs](core/src/transfer/server.rs)、lib.rs（状态映射）。
- 测试：新增集成测试，两个本地实例，覆盖「传输中暂停→双方进入 Paused 且状态保留→恢复→完成」，发送端暂停与接收端暂停各一例。

### #6 独立原生接收弹窗 + 通知 + 实时进度

- 新增 `receive` 窗口（动态创建，定位主屏右下角：取 `primary_monitor` 尺寸 - 窗口尺寸 - 边距）。
- 收到 offer：Rust 创建/显示 `receive` 窗口 → `request_user_attention(Critical)` + 通知横幅。
- `<ReceivePopup/>` 订阅 `incoming-file`/`transfer-updated`/`snapshot`：
  - 待确认项：展示名称/来源/大小 + 「接收/拒绝/更改位置」。
  - 已接收项：**确认后弹窗不关闭**，原地切换为进度条 + 速率，状态终态后转为「打开所在文件夹/移除」。
- 主界面同步：移除原本阻塞式的应用内接收 Modal（`dialog-backdrop`），接收交互改由原生弹窗承担；主界面「传输」列表仍展示同一进度（已具备）。
- 弹窗空（无待确认、无进行中）时自动 `hide`。
- 文件：lib.rs（窗口管理、通知、定位）、main.tsx、新增 `ReceivePopup.tsx`、capabilities、tauri.conf（窗口默认不建，仅声明权限）、Cargo（notification 插件）。

### #7 修改保存位置

- 全局默认：`AppSettings.default_save_dir`，设置面板加「默认保存目录」字段 + 「浏览」按钮（`dialog.open({directory:true})`）。`accept_transfer` 默认用它（替换硬编码 `~/Downloads/AnyDrop`）。
- 每次可改：`receive` 弹窗待确认项提供「更改位置」按钮（目录选择），选定后作为本次覆盖。
- `accept_transfer` 增可选参数 `save_dir: Option<String>`；为空用默认。
- 文件：lib.rs、ReceivePopup.tsx、App.tsx（设置）、styles。

### #8 打开所属文件夹层级修复 + 选中文件

- 现状：`open_transfer_folder` 打开 `local_path.parent()`，而收到文件的 `local_path` 是保存根目录 → 打开成了上一层，且不选中。
- 修复：
  1. 在 `Transfer` 上记录**真实落地路径** `reveal_path`（接收：`save_root/<顶层条目>`；发送：源路径）。
  2. 新命令/改造：用 `tauri-plugin-opener` 的 `reveal_item_in_dir(reveal_path)`，跨平台在文件管理器中**选中**该文件/文件夹（Win `explorer /select,`、mac `open -R`、Linux 尽力而为）。
- 文件：lib.rs（落地路径计算、reveal）、前端按钮保持。

### #9 刷新按钮旋转动画

- 刷新进行中给 `RefreshCw` 图标加 `@keyframes spin{to{transform:rotate(360deg)}}`，`animation: spin 200ms linear infinite`；至少转满一圈（最短 ~200ms）后停。灵动快速。
- 纯前端：App.tsx（`refreshing` 状态）、styles.scss。

### #5 文件速览（独立窗口）

- 已接收完成的项，行内加「速览」按钮（眼睛图标）。
- 命令 `preview_file(transfer_key)`：解析 `reveal_path` 与按扩展名判定的类型，创建/复用 `preview` 窗口并把 `path`/`kind` 通过事件或初始化参数传入。
- `<PreviewWindow/>` 渲染：
  - 图片 → `<img>`；音频 → `<audio controls>`；视频 → `<video controls>`：经 Tauri **asset 协议**（`convertFileSrc`，作用域限保存目录）流式读取。
  - 其它类型 → 类型描述卡片（图标 + 扩展名/MIME + 大小 + 路径），无预览。
  - **压缩包不做任何预览处理**，按「其它类型」走描述卡。
- 交互：ESC 关闭、点击遮罩关闭；窗口标题为文件名。
- 文件：lib.rs（窗口、asset 作用域、类型判定）、main.tsx、新增 `PreviewWindow.tsx`、capabilities、tauri.conf（asset 安全配置）。

### #10 单例限制 + 托盘去重

- 引入 `tauri-plugin-single-instance`，回调中 `show_main_window`。根治多开与重复托盘。
- 文件：Cargo.toml、lib.rs（`run()` 注册插件）。

### #11 剪贴板接收弹窗开关

- `AppSettings.clipboard_popup_enabled`，默认 `false`，设置面板加开关。
- 开启时，收到剪贴板文本/图片（已有 `text-received`/`image-received` 事件）→ 复用 `receive` 窗口展示一条「收到剪贴板」卡片（文本截断预览 / 图片缩略），数秒后自动消失或手动关。
- 默认关：保持现有静默行为。
- 文件：lib.rs（settings、收到剪贴板时按开关触发弹窗）、ReceivePopup.tsx（剪贴板卡片类型）、App.tsx（设置开关）。

---

## 三、实现顺序（依赖驱动）

1. **#2 原生配置基建** —— 后续多项依赖新设置字段。
2. **#10 单例 + #9 刷新动画 + #8 打开文件夹修复** —— 独立小项，快速见效、互不耦合。
3. **#1 Peer 备注** —— 依赖 #2 的配置层。
4. **#3 发送确认框** —— 独立前端。
5. **#6 接收弹窗 + 通知**（含多窗口基建）—— 最大项，是 #7/#11/#5 的窗口与通信底座。
6. **#7 保存位置** —— 挂在接收弹窗 + 设置上。
7. **#11 剪贴板弹窗** —— 复用接收弹窗。
8. **#5 文件速览** —— 独立 `preview` 窗口。
9. **#4 暂停协议修复** —— 核心后端改动 + 集成测试，独立推进，放最后单独验证。

---

## 四、风险与边界

- **多窗口 + 单例交互**：单例聚焦逻辑只针对 `main`；`receive`/`preview` 由 Rust 管理生命周期。
- **暂停修复的协议兼容**：关闭码方案不改变线格式（仍是现有 JSON 控制消息 + QUIC 关闭码），与现网默认部署兼容；老版本对端遇到 `CLOSE_PAUSE` 会当作普通连接错误并重试，退化为「自动恢复」，不崩溃。
- **asset 协议安全**：作用域严格限定到保存目录，避免任意文件读取。
- **速览大文件**：媒体走流式 asset 协议，不整块读入内存。
- **备注同名冲突**：两台主机同名时备注会共享——已知限制，可接受（按用户选择的 hostname key）。
- **Linux reveal/通知**：尽力而为，非主目标平台（Win/mac 优先）。

---

## 五、验证

- 后端：`cargo build -p anydrop` + 新增传输集成测试（暂停/恢复双向）。
- 前端：`yarn workspace @anydrop/desktop-tauri build`（tsc + vite）。
- 手动：双机或双实例联调发现、发送确认、接收弹窗进度、暂停/恢复、速览、保存位置、备注、剪贴板弹窗、单例。
