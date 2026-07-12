# BreezeRESP

[English](README_EN.md) | 中文
> 一款轻量、快速的 Redis 可视化工具，基于 Tauri v2 + Vue 3 构建。

![Version](https://img.shields.io/badge/version-1.0.0-blue)
![License](https://img.shields.io/badge/license-Apache--2.0-blue)
![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey)

---

## ✨ 功能特性

- **连接配置** — 卡片式多实例配置，SSL/TLS 加密，系统 Keychain 密钥存储（AES-GCM），连通性测试，16 个 DB 切换
- **数据浏览** — `:` 分隔的树形级联目录 + 虚拟滚动，String / Hash / List / Set / ZSet 五类型查看与内联编辑，TTL 环形进度条，长值浮窗查看，搜索防抖
- **实时监控** — QPS 趋势图，内存 / 命中率 / CPU 等关键指标仪表盘
- **Pipeline** — 可视化编排批量命令，拖拽排序，逐条结果与延迟统计（含 RTT 节省率），脚本加密保存 / 加载
- **沙箱模式** — 执行前 Diff 预览，内置写命令模板，快照与一键回滚（自动生成逆操作），危险命令前端拦截
- **设置中心** — 明暗主题切换，语言偏好持久化
- **跨平台** — 支持 Windows / macOS / Linux

## 🛠 技术栈

| 层级 | 技术 |
|------|------|
| 框架 | [Tauri v2](https://v2.tauri.app/)（Rust edition 2024） |
| 前端 | [Vue 3](https://vuejs.org/) + [TypeScript](https://www.typescriptlang.org/) |
| 构建 | [Vite 6](https://vite.dev/) |
| 样式 | [Tailwind CSS v4](https://tailwindcss.com/) |
| 状态 | [Pinia](https://pinia.vuejs.org/) |
| 路由 | [Vue Router 4](https://router.vuejs.org/) |
| 国际化 | [Vue I18n](https://vue-i18n.intlify.dev/) |
| 图标 | [Lucide](https://lucide.dev/) |
| 后端 | [Rust](https://www.rust-lang.org/) + [Tokio](https://tokio.rs/) |
| Redis 客户端 | [rust-redis](https://github.com/redis-rs/redis-rs) + [deadpool-redis](https://github.com/deadpool-rs/deadpool) |
| 加密 | [AES-GCM](https://docs.rs/aes-gcm) + 系统 Keychain（[keyring](https://crates.io/crates/keyring)） |

## 📁 项目结构

```
BreezeRESP/
├── src/                    # Vue 3 前端
│   ├── components/         # 级联树 / 图表 / 布局 / 共享组件
│   ├── i18n/               # 国际化 (zh-CN / en)
│   ├── stores/             # Pinia 状态
│   ├── views/              # 连接 / 浏览 / Pipeline / 沙箱
│   └── utils/              # 格式化 / 回滚逆运算 / 命令模板
├── src-tauri/              # Rust 后端
│   ├── src/commands/       # Tauri IPC 命令
│   └── src/core/           # 连接池 / 加密存储 / Keychain / 指标
├── scripts/                # 版本同步脚本
├── package.json
└── vite.config.ts
```

## 🚀 快速开始

**环境要求：** [Node.js](https://nodejs.org/) >= 18 · [Rust](https://www.rust-lang.org/tools/install) >= 1.77 · [Tauri Prerequisites](https://v2.tauri.app/start/prerequisites/)

```bash
npm install          # 安装依赖
npm run tauri:dev    # 开发运行
npm run tauri:build  # 构建生产包
```

产物位于 `src-tauri/target/release/bundle/`。仅前端调试可用 `npm run dev`。

## 📦 构建产物

| 平台 | 格式 |
|------|------|
| Windows | `.exe` (NSIS) |
| macOS | `.dmg` / `.app`（支持 x86_64 & ARM64） |
| Linux | `.deb` / `.AppImage`（支持 x86_64 & ARM64） |

## 📄 License

[Apache 2.0](LICENSE)

---

<div align="center">

**BreezeRESP** — 像微风一样轻盈的 Redis 可视化工具 🍃

Made with ❤️ by [unrealx766](https://github.com/unrealx766)

</div>
