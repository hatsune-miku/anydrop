# AnyDrop Windows Frontend

[简体中文](README.zh.md) | [English](README.md)

AnyDrop 是一个跨平台的文本和文件分享系统。

- [观看演示视频](https://hatsune-miku.github.io/#anydrop-video)

## 如何下载？

请前往 [v0.1.0.0 版本的 release page](https://github.com/hatsune-miku/AnyDrop-win/releases/tag/v0.1.0.0).

## 问题反馈

如果使用 Windows 版 AnyDrop 的过程中遇到问题，请用中文或英文

- [提起一个 issue](https://github.com/hatsune-miku/AnyDrop-win/issues/new/choose)，或是
- [查看现有 issues](https://github.com/hatsune-miku/AnyDrop-win/issues)

## AnyDrop Family

- [libanydrop](https://github.com/hatsune-miku/libanydrop)
- [Windows Client (WinUI 3)](https://github.com/hatsune-miku/AnyDrop-win)
- [Android Client (React Native)](https://github.com/hatsune-miku/anydrop4a)
- [macOS Client (SwiftUI)](https://github.com/Lsjy44/anydrop_mac)
- [Netdisk Frontend (Vue.js)](https://github.com/hatsune-miku/anydrop-cloud)
- [Backend (SpringBoot)](https://github.com/hatsune-miku/anydrop-backend)

# 附录

## 怎么看 CPU 架构？

- 对于 **Windows** 用户：
    - 绝大多数 Windows PC 都是 `x64` 架构，尤其是游戏本。
    - 如果不确定，可参考[这篇文章](https://support.microsoft.com/en-us/windows/32-bit-and-64-bit-windows-frequently-asked-questions-c6ca9541-8dce-4d48-0415-94a3faa2e13d)。

- 对于 **macOS** 用户：
    - 如果你的 MacBook 有刘海，那么 CPU 架构是 `arm64`。
    - 否则，请点击左上角的苹果 () 图标，选择“关于本机”。如果处理器那里显示 Intel，那么架构是 `x64`。如果显示 M1 或 M2，那么架构是 `arm64`。

## appx_install.exe 是什么？

这是一个用于安装 AnyDrop 的开源的小程序。

参见：[appx_install](https://github.com/hatsune-miku/appx-install)
