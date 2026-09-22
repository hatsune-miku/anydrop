# AnyDrop 更新部署

更新端点：`https://anydrop-api.vanillacake.cn/rc/latest.json`。Windows x64 与 macOS universal 共用 RC 渠道；移动端不使用 Tauri Updater。

## 发布链路

1. 推送当前仓库 main，或手动运行 Release Candidate workflow 并选择 universal。
2. GitHub 编译版本 `0.1.2-rc.<run_number>.<run_attempt>`，使用 AnyDrop 私钥签名 Windows NSIS 与 macOS `.app.tar.gz`，将产物和 `latest.json` 放入 `rc-v<版本>` Release。
3. 服务器 `anydrop-updater.timer` 每五分钟检查一次。`mirror.py` 从 GitHub API 选择最高的签名 RC，下载两个平台产物并执行 minisign 验签。
4. 全部成功后发布不可变版本目录，原子切换清单，地址改为本站 HTTPS 下载。未完整上传、下载或验签失败时保留上一个版本；并发运行由文件锁互斥。
5. 客户端在「更多设置」检查、下载、再次验签，由用户安装并重启。没有更新版本时返回 204。存在未完成传输时禁止安装。

不复用 KVM 密钥或发布目录。KVM 仅作为 GitHub Release 镜像流程参考。当前不是 stable 渠道；未来增加 stable 时应使用独立清单和明确的版本迁移策略。

## 已部署内容

主机 `miku@vanillacake.cn`：

- `/opt/anydrop-updater/mirror.py`：镜像脚本，依赖 Python 3.11+、GitHub CLI、minisign。
- `/etc/anydrop-updater/minisign.pub`：从 Tauri 公钥外层 Base64 解码得到的 minisign 公钥，无私钥。
- `/var/www/html/anydrop-api/rc/`：版本目录与公开 `latest.json`；由 miku 写入，Nginx 只读。
- `/etc/nginx/sites-available/anydrop-api` 及 sites-enabled 软链：本目录 nginx.conf。
- `/etc/systemd/system/anydrop-updater.{service,timer}`：本目录对应文件。
- `/etc/letsencrypt/live/anydrop-api.vanillacake.cn/`：独立域名证书，Certbot webroot 自动续期。
- `/etc/letsencrypt/renewal-hooks/deploy/30-anydrop-nginx`：本目录 renew-nginx.sh，仅此证书续期后检查并重载 Nginx。

GitHub CLI 使用服务器上 miku 已有的 GitHub 登录，不复制 token。若服务器必须使用代理，可在 root 所有的 `/etc/anydrop-updater/mirror.env` 中设置 `HTTPS_PROXY`；当前采用服务器可用的直接连接。

```sh
sudo systemctl start anydrop-updater.service
systemctl status anydrop-updater.timer
journalctl -u anydrop-updater.service -n 30 --no-pager
curl -f https://anydrop-api.vanillacake.cn/healthz
curl -i https://anydrop-api.vanillacake.cn/rc/latest.json
```

暂停镜像：`sudo systemctl stop anydrop-updater.timer`；已发布文件仍可下载。恢复使用 `sudo systemctl start anydrop-updater.timer`。不删除已发版本目录，避免下载中的客户端失效。

## 签名与备份

GitHub repository Secrets：`ANYDROP_UPDATER_PRIVATE_KEY`、`ANYDROP_UPDATER_PRIVATE_KEY_PASSWORD`。

Repository Variables：`ANYDROP_UPDATER_PUBLIC_KEY`、`ANYDROP_UPDATER_ENDPOINT`。公开配置也写入桌面 tauri.conf.json。私钥由 workflow 环境变量传给 Tauri，不写入生成配置或日志。

维护者本机备份目录为 `~/.config/anydrop/signing/`，包含 `desktop-updater.key`、`desktop-updater.password` 和 `desktop-updater.key.pub`。私钥加密；目录权限 0700、私钥及密码文件 0600。请同时备份私钥与密码，GitHub Secrets 无法读回。不要重新生成覆盖当前密钥，否则已安装客户端将无法验证新签名。

公钥文件 SHA-256：`84adb97d142d6ad02938a5946dfb3c5888205fe07d7a7ccc0f335f17058c1cc9`。

签名验证遵循 [Tauri Updater](https://v2.tauri.app/plugin/updater/)。它与 Windows 代码签名、Apple 签名 / 公证不同，后两者仍按现有项目状态处理。

## 检查

```sh
node --test tests/updater.test.mjs
python3 -m unittest discover -s tests -p updater_mirror_test.py
cargo test --workspace --locked
```

镜像测试覆盖双平台完整性、universal 包复用、异常下载 / 验签不发布、版本防回退、路径来源校验和不可变版本保护。已用真实新密钥签署测试文件，并在服务器执行 minisign 验签通过。首次签名 Release 的构建与公网文件验证结果记录在桌面集成文档中。
