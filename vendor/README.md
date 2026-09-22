# 本地开发包

`a1knla-cakeui-native-0.1.0.tgz` 来自 CakeUI 仓库的 `packages/native`，当前尚未发布 npm。包含原生组件 TypeScript 源码，由 Metro 编译。包内没有路径链接，干净 checkout 安装不依赖另一个仓库。

修改组件应在 CakeUI 源码进行，运行 tokens / 文档 / 包验证后用 `npm pack` 重建。替换同名 tarball 时同时更新 Yarn 对该包的内容摘要，避免复用旧缓存。发布 npm 后再切换到注册表中的固定版本。

当前 tarball SHA-1：`1fd5ba6e1a0bcd79d321ab432dc3935ddabe1f13`。
