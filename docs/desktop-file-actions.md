# 剪贴板与桌面文件入口

剪贴板单次上限统一为 4 MiB（4,194,304 字节）：文本按 UTF-8 字节数，图片按编码后的 PNG 字节数。协议额外为包头保留空间；Android、iOS 同步使用该上限，旧版本接收端仍受旧限制约束。

Windows / macOS 按系统剪贴板变更编号识别每次复制，复制相同文本或图片也会重新发送。只屏蔽 AnyDrop 写入远端内容时对应的那次系统变更；发送操作本身不设置屏蔽标记。双击复制模式要求 600 ms 内复制相同内容，远端写入不算一次点击。每台设备只发送一次，多个网卡地址按当前子网排序，失败后再尝试其他地址。

防回弹不要求对端升级，不依赖消息 ID、来源字段或跳数；文本 / 图片继续使用旧协议。旧版多地址重复投递、延迟回传的每次本机写入都记录变更编号，不会被当作用户复制再广播。旧版本自身仍可能忽略相同内容的重新复制，需升级该发送端才能获得新的重复复制行为。

自动与手动发送使用同一读取路径：支持 PNG、JPEG、TIFF、BMP、GIF、WebP；Windows 额外处理传统 CF_DIB、调色板、位掩码和 DIBV5，macOS 直接读取图片类型。支持剪贴板中的单个本地图片文件，以及 HTML 中单个内嵌 data 图片或本地 file 图片；不下载 HTTP 图片或网络共享路径。只有 HTML 的文本也可识别。动画图片同步第一帧，统一编码为 PNG。源文件上限 64 MiB，解码尺寸与内存也有上限；转换后的 PNG 超过 4 MiB 时仍需确认文件发送。

监听每 100 ms 检查变更编号，不常驻读取图片，不依赖 Windows 隐藏消息窗口的通知订阅；每次读取重新取得剪贴板句柄。忙碌或延迟提供的数据会在同一变更下重试，接收写入也会重试。macOS 后台访问使用 autorelease pool。TCP / UDP / mDNS 工作线程异常退出后自动重启。读取失败日志仅记录错误和格式名称，不记录剪贴板内容。

接收提示由 Rust 暂存至前端渲染确认，最多保留 4 条未确认消息、最长 2 分钟，恢复后再显示。Windows 重新显示时重新定位并以不激活方式恢复原生窗口；视图 2.5 秒未响应时尝试重新加载，重试间隔至少 30 秒；重新加载失败或仍未响应时重建窗口。文件接收与超限图片确认也使用同一个视图恢复机制。

收到文本后在 Rust 工作线程识别完整 JSON、XML、TOML，并按 Unicode scalar 数显示 `收到 JSON (12,345 字符)`。不识别 YAML；空白、注释和只有表头的 TOML 回退到普通文本。XML 不加载外部实体。解析器保留深度、实体展开与节点数量保护，过于复杂的文档会按普通文本处理。

桌面自动发送或手动发送超限内容时，先保存此次内容的临时快照，在右侧显示「确认以文件形式发送」。一个在线设备时一次确认，多设备时先选目标，无设备时保持等待。文件只发给一个目标。新的超限复制替换尚未确认的旧剪贴板请求；取消会删除临时文件。已确认的文件在传输完成、拒绝或取消后清理，暂停/失败时保留到进程结束以便重试。

更多设置中「自动接收文件」默认关闭。打开后，新文件请求直接保存到默认接收目录，失败时保留手动接收入口。该设置与快捷键、右键菜单独立保存于 `desktop-settings.json`。

macOS 接收浮窗允许首次点击，但不成为 key / main window，也不激活 AnyDrop；显示浮窗仅调整层级，点「已读」后隐藏时不把主窗口带到前台。实现保留 Tao 的窗口对象与 WebKit 的 KVO 类，按窗口标记限制原生行为，普通主窗口仍使用原来的显示、聚焦逻辑。`receive_window_macos.rs` 使用 AppKit 的非激活窗口内部接口，仅用于 macOS 桌面；升级 Tauri / Tao 或 macOS 时需重新运行原生探针。

## 文件右键菜单

「注册文件右键菜单」默认开启，开启时提供「重新注册」。两项命令为「使用 AnyDrop 发送」与「复制绝对路径」。文件发送使用同一个选设备确认浮窗，不会广播文件。

macOS 在当前用户 `~/Library/Services` 安装 `AnyDrop-Send.workflow`、`AnyDrop-CopyPaths.workflow`，显示在 Finder 的「服务」中。启动时刷新应用路径，关闭开关删除这两个工作流并刷新服务缓存。支持文件、文件夹和多选；选择的文件名作为独立参数传递。

Windows 传统菜单在当前用户注册两个 `IExplorerCommand`，扩展按版本保留独立文件，避免升级覆盖被 Explorer 占用的 DLL；原生扩展位于安装目录 `shell/AnyDropShell-版本号.dll`。Windows 11 中通过「显示更多选项」使用传统菜单，不修改系统的菜单策略。卸载钩子负责注销当前用户的传统菜单。

注册入口通过 `GetSystemDirectoryW` 定位系统的 `WindowsPowerShell/v1.0/powershell.exe`，不依赖启动进程继承的 `PATH`，不要求另装 PowerShell 7；卸载也使用系统 PowerShell 的绝对路径，并在 32 位 NSIS 进程中优先通过 Sysnative 启动原生版本。系统 PowerShell 或注册脚本确实缺失时，错误会明确显示缺失项与路径。原来的 `program not found` 表示注册脚本尚未启动，并非右键菜单签名错误。

Windows 11 新式右键菜单暂时停用：不构建、签名或分发 MSIX 身份包，启动、重新注册与注销均不调用 Appx 注册链路，也不会再显示缺少签名身份包的错误。`shell/windows/AppxManifest.xml` 仅保留为未启用的历史源文件。发布不需要 `ANYDROP_WINDOWS_CERTIFICATE` 或其密码；Tauri Updater 的现有签名流程保持独立运行。

本地 Windows 构建请先运行 `pwsh ./scripts/build-windows-shell.ps1`；RC workflow 已包含此步骤。`Desktop verification` workflow 可单独验证 Windows 编译、COM 类和协议测试，不创建 Release。

## 验证

- `cargo test -p anydrop --lib --test test_clipboard_limits --test test_clipboard_recovery`：包含 12 次 TCP 接收服务重启、半截连接、重复文本、图片与双网卡去重。
- `cargo test -p anydrop-desktop-tauri --lib`
- RC 的 Windows / macOS runner 额外运行 `-- --include-ignored --test-threads=1`，在隔离桌面实际读写系统剪贴板，覆盖 30 轮重复复制与远端写入、原生图片格式、HTML 和本地图片文件。旧协议测试通过真实 TCP 发送固定的旧格式文本 / 图片帧，覆盖多次重复投递、合并通知、超过原有屏蔽窗口的延迟回传，以及接收后再次手动复制；这是旧协议与本机防回弹测试，不是旧版 GUI 双机实测。该测试会改写系统剪贴板，不应在日常桌面直接运行。
- `scripts/popup-check.mjs` 覆盖延迟加载、相同内容的新提示、同 ID 去重、隐藏后恢复、确认与自动消失。上述加速测试不等同于 11–17 天真实休眠运行，也未代替微信 / 飞书各版本的实机测试；不支持的私有格式可依据格式名称日志继续定位。
- 启动前端后运行 `node scripts/popup-check.mjs` 与 `node scripts/desktop-check.mjs`，可加 `VISUAL_ENGINE=webkit`。
- Swift 协议检查与 Android `:app:testDevUnitTest` 覆盖 4 MiB 往返、超限及损坏帧。
- macOS 单测实际运行生成的 Automator 服务，检查空格、引号、中文和 shell 特殊字符在文件参数中被完整保留。
- `cargo run -p anydrop-desktop-tauri --example receive_activation_probe` 在真实 macOS 桌面检查非激活标记、保留 Tao / KVO 类、连续 30 次显示和隐藏不激活应用或显示主窗口，以及应用已在前台时保留主窗口焦点。不启动局域网服务、不改系统设置；该探针不模拟物理鼠标点击。
- Windows RC 发布前运行原生 COM 测试与 `tests/windows-shell-registration.ps1`，验证无 MSIX 时启动注册、重新注册、重复注册、中文菜单与注销均成功。
- Windows Rust 回归测试在清空父、子进程 `PATH` 的独立进程中复现旧启动方式的 `program not found`，验证新入口成功；另覆盖工作目录 / `PATH` 中存在同名假程序。注册脚本测试也清空 `PATH`，验证普通注册、修复与注销完整通过。
- 原生集成探针 `cargo run -p anydrop --example desktop_file_probe -- PORT FILE [pending]` 仅连接 loopback。在 debug 构建中可用 `ANYDROP_TEST_CONFIG_DIR` 指定独立设置目录；该模式不修改系统右键菜单。原生透明窗口仍需在各目标系统实测，浏览器截图不能替代原生合成器验证。
