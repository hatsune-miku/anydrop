# 剪贴板与桌面文件入口

剪贴板单次上限统一为 4 MiB（4,194,304 字节）：文本按 UTF-8 字节数，图片按编码后的 PNG 字节数。协议额外为包头保留空间；Android、iOS 同步使用该上限，旧版本接收端仍受旧限制约束。

收到文本后在 Rust 工作线程识别完整 JSON、XML、TOML，并按 Unicode scalar 数显示 `收到 JSON (12,345 字符)`。不识别 YAML；空白、注释和只有表头的 TOML 回退到普通文本。XML 不加载外部实体。解析器保留深度、实体展开与节点数量保护，过于复杂的文档会按普通文本处理。

桌面自动发送或手动发送超限内容时，先保存此次内容的临时快照，在右侧显示「确认以文件形式发送」。一个在线设备时一次确认，多设备时先选目标，无设备时保持等待。文件只发给一个目标。新的超限复制替换尚未确认的旧剪贴板请求；取消会删除临时文件。已确认的文件在传输完成、拒绝或取消后清理，暂停/失败时保留到进程结束以便重试。

更多设置中「自动接收文件」默认关闭。打开后，新文件请求直接保存到默认接收目录，失败时保留手动接收入口。该设置与快捷键、右键菜单独立保存于 `desktop-settings.json`。

macOS 接收浮窗允许首次点击，但不成为 key / main window，也不激活 AnyDrop；显示浮窗仅调整层级，点「已读」后隐藏时不把主窗口带到前台。实现保留 Tao 的窗口对象与 WebKit 的 KVO 类，按窗口标记限制原生行为，普通主窗口仍使用原来的显示、聚焦逻辑。`receive_window_macos.rs` 使用 AppKit 的非激活窗口内部接口，仅用于 macOS 桌面；升级 Tauri / Tao 或 macOS 时需重新运行原生探针。

## 文件右键菜单

「注册文件右键菜单」默认开启，开启时提供「重新注册」。两项命令为「使用 AnyDrop 发送」与「复制绝对路径」。文件发送使用同一个选设备确认浮窗，不会广播文件。

macOS 在当前用户 `~/Library/Services` 安装 `AnyDrop-Send.workflow`、`AnyDrop-CopyPaths.workflow`，显示在 Finder 的「服务」中。启动时刷新应用路径，关闭开关删除这两个工作流并刷新服务缓存。支持文件、文件夹和多选；选择的文件名作为独立参数传递。

Windows 传统菜单在当前用户注册两个 `IExplorerCommand`，扩展按版本保留独立文件，避免升级覆盖被 Explorer 占用的 DLL；原生扩展位于安装目录 `shell/AnyDropShell-版本号.dll`。Windows 11 中通过「显示更多选项」使用传统菜单，不修改系统的菜单策略。卸载钩子负责注销当前用户的传统菜单。

Windows 11 新式右键菜单暂时停用：不构建、签名或分发 MSIX 身份包，启动、重新注册与注销均不调用 Appx 注册链路，也不会再显示缺少签名身份包的错误。`shell/windows/AppxManifest.xml` 仅保留为未启用的历史源文件。发布不需要 `ANYDROP_WINDOWS_CERTIFICATE` 或其密码；Tauri Updater 的现有签名流程保持独立运行。

本地 Windows 构建请先运行 `pwsh ./scripts/build-windows-shell.ps1`；RC workflow 已包含此步骤。`Desktop verification` workflow 可单独验证 Windows 编译、COM 类和协议测试，不创建 Release。

## 验证

- `cargo test -p anydrop --test test_clipboard_limits`
- `cargo test -p anydrop-desktop-tauri --lib`
- 启动前端后运行 `node scripts/popup-check.mjs` 与 `node scripts/desktop-check.mjs`，可加 `VISUAL_ENGINE=webkit`。
- Swift 协议检查与 Android `:app:testDevUnitTest` 覆盖 4 MiB 往返、超限及损坏帧。
- macOS 单测实际运行生成的 Automator 服务，检查空格、引号、中文和 shell 特殊字符在文件参数中被完整保留。
- `cargo run -p anydrop-desktop-tauri --example receive_activation_probe` 在真实 macOS 桌面检查非激活标记、保留 Tao / KVO 类、连续 30 次显示和隐藏不激活应用或显示主窗口，以及应用已在前台时保留主窗口焦点。不启动局域网服务、不改系统设置；该探针不模拟物理鼠标点击。
- Windows RC 发布前运行原生 COM 测试与 `tests/windows-shell-registration.ps1`，验证无 MSIX 时启动注册、重新注册、重复注册、中文菜单与注销均成功。
- 原生集成探针 `cargo run -p anydrop --example desktop_file_probe -- PORT FILE [pending]` 仅连接 loopback。在 debug 构建中可用 `ANYDROP_TEST_CONFIG_DIR` 指定独立设置目录；该模式不修改系统右键菜单。原生透明窗口仍需在各目标系统实测，浏览器截图不能替代原生合成器验证。
