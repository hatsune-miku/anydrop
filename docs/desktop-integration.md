# 桌面自启、全局快捷键与在线更新

更新日期：2026-09-22。适用：当前 Tauri 桌面应用，发布目标为 Windows x64 和 macOS universal。

维护状态：在线更新已恢复接入，独立签名密钥、GitHub Secrets、HTTPS 端点与服务器定时镜像已配置。默认渠道为现有 RC 工作流；服务端部署与备份说明见 [deploy/updater](../deploy/updater/README.md)。

## 用户行为

设置区底部的「更多设置」与「保存设置」并列，打开 CakeUI 二级弹窗，集中放置软件更新、开机自启和键位绑定。这些项目即时生效，无需再按主页面的「保存设置」；关闭弹窗不会提交主页面尚未保存的名称或频段。Esc、右上角关闭和「完成」均可退出，关闭后焦点回到入口按钮。

「开机自启」状态直接读取系统注册结果；开启后使用 `--autostart` 参数登录启动，主窗口保持隐藏，托盘及原有局域网服务运行。普通手动启动仍显示主窗口。关闭窗口仍只是隐藏，退出应用才释放全局快捷键。

「发送当前剪贴板」默认不绑定。点击录制后按含 Ctrl、Alt 或 Command / Win 的组合键，Esc / Tab 可取消，清除即注销。录制时按 Esc 仅取消录制，关闭「更多设置」也会结束录制，恢复后台快捷键动作。新组合注册失败时保留原有绑定并显示错误；成功配置保存到应用配置目录中的 `desktop-settings.json`。系统中的自启状态不以此文件为准。

全局快捷键在 Rust 层注册，主窗口隐藏或失焦时仍可使用。录制时暂停该动作，避免误发送；按住重复事件和重叠发送被过滤。动作与主窗口「发送剪贴板」共用实现：发送给当前同频段已发现的局域网设备；优先文本，否则在开启图片同步时编码并发送图片。文件剪贴板格式尚不作为文件发送。手动动作不受自动同步 / 双击复制开关限制，服务停止时会报告失败。发起广播不等于逐设备确认送达。

更新流程为检查 → 展示版本 / 说明 → 下载并验签 → 用户点击安装并重启。已配置更新服务的发布构建启动后延迟 10 秒检查，开发模式不自动检查。更新详情从「更多设置」中打开，关闭详情后回到「更多设置」。下载过程中可依次关闭两层弹窗，重新打开后仍保留状态。存在待处理、传输中或暂停的任务时，前端和 Rust 都阻止安装；安装前停止服务，失败时尝试恢复。更新下载校验与安装由 Rust 持有，不依赖弹窗持续打开。

元数据请求超时 30 秒，安装包下载超时 15 分钟。只接受 HTTPS 元数据配置及初始下载地址，Tauri Updater 对下载内容执行签名验证。客户端内置 AnyDrop 公钥和 `https://anydrop-api.vanillacake.cn/rc/latest.json`；首次签名 RC 发布前服务返回 HTTP 204，表示暂无更新。

## 已配置的发布凭据

| 项目                 | 配置位置                                             | 内容                                                                                 |
| -------------------- | ---------------------------------------------------- | ------------------------------------------------------------------------------------ |
| AnyDrop 专属更新公钥 | GitHub Actions Variable `ANYDROP_UPDATER_PUBLIC_KEY` | Tauri signer 生成的 `.pub` 文件完整文本；可以公开                                    |
| 更新元数据地址       | Variable `ANYDROP_UPDATER_ENDPOINT`                  | 稳定 HTTPS 地址，返回该渠道的 Tauri 更新 JSON；请同时确定域名和 RC / stable 渠道策略 |
| AnyDrop 专属更新私钥 | Secret `ANYDROP_UPDATER_PRIVATE_KEY`                 | 私钥文件完整文本，由维护者直接录入 Secrets 并另行备份，无需发到对话                  |
| 私钥密码（如有）     | Secret `ANYDROP_UPDATER_PRIVATE_KEY_PASSWORD`        | 与该私钥配套的密码                                                                   |
| 元数据发布目标       | `/var/www/html/anydrop-api/rc/`                      | 独立 Nginx 站点与 `anydrop-updater.timer`；每五分钟检查 GitHub RC                   |

2026-09-22 已生成 AnyDrop 专用密钥并配置以上 Secrets / Variables。私钥加密保存在维护者本机 `~/.config/anydrop/signing/desktop-updater.key`，密码保存在同目录 `desktop-updater.password`，文件权限 0600、目录权限 0700；两者均需备份，不能提交到仓库。服务器仅持有公钥。KFC / KVM 仅用于参考更新调用链与镜像方式，未复制它们的签名密钥。更新签名与 Apple 代码签名 / 公证、Windows 代码签名是独立事项；如需正式分发的系统信任，仍沿对应平台的签名流程配置。

后续独立项目可在安全目录生成专属密钥，例如；不要重新生成或覆盖当前 AnyDrop 密钥：

```sh
yarn tauri signer generate -w /absolute/private-directory/anydrop-updater.key
```

私钥不能放进仓库或构建配置文件。Tauri 更新器使用嵌入公钥验签，不能绕过；保管和轮换需考虑已安装客户端。参见 [Tauri Updater 签名与静态 JSON 文档](https://v2.tauri.app/plugin/updater/)。

## 构建与发布

`scripts/prepare-updater.mjs` 仅输出公开配置到忽略的 `apps/desktop-tauri/src-tauri/.generated/updater.json`。公钥、端点和签名环境变量全部缺失时关闭更新产物；只填一部分则构建失败，避免产生错误配置的更新客户端。私钥只通过 `TAURI_SIGNING_PRIVATE_KEY` 传递给 Tauri 构建进程，密码使用 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。

本地已通过环境设置上述配置后：

```sh
yarn updater:prepare
yarn workspace @anydrop/desktop-tauri tauri build --config src-tauri/.generated/updater.json --bundles nsis
# macOS 改用 --target universal-apple-darwin --bundles app,dmg
```

现有 `.github/workflows/rc.yml` 使用 `tauri.conf.json` 的基础版本，生成 `基础版本-rc.运行号.重试号`，同时嵌入客户端并用于发布标签。版本比较依赖 SemVer：同一基础版本的正式版高于其 RC；已发布正式版之后，下一轮 RC 必须提高基础版本。

Windows 生成 NSIS `.exe` 和 `.exe.sig`；macOS 生成更新用 `.app.tar.gz` 及其 `.sig`，原有 DMG / ZIP 继续用于手动安装。`scripts/updater-manifest.mjs` 要求两平台产物及签名齐全，生成静态 `latest.json`；universal macOS 归档同时映射 `darwin-aarch64` 和 `darwin-x86_64`，Windows 映射 `windows-x86_64`。manifest 使用签名文件内容，下载地址指向该仓库的对应 GitHub Release。

工作流把 `latest.json` 上传为 RC 的 Release 附件。服务器通过 GitHub API 选择具有完整清单的最高 RC 版本，下载 Windows 与 universal macOS 产物，使用 AnyDrop 公钥执行 minisign 验签，确认全部成功后原子切换 `/rc/latest.json`。清单内的下载地址改写到同域名的不可变版本目录；同一个 macOS 包只下载一次，供两个架构使用。失败时保留当前清单，不回退版本、不覆盖已发布文件、不自动删除旧版本。

当前仅接入 RC 渠道，不使用 GitHub `/releases/latest`，避免误选旧 stable Release。旧客户端没有当前公钥与端点，需要手动安装一次新的签名 RC，后续才可在线更新。真实覆盖安装、重启和设置保留仍需在对应操作系统上验收，不能由镜像验签代替。

## 验证范围

已完成前端构建、macOS 上 Rust workspace 测试、发布配置 / manifest 单元测试，以及模拟 Tauri 的 UI 检查（自启切换、快捷键录制 / 冲突 / 清除、避免录制误发、下载与传输安装保护）。组件迁移另有双引擎截图回归。

2026-09-20：「更多设置」调整通过 Chromium / WebKit 的深浅色与较矮窗口检查，覆盖关闭后焦点返回、录制中 Esc 与关闭取消、设置保留、两层更新弹窗关闭 / 重开及下载状态保留。使用 Tauri 测试替身，未修改真实系统自启或快捷键。36 个原有截图场景中，pixelmatch 阈值 0.02 下设置区域之外没有变化；接收 / 预览窗口也没有变化。已人工查看新布局，并更新 28 张有意改变的主界面回归基线，原始 CakeUI 迁移基线保持不变；[区域比较记录](verification/more-settings-visual.json)。

更新基线后重新执行全部 36 个场景，超阈值变化像素均为 0，见[本次完整回归报告](verification/more-settings-regression.json)。

尚未执行：真实登录自启、两台机器的全局快捷键发送、Windows 原生构建 / 运行、真实签名更新的端到端安装，以及 GitHub Actions 的远程发布。这些检查依赖实际平台、签名配置或发布端点，不能用浏览器替身测试代替。已有 macOS 预览相关未使用代码告警仍存在。

实现入口：`src-tauri/src/desktop.rs` 负责系统集成，`src-tauri/src/updates.rs` 负责更新状态和安装，`src/components/DesktopSettings/` 负责界面。[Tauri Autostart](https://v2.tauri.app/plugin/autostart/) 和 [Global Shortcut](https://v2.tauri.app/plugin/global-shortcut/) 提供底层插件能力。
