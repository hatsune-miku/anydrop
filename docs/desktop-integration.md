# 桌面自启、全局快捷键与在线更新

更新日期：2026-09-23。适用：当前 Tauri 桌面应用，发布目标为 Windows x64 和 macOS universal。

维护状态：在线更新已上线，首个签名版本为 `0.1.2-rc.47.1`，已完成远程构建、镜像和真实客户端下载验签。默认渠道为现有 RC 工作流；服务端部署与备份说明见 [deploy/updater](../deploy/updater/README.md)。

## 用户行为

设置区底部的「更多设置」与「保存设置」并列，打开 CakeUI 二级弹窗。开机自启、自动接收文件和文件右键菜单使用右侧开关，附带操作说明；键位绑定独立排列，软件更新在底部显示实际应用版本与检查状态。弹窗标题和底部操作保持可见，矮窗口下仅正文滚动。这些项目即时生效，无需再按主页面的「保存设置」；关闭弹窗不会提交主页面尚未保存的名称或频段。Esc、右上角关闭和「完成」均可退出，关闭后焦点回到入口按钮。

「更多设置」弹窗及遮罩同步淡入、淡出，使用 200ms 的 `--cake-normal` 时长和 `cubic-bezier(0.29, 0, 0, 1)` 曲线，不附带位移或缩放。关闭请求立即结束快捷键录制并停用弹窗内交互，淡出完成后关闭原生弹窗、恢复入口焦点；动画结束事件缺失时有计时兜底。淡入中关闭从当前透明度继续淡出，重开会取消待执行的关闭。减少动态效果偏好或零时长设置下立即关闭。

「开机自启」状态直接读取系统注册结果；开启后使用 `--autostart` 参数登录启动，主窗口保持隐藏，托盘及原有局域网服务运行。普通手动启动仍显示主窗口。关闭窗口仍只是隐藏，退出应用才释放全局快捷键。

「发送当前剪贴板」默认不绑定。点击录制后按含 Ctrl、Alt 或 Command / Win 的组合键，Esc / Tab 可取消，清除即注销。录制时按 Esc 仅取消录制，关闭「更多设置」也会结束录制，恢复后台快捷键动作。新组合注册失败时保留原有绑定并显示错误；成功配置保存到应用配置目录中的 `desktop-settings.json`。系统中的自启状态不以此文件为准。

快捷键使用可读名称展示，例如 `Ctrl + Alt + V`；macOS 显示 Option / Command，原生保存与注册格式不变。鼠标按「取消」结束录制并保留原有绑定，不会因输入框失焦而重新开始录制。

全局快捷键在 Rust 层注册，主窗口隐藏或失焦时仍可使用。录制时暂停该动作，避免误发送；按住重复事件和重叠发送被过滤。动作与主窗口「发送剪贴板」共用实现：发送给当前同频段已发现的局域网设备；优先文本，否则在开启图片同步时编码并发送图片。文件剪贴板格式尚不作为文件发送。手动动作不受自动同步 / 双击复制开关限制，服务停止时会报告失败。发起广播不等于逐设备确认送达。

更新流程为检查 → 展示版本 / 说明 → 下载并验签 → 用户点击安装并重启。已配置更新服务的发布构建启动后延迟 10 秒检查，开发模式不自动检查。更新详情从「更多设置」中打开，关闭详情后回到「更多设置」。下载过程中可依次关闭两层弹窗，重新打开后仍保留状态。存在待处理、传输中或暂停的任务时，前端和 Rust 都阻止安装；安装前停止服务，失败时尝试恢复。更新下载校验与安装由 Rust 持有，不依赖弹窗持续打开。

元数据请求超时 30 秒，安装包下载超时 15 分钟。只接受 HTTPS 元数据配置及初始下载地址，Tauri Updater 对下载内容执行签名验证。客户端内置 AnyDrop 公钥和 `https://anydrop-api.vanillacake.cn/rc/latest.json`；首次签名 RC 发布前服务返回 HTTP 204，表示暂无更新。

## 已配置的发布凭据

| 项目                 | 配置位置                                             | 内容                                                                                 |
| -------------------- | ---------------------------------------------------- | ------------------------------------------------------------------------------------ |
| AnyDrop 专属更新公钥 | GitHub Actions Variable `ANYDROP_UPDATER_PUBLIC_KEY` | Tauri signer 生成的 `.pub` 文件完整文本；可以公开                                    |
| 更新元数据地址       | Variable `ANYDROP_UPDATER_ENDPOINT`                  | `https://anydrop-api.vanillacake.cn/rc/latest.json`，当前采用 RC 渠道 |
| AnyDrop 专属更新私钥 | Secret `ANYDROP_UPDATER_PRIVATE_KEY`                 | 已生成并录入的加密私钥；另有本机备份，不写入仓库或日志                  |
| 私钥密码             | Secret `ANYDROP_UPDATER_PRIVATE_KEY_PASSWORD`        | 已生成并录入的配套随机密码；与私钥一同备份                                                                   |
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

2026-09-23：「更多设置」视觉调整通过前端构建、Chromium / WebKit 深浅色与 1020 × 580 矮窗口检查。覆盖正文滚动时底部操作可见、开关与重新注册、快捷键显示和原生保存格式、鼠标取消录制、嵌套更新弹窗，以及原有淡入淡出、减少动态效果和焦点返回。截图保存在本地 `test-results/desktop/settings-design-*.png`；使用 Tauri 测试替身，未修改真实系统自启或快捷键。

已完成前端构建、macOS 上 Rust workspace 测试、发布配置 / manifest 单元测试，以及模拟 Tauri 的 UI 检查（自启切换、快捷键录制 / 冲突 / 清除、避免录制误发、下载与传输安装保护）。组件迁移另有双引擎截图回归。

2026-09-20：「更多设置」调整通过 Chromium / WebKit 的深浅色与较矮窗口检查，覆盖关闭后焦点返回、录制中 Esc 与关闭取消、设置保留、两层更新弹窗关闭 / 重开及下载状态保留。使用 Tauri 测试替身，未修改真实系统自启或快捷键。36 个原有截图场景中，pixelmatch 阈值 0.02 下设置区域之外没有变化；接收 / 预览窗口也没有变化。已人工查看新布局，并更新 28 张有意改变的主界面回归基线，原始 CakeUI 迁移基线保持不变；[区域比较记录](verification/more-settings-visual.json)。

更新基线后重新执行全部 36 个场景，超阈值变化像素均为 0，见[本次完整回归报告](verification/more-settings-regression.json)。

2026-09-22：「更多设置」淡入 / 淡出在 Chromium、WebKit 检查通过，覆盖弹窗与遮罩透明度同步、200ms 时长及指定曲线、淡入中关闭、关闭中重开、动画被取消后的兜底关闭、Esc 和焦点恢复、减少动态效果时立即关闭。原有桌面设置与嵌套更新弹窗交互检查也通过。[动效验证记录](verification/more-settings-motion.json)。

2026-09-22：GitHub Actions 已完成 Windows x64 与 macOS universal 原生发布构建、签名和 Release 上传。[首个签名 RC 为 0.1.2-rc.47.1](https://github.com/hatsune-miku/anydrop/releases/tag/rc-v0.1.2-rc.47.1)，对应提交 `23425f39dce4157253cf9b911ccebaff8e9cbaa8`；[工作流的四个任务全部通过](https://github.com/hatsune-miku/anydrop/actions/runs/35699426937)。修正了 CakeUI Native 本地归档与 yarn.lock 的 SHA-1 不一致问题，并在全新目录、全新缓存下使用 Node 22 完成 frozen-lockfile 安装；没有改变组件样式。

服务器已用 minisign 验证两个安装包后发布 `/rc/latest.json`；清单返回 HTTP 200 且禁止缓存，版本目录启用不可变缓存，重复同步保持当前版本。补齐了 systemd 服务的现有代理配置（本机 7897 端口），首次完整镜像用时约 6 秒。

在 macOS 的隔离 Tauri 测试运行时中，使用项目相同的 `tauri-plugin-updater 2.11.0`、正式端点与公钥，对三个目标分别执行真实 HTTPS 检查、下载和签名验证，全部通过。Windows 包为 4,418,794 字节，macOS 通用包为 13,717,375 字节，下载内容的 SHA-256 均与 GitHub Release 附件一致；macOS 包经 lipo 确认为 arm64 / x86_64 双架构，发布版本、端点与公钥均已嵌入实际二进制。该探针没有执行安装或重启，也没有启动 AnyDrop 的局域网服务。[完整验证记录](verification/updater-release.json)。

尚未执行：真实登录自启、两台机器的全局快捷键发送、Windows 原生运行，以及真实签名更新的覆盖安装、重启和设置保留。这些检查依赖实际运行平台，不能用浏览器替身或下载验签代替。已有 macOS 预览相关未使用代码告警仍存在。

实现入口：`src-tauri/src/desktop.rs` 负责系统集成，`src-tauri/src/updates.rs` 负责更新状态和安装，`src/components/DesktopSettings/` 负责界面。[Tauri Autostart](https://v2.tauri.app/plugin/autostart/) 和 [Global Shortcut](https://v2.tauri.app/plugin/global-shortcut/) 提供底层插件能力。
