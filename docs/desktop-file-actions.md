# 剪贴板与桌面文件入口

剪贴板单次上限统一为 4 MiB（4,194,304 字节）：文本按 UTF-8 字节数，图片按编码后的 PNG 字节数。协议额外为包头保留空间；Android、iOS 同步使用该上限，旧版本接收端仍受旧限制约束。

收到文本后在 Rust 工作线程识别完整 JSON、XML、TOML，并按 Unicode scalar 数显示 `收到 JSON (12,345 字符)`。不识别 YAML；空白、注释和只有表头的 TOML 回退到普通文本。XML 不加载外部实体。解析器保留深度、实体展开与节点数量保护，过于复杂的文档会按普通文本处理。

桌面自动发送或手动发送超限内容时，先保存此次内容的临时快照，在右侧显示「确认以文件形式发送」。一个在线设备时一次确认，多设备时先选目标，无设备时保持等待。文件只发给一个目标。新的超限复制替换尚未确认的旧剪贴板请求；取消会删除临时文件。已确认的文件在传输完成、拒绝或取消后清理，暂停/失败时保留到进程结束以便重试。

更多设置中「自动接收文件」默认关闭。打开后，新文件请求直接保存到默认接收目录，失败时保留手动接收入口。该设置与快捷键、右键菜单独立保存于 `desktop-settings.json`。

## 文件右键菜单

「注册文件右键菜单」默认开启，开启时提供「重新注册」。两项命令为「使用 AnyDrop 发送」与「复制绝对路径」。文件发送使用同一个选设备确认浮窗，不会广播文件。

macOS 在当前用户 `~/Library/Services` 安装 `AnyDrop-Send.workflow`、`AnyDrop-CopyPaths.workflow`，显示在 Finder 的「服务」中。启动时刷新应用路径，关闭开关删除这两个工作流并刷新服务缓存。支持文件、文件夹和多选；选择的文件名作为独立参数传递。

Windows 传统菜单在当前用户注册两个 `IExplorerCommand`，扩展按版本保留独立文件，避免升级覆盖被 Explorer 占用的 DLL；原生扩展位于安装目录 `shell/AnyDropShell-版本号.dll`。Windows 11 新菜单使用相同扩展与稀疏身份包；Windows 可将同一应用的两个动作收在 AnyDrop 子菜单中。不会修改系统的菜单策略。卸载钩子负责注销当前用户的菜单与身份包。

### Windows 11 发布前所需材料

需要受目标 Windows 系统信任的代码签名证书（含私钥的 PFX 和密码），或另行接入签名服务。**Tauri Updater 的 Ed25519 密钥不适用于 Windows 代码签名。** 不会在用户设备上自行添加信任根或启用开发者模式。

现成 PFX 流程对应 GitHub Secrets：

- `ANYDROP_WINDOWS_CERTIFICATE`：PFX 的 Base64 内容。
- `ANYDROP_WINDOWS_CERTIFICATE_PASSWORD`：PFX 密码。

`scripts/build-windows-shell.ps1` 使用证书 Subject 填充包 Publisher，编译扩展、生成并签名 MSIX。没有证书时仍编译传统菜单扩展，未签名的身份包只留作测试产物，不随正式安装包安装；设置中会提示新菜单缺少已签名身份包。证书或签名服务尚未提供时，不能宣称 Windows 11 新菜单已可分发。

本地 Windows 构建请先运行 `pwsh ./scripts/build-windows-shell.ps1`；RC workflow 已包含此步骤。`Desktop verification` workflow 可单独验证 Windows 编译、COM 类和协议测试，不创建 Release。

## 验证

- `cargo test -p anydrop --test test_clipboard_limits`
- `cargo test -p anydrop-desktop-tauri --lib`
- 启动前端后运行 `node scripts/popup-check.mjs` 与 `node scripts/desktop-check.mjs`，可加 `VISUAL_ENGINE=webkit`。
- Swift 协议检查与 Android `:app:testDevUnitTest` 覆盖 4 MiB 往返、超限及损坏帧。
- macOS 单测实际运行生成的 Automator 服务，检查空格、引号、中文和 shell 特殊字符在文件参数中被完整保留。
- 原生集成探针 `cargo run -p anydrop --example desktop_file_probe -- PORT FILE [pending]` 仅连接 loopback。在 debug 构建中可用 `ANYDROP_TEST_CONFIG_DIR` 指定独立设置目录；该模式不修改系统右键菜单。原生透明窗口仍需在各目标系统实测，浏览器截图不能替代原生合成器验证。
