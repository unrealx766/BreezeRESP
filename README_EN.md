# BreezeRESP

[English](README_EN.md) | 中文
> A lightweight and fast Redis GUI tool built with Tauri v2 + Vue 3.

![Version](https://img.shields.io/badge/version-0.1.0-blue)
![License](https://img.shields.io/badge/license-Apache--2.0-blue)
![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey)

---

## ✨ Features

- **Connection Config** — Card-based multi-instance configuration, SSL/TLS encryption, system Keychain secret storage (AES-GCM), connectivity testing, 16 DB switching
- **Data Browser** — `:`-delimited cascading tree directory with virtual scrolling, view & inline edit for String / Hash / List / Set / ZSet, TTL ring gauge, floating window for long values, debounced search
- **Real-time Monitoring** — QPS trend chart, dashboard for memory / hit rate / CPU and other key metrics
- **Pipeline** — Visual batch command orchestration, drag-and-drop sorting, per-command result & latency stats (with RTT savings), encrypted script save / load
- **Sandbox Mode** — Pre-execution diff preview, built-in write command templates, snapshot & one-click rollback (auto-generated inverse operations), dangerous command frontend interception
- **Settings Center** — Dark / light theme toggle, persistent language preference
- **Cross-Platform** — Supports Windows / macOS / Linux

## 🛠 Tech Stack

| Layer | Technology |
|-------|------------|
| Framework | [Tauri v2](https://v2.tauri.app/) (Rust edition 2024) |
| Frontend | [Vue 3](https://vuejs.org/) + [TypeScript](https://www.typescriptlang.org/) |
| Build | [Vite 6](https://vite.dev/) |
| Styling | [Tailwind CSS v4](https://tailwindcss.com/) |
| State | [Pinia](https://pinia.vuejs.org/) |
| Router | [Vue Router 4](https://router.vuejs.org/) |
| i18n | [Vue I18n](https://vue-i18n.intlify.dev/) |
| Icons | [Lucide](https://lucide.dev/) |
| Backend | [Rust](https://www.rust-lang.org/) + [Tokio](https://tokio.rs/) |
| Redis Client | [rust-redis](https://github.com/redis-rs/redis-rs) + [deadpool-redis](https://github.com/deadpool-rs/deadpool) |
| Encryption | [AES-GCM](https://docs.rs/aes-gcm) + System Keychain ([keyring](https://crates.io/crates/keyring)) |

## 📁 Project Structure

```
BreezeRESP/
├── src/                    # Vue 3 frontend
│   ├── components/         # Cascade tree / charts / layout / shared components
│   ├── i18n/               # Internationalization (zh-CN / en)
│   ├── stores/             # Pinia state management
│   ├── views/              # Connection / Browser / Pipeline / Sandbox
│   └── utils/              # Formatting / rollback inverse / command templates
├── src-tauri/              # Rust backend
│   ├── src/commands/       # Tauri IPC commands
│   └── src/core/           # Connection pool / encrypted storage / Keychain / metrics
├── scripts/                # Version sync scripts
├── package.json
└── vite.config.ts
```

## 🚀 Quick Start

**Prerequisites:** [Node.js](https://nodejs.org/) >= 18 · [Rust](https://www.rust-lang.org/tools/install) >= 1.77 · [Tauri Prerequisites](https://v2.tauri.app/start/prerequisites/)

```bash
npm install          # Install dependencies
npm run tauri:dev    # Run in development mode
npm run tauri:build  # Build production bundle
```

Build output is located in `src-tauri/target/release/bundle/`. Use `npm run dev` for frontend-only debugging.

## 📦 Build Artifacts

| Platform | Format |
|----------|--------|
| Windows | `.exe` (NSIS) |
| macOS | `.dmg` / `.app` (x86_64 & ARM64) |
| Linux | `.deb` / `.AppImage` (x86_64 & ARM64) |

## 📄 License

[Apache 2.0](LICENSE)

---

<div align="center">

**BreezeRESP** — A Redis GUI tool as light as a breeze 🍃

Made with ❤️ by [unrealx766](https://github.com/unrealx766)

</div>
