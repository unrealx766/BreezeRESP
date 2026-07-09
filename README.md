# BreezeRESP

> 一款轻量、快速的 Redis 桌面客户端，基于 Tauri v2 + Vue 3 构建。

![Version](https://img.shields.io/badge/version-0.1.0-blue)
![License](https://img.shields.io/badge/license-GPL--3.0-blue)
![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey)

---

BreezeRESP 是一款基于 Tauri v2 构建的原生桌面 Redis 可视化。提供从连接管理到数据浏览、实时监控、批量执行、沙箱预演的完整 Redis 操作体验，支持中英文界面。

## ✨ 功能特性

- **连接管理** — 卡片式管理多个 Redis 实例，SSL/TLS 加密连接，AES-GCM 本地加密存储密码，连接状态实时指示，一键测试连通性，16 个 DB 快速切换
- **数据浏览** — 树形级联目录按 `:` 自动折叠键层级 + 虚拟滚动万级 Key 流畅加载，完整支持 String / Hash / List / Set / ZSet 五种数据类型的查看与内联编辑，TTL 环形进度条可视化，长值浮动窗口查看
- **实时监控** — QPS 趋势图实时刷新，内存用量、命中率、CPU 使用等关键指标仪表盘
- **Pipeline 模式** — 可视化编排批量命令，拖拽排序，一键执行查看逐条结果与延迟统计，脚本可保存/加载/导入/导出
- **沙箱模式** — 命令执行前预览效果与数据变更 Diff，内置常用命令模板，快照与一键回滚，安全操作不翻车
- **国际化** — 内置中文 / English 双语支持，一键切换

## 🛠 技术栈

| 层级 | 技术 |
|------|------|
| 框架 | [Tauri v2](https://v2.tauri.app/) |
| 前端 | [Vue 3](https://vuejs.org/) + [TypeScript](https://www.typescriptlang.org/) |
| 构建 | [Vite 6](https://vite.dev/) |
| 样式 | [Tailwind CSS v4](https://tailwindcss.com/) |
| 状态 | [Pinia](https://pinia.vuejs.org/) |
| 路由 | [Vue Router 4](https://router.vuejs.org/) |
| 国际化 | [Vue I18n](https://vue-i18n.intlify.dev/) |
| 图标 | [Lucide](https://lucide.dev/) |
| 后端 | [Rust](https://www.rust-lang.org/) + [Tokio](https://tokio.rs/) |
| Redis 客户端 | [deadpool-redis](https://github.com/deadpool-rs/deadpool) |
| 加密 | [AES-GCM](https://docs.rs/aes-gcm) |

## 📁 项目结构

```
BreezeRESP/
├── src/                        # 前端源码
│   ├── components/
│   │   ├── cascade/            # 级联树组件
│   │   ├── charts/             # 图表组件 (QPS / TTL)
│   │   └── layout/             # 布局组件 (Header / Sidebar / StatusBar)
│   ├── i18n/                   # 国际化配置
│   ├── mocks/                  # Mock 数据
│   ├── router/                 # 路由配置
│   ├── services/               # Tauri 命令封装
│   ├── stores/                 # Pinia 状态管理
│   ├── types/                  # TypeScript 类型定义
│   ├── views/                  # 页面视图
│   ├── App.vue                 # 根组件
│   └── main.ts                 # 入口
├── src-tauri/                  # Rust 后端
│   ├── src/
│   │   ├── commands/           # Tauri 命令 (连接/级联/指标/Pipeline/沙箱)
│   │   ├── core/               # 核心模块 (连接池/配置/指标/预取/加密存储)
│   │   ├── lib.rs
│   │   └── main.rs
│   ├── icons/                  # 应用图标
│   └── tauri.conf.json         # Tauri 配置
├── public/                     # 静态资源
├── package.json
├── vite.config.ts
└── tsconfig.json
```

## 🚀 快速开始

### 环境要求

- [Node.js](https://nodejs.org/) >= 18
- [Rust](https://www.rust-lang.org/tools/install) >= 1.70
- [Tauri Prerequisites](https://v2.tauri.app/start/prerequisites/)

### 安装依赖

```bash
npm install
```

### 开发运行

```bash
npm run tauri:dev
```

### 构建生产包

```bash
npm run tauri:build
```

构建产物位于 `src-tauri/target/release/bundle/`。

### 仅前端开发（不启动 Tauri）

```bash
npm run dev
```

## 📦 构建产物

| 平台 | 格式 |
|------|------|
| Windows | `.msi` / `.exe` (NSIS) |
| macOS | `.dmg` / `.app` |
| Linux | `.deb` / `.AppImage` |

## 🎨 设计规范

- **主色调**：Redis 红 `#DC382D`
- **背景色**：深灰 `#1a1a2e` → `#16213e` 渐变
- **字体**：Inter（UI 文本）+ JetBrains Mono（代码/数据）
- **图标**：Lucide Icons，统一 `stroke-width: 2`


## 📄 License

[GPL-3.0](LICENSE) — 本项目采用 GNU General Public License v3 开源协议。

- 你可以自由使用、修改和分发本软件
- 分发修改版本时必须同样以 GPL v3 协议开源
- 本软件不提供任何担保

---

<div align="center">

**BreezeRESP** — 像微风一样轻盈的 Redis 桌面客户端 🍃

Made with ❤️ by [unrealx766](https://github.com/unrealx766)

</div>
