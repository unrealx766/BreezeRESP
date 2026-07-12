# BreezeRESP

[English](README_EN.md) | 中文
> A lightweight and fast Redis GUI tool built with Tauri v2 + Vue 3.

![Version](https://img.shields.io/badge/version-1.0.0-blue)
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

## 📸 Screenshots

| Connection Management | Dark Mode |
|:---:|:---:|
| ![Connection Management](https://raw.githubusercontent.com/unrealx766/BreezeRESP/screenshots/home.png) | ![Dark Mode](https://raw.githubusercontent.com/unrealx766/BreezeRESP/screenshots/dark_mode.png) |

| Data Browser | Pipeline Builder |
|:---:|:---:|
| ![Data Browser](https://raw.githubusercontent.com/unrealx766/BreezeRESP/screenshots/data_browser.png) | ![Pipeline Builder](https://raw.githubusercontent.com/unrealx766/BreezeRESP/screenshots/Pipeline.png) |

| Command Sandbox |
|:---:|
| ![Command Sandbox](https://raw.githubusercontent.com/unrealx766/BreezeRESP/screenshots/shadow.png) |

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

| Platform | Format | Command | Notes |
|----------|--------|---------|-------|
| Windows | `.exe` (NSIS) | `npm run tauri:build:win` | Installer with custom path support |
| macOS (Intel) | `.dmg` / `.app` | `npm run tauri:build:mac` | x86_64 |
| macOS (Apple Silicon) | `.dmg` / `.app` | `npm run tauri:build:mac-arm` | aarch64 |
| Linux (Debian-based) | `.deb` / `.AppImage` | `npm run tauri:build:linux` | UOS / Ubuntu / Deepin |
| Linux (RPM-based) | `.rpm` | `npm run tauri:build:linux-rpm` | CentOS / Kylin / openEuler |
| Linux ARM64 | `.deb` / `.AppImage` | `npm run tauri:build:linux-arm` | aarch64 |

> **Note:** RPM packages must be built on RPM-based distributions (Fedora / CentOS / RHEL). Cross-building from Debian/Ubuntu is not supported. A Fedora runner is configured in CI for automated builds.

## 📄 License

[Apache 2.0](LICENSE)

## ⚠️ Disclaimer

1. **Data Safety** — BreezeRESP is a Redis client tool that directly performs read/write operations on target databases. Users assume all data risks, including but not limited to data loss, corruption, or service interruption caused by misoperation. It is recommended to back up data before operating on production environments.
2. **Sandbox & Rollback** — The preview and rollback features in Sandbox mode are provided for reference only and are not guaranteed to be fully accurate in all scenarios. Users should carefully review changes before executing write operations and verify rollback results.
3. **Connection Security** — This tool uses AES-256-GCM encryption to store connection information and manages keys via the system Keychain. However, it assumes no responsibility for information disclosure resulting from improper OS security configurations, third-party malware, or network attacks.
4. **Third-Party Dependencies** — This project depends on multiple open-source components (Tauri, Vue, Rust crates, etc.). The security and stability of each component are the responsibility of their respective maintainers.
5. **No Warranty** — This software is provided "as is" without any express or implied warranty, including but not limited to warranties of merchantability, fitness for a particular purpose, and non-infringement. In no event shall the authors or copyright holders be liable for any claim, damages, or other liability arising from, out of, or in connection with the software or the use or other dealings in the software.

---

<div align="center">

**BreezeRESP** — A Redis GUI tool as light as a breeze 🍃

Made with ❤️ by [unrealx766](https://github.com/unrealx766)

</div>
