# scrcpy-mac-ui

<div align="center">

**macOS 平台的 scrcpy 图形化管理工具**

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![macOS](https://img.shields.io/badge/macOS-11.0+-blue.svg)](https://www.apple.com/macos)
[![Tauri](https://img.shields.io/badge/Tauri-2.0-orange.svg)](https://tauri.app)

[English](#english) | [中文](#中文)

</div>

---

## 中文

### 简介

scrcpy-mac-ui 是 [scrcpy](https://github.com/Genymobile/scrcpy) 的 macOS 图形化前端，让 Android 设备投屏变得更简单、更强大。

**核心特性：**
- 🎮 **多设备管理** — 自动检测，一键切换，智能状态提示
- ⌨️ **Mac 键盘输入** — UHID 物理键盘模式，中英文流畅输入
- 📡 **无线投屏** — USB 引导 + 配对模式，无线自由
- 🎬 **录屏控制** — 一键录制，自定义保存位置
- 🎛️ **实时控制** — 屏幕开关、音频路由、导航快捷键
- 📦 **一键安装** — 自动化依赖安装，5 分钟上手

### 快速开始

#### 系统要求
- macOS 11.0 (Big Sur) 或更高版本
- Android 5.0+ 设备
- USB 数据线（首次配置）

#### 安装步骤

**方式 A：下载发行版（推荐）**

1. 从 [Releases](../../releases) 下载最新的 `scrcpy-mac-ui-installer-vX.X.X.zip`
2. 解压并运行 `install_deps.sh`（自动安装 adb + scrcpy）
3. 将 `scrcpy-mac-ui.app` 拖到 `/Applications/`
4. 右键打开（首次会被 macOS 拦截）

**方式 B：从源码构建**

```bash
# 安装依赖
brew install android-platform-tools scrcpy oven-sh/bun/bun
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 克隆仓库
git clone https://github.com/rockcingma/SCRCPY-macUI.git
cd SCRCPY-macUI

# 安装前端依赖
bun install

# 构建发布版本
bun run tauri build
```

输出位置：`src-tauri/target/release/bundle/macos/scrcpy-mac-ui.app`

#### 首次使用

1. **连接设备**
   - USB 连接 Android 设备
   - 启用「USB 调试」（设置 → 开发者选项）
   - 授权电脑访问

2. **启动投屏**
   - 打开 scrcpy-mac-ui
   - 设备自动检测（显示绿色 ✓）
   - 点击「高画质启动」

3. **配置 Mac 键盘输入**（首次）
   - 投屏后按 `⌘K`
   - 手机自动跳转到物理键盘设置
   - 点击激活 "English (US)" 或 "简体中文"
   - 返回，Mac 键盘即可输入

### 核心功能

#### 设备管理

**智能状态指示：**
- 🟢 **绿色** — 单设备已连接，可以启动
- 🔵 **蓝色** — 多设备，点击选择
- 🟡 **黄色** — 未授权，检查手机屏幕
- 🟠 **橙色** — adb 未安装
- ⚫ **灰色** — 未连接

**多设备支持：** 连接多台设备时自动显示下拉选择器，一键切换。

#### 投屏预设

| 预设 | 分辨率 | 码率 | 帧率 | 适用场景 |
|------|--------|------|------|----------|
| **高画质** | 1920px | 8M | 60fps | USB 连接，看视频 |
| **WiFi 均衡** | 1280px | 4M | 30fps | 无线连接，日常使用 |
| **游戏低延迟** | 1280px | 4M | 60fps | 游戏（无音频） |
| **省电** | 1024px | 2M | 30fps | 低电量时 |

#### Mac 键盘输入

**工作原理：**
- UHID 模式模拟物理 USB 键盘
- Mac 保持 ABC 英文输入法
- 手机输入法处理拼音转中文
- 支持中英混输、标点符号

**使用方式：**
1. Mac 切到 ABC 英文输入法（`Ctrl+Space`）
2. 在 Mac 键盘打拼音（如 `nihao`）
3. 手机输入法显示候选（你好、泥壕...）
4. 按空格或数字选词

#### 无线投屏

**方法 A：USB 引导（推荐）**
1. USB 连接设备
2. 点击「USB 启用无线」
3. 拔掉 USB 线
4. 输入 IP 连接（如 `192.168.1.100:5555`）

**方法 B：配对模式（Android 11+）**
1. 设备上启用「无线调试」
2. 点击「使用配对码配对设备」
3. 在 app 中输入 IP、端口、配对码
4. 配对成功后连接

#### 运行时控制

**悬浮窗功能：**
- 🔴 **录屏** — 一键开始/停止录制
- ◐ **屏幕开关** — 关闭手机屏幕但继续镜像
- ♪ **音频路由** — Mac 播放 / 手机扬声器切换
- ⌂ **导航快捷键** — Home、Back、Recents、电源、音量等

### scrcpy 快捷键

在 scrcpy 窗口中：

| 快捷键 | 功能 |
|--------|------|
| `⌘H` | 回到主屏幕 |
| `⌘B` | 返回键 |
| `⌘S` | 最近任务 |
| `⌘K` | 打开物理键盘设置 |
| `⌘O` | 开关手机屏幕 |
| `⌘↑` / `⌘↓` | 音量加减 |
| `⌘F` | 全屏 |

### 常见问题

<details>
<summary><b>设备显示「未连接」</b></summary>

1. 检查 USB 线是否支持数据传输
2. 启用「USB 调试」（设置 → 开发者选项）
3. 查看设备屏幕是否有授权提示
4. 运行 `adb devices` 验证连接

</details>

<details>
<summary><b>Mac 键盘输入无反应</b></summary>

**检查清单：**
- [ ] scrcpy 窗口是否有焦点？
- [ ] Mac 输入法是否是 ABC 英文？
- [ ] 物理键盘是否配置？（按 `⌘K` 检查）
- [ ] 英文输入是否可以？

如果英文可以但中文不行 → Mac 切到 ABC 英文输入法。

</details>

<details>
<summary><b>无线连接失败</b></summary>

1. 确认设备和 Mac 在同一 Wi-Fi
2. 用 USB 重新连接，点击「USB 启用无线」
3. 检查新的 IP 地址
4. 验证：`adb connect 192.168.1.100:5555`

</details>

<details>
<summary><b>app 无法打开（macOS 阻止）</b></summary>

首次打开未签名应用会被拦截：
- **方法 1**：右键 → 打开
- **方法 2**：系统设置 → 隐私与安全性 → 「仍要打开」

</details>

### 技术栈

- **前端**：React 18 + TypeScript + Vite
- **后端**：Rust + Tauri 2
- **构建工具**：Bun
- **依赖**：adb + scrcpy 4.0+

### 项目结构

```
scrcpy-mac-ui/
├── src/                    # React 前端
│   ├── Launcher.tsx       # 主界面（设备管理 + 预设）
│   ├── FloatPanel.tsx     # 悬浮窗（运行时控制）
│   └── types.ts           # 预设配置
├── src-tauri/             # Rust 后端
│   └── src/
│       ├── lib.rs        # Tauri 命令
│       ├── adb.rs        # adb 调用封装
│       └── scrcpy.rs     # scrcpy 进程管理
└── scripts/
    └── package.sh        # 打包脚本
```

### 路线图

- [ ] 代码签名（Apple Developer）
- [ ] 设备别名自定义
- [ ] 预设管理（保存自定义）
- [ ] 自动更新
- [ ] 多设备同时投屏

### 贡献

欢迎提交 Issue 和 Pull Request！

**开发环境：**
```bash
# 安装依赖
bun install

# 开发模式
bun run tauri dev

# 运行测试
bun run test

# 构建发布版
bun run tauri build
```

### 许可证

[MIT License](LICENSE)

### 致谢

- [scrcpy](https://github.com/Genymobile/scrcpy) — 强大的 Android 投屏工具
- [Tauri](https://tauri.app) — 跨平台桌面应用框架

---

## English

### Introduction

scrcpy-mac-ui is a macOS graphical frontend for [scrcpy](https://github.com/Genymobile/scrcpy), making Android screen mirroring simpler and more powerful.

**Key Features:**
- 🎮 **Multi-device Management** — Auto-detection, one-click switching, smart status indicators
- ⌨️ **Mac Keyboard Input** — UHID physical keyboard mode, seamless typing in any language
- 📡 **Wireless Mirroring** — USB bootstrap + pairing mode, cable-free experience
- 🎬 **Recording Control** — One-click recording with custom save location
- 🎛️ **Runtime Controls** — Screen on/off, audio routing, navigation shortcuts
- 📦 **One-Click Installation** — Automated dependency setup, ready in 5 minutes

### Quick Start

#### System Requirements
- macOS 11.0 (Big Sur) or later
- Android 5.0+ device
- USB cable (for initial setup)

#### Installation

**Option A: Download Release (Recommended)**

1. Download latest `scrcpy-mac-ui-installer-vX.X.X.zip` from [Releases](../../releases)
2. Extract and run `install_deps.sh` (auto-installs adb + scrcpy)
3. Drag `scrcpy-mac-ui.app` to `/Applications/`
4. Right-click → Open (first time only)

**Option B: Build from Source**

```bash
# Install dependencies
brew install android-platform-tools scrcpy oven-sh/bun/bun
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone repository
git clone https://github.com/rockcingma/SCRCPY-macUI.git
cd SCRCPY-macUI

# Install frontend dependencies
bun install

# Build release
bun run tauri build
```

Output: `src-tauri/target/release/bundle/macos/scrcpy-mac-ui.app`

#### First-Time Setup

1. **Connect Device**
   - Connect Android device via USB
   - Enable USB debugging (Settings → Developer Options)
   - Authorize computer access

2. **Start Mirroring**
   - Open scrcpy-mac-ui
   - Device auto-detected (green ✓)
   - Click "High Quality Launch"

3. **Configure Mac Keyboard** (first time)
   - Press `⌘K` after mirroring starts
   - Phone opens physical keyboard settings
   - Select "English (US)" or your language
   - Return, Mac keyboard now works

### Core Features

#### Device Management

**Smart Status Indicators:**
- 🟢 **Green** — Single device connected, ready to launch
- 🔵 **Blue** — Multiple devices, click to select
- 🟡 **Yellow** — Unauthorized, check phone screen
- 🟠 **Orange** — adb not installed
- ⚫ **Gray** — No device connected

**Multi-device Support:** Automatic dropdown selector when multiple devices are connected.

#### Mirroring Presets

| Preset | Resolution | Bitrate | FPS | Use Case |
|--------|------------|---------|-----|----------|
| **High Quality** | 1920px | 8M | 60fps | USB, watching videos |
| **WiFi Balanced** | 1280px | 4M | 30fps | Wireless, daily use |
| **Game Low Latency** | 1280px | 4M | 60fps | Gaming (no audio) |
| **Power Saving** | 1024px | 2M | 30fps | Low battery |

#### Mac Keyboard Input

**How It Works:**
- UHID mode simulates physical USB keyboard
- Mac stays in ABC (English) input method
- Phone's IME handles pinyin/typing
- Supports multilingual input

**Usage:**
1. Switch Mac to ABC input method (`Ctrl+Space`)
2. Type in Mac keyboard (e.g., `nihao` for Chinese)
3. Phone IME shows candidates (你好, 泥壕...)
4. Press space or number to select

#### Wireless Mirroring

**Method A: USB Bootstrap (Recommended)**
1. Connect via USB
2. Click "Enable Wireless via USB"
3. Unplug USB cable
4. Connect with IP (e.g., `192.168.1.100:5555`)

**Method B: Pairing (Android 11+)**
1. Enable "Wireless Debugging" on device
2. Click "Pair device with pairing code"
3. Enter IP, port, and pairing code in app
4. Connect after successful pairing

#### Runtime Controls

**Float Panel Features:**
- 🔴 **Recording** — One-click start/stop
- ◐ **Screen Toggle** — Turn off phone screen, keep mirroring
- ♪ **Audio Routing** — Mac speakers / Phone speakers
- ⌂ **Navigation Shortcuts** — Home, Back, Recents, Power, Volume, etc.

### scrcpy Shortcuts

In scrcpy window:

| Shortcut | Function |
|----------|----------|
| `⌘H` | Home |
| `⌘B` | Back |
| `⌘S` | Recents |
| `⌘K` | Open Physical Keyboard Settings |
| `⌘O` | Toggle Screen On/Off |
| `⌘↑` / `⌘↓` | Volume Up/Down |
| `⌘F` | Fullscreen |

### FAQ

<details>
<summary><b>Device shows "Not Connected"</b></summary>

1. Check USB cable supports data transfer
2. Enable USB debugging (Settings → Developer Options)
3. Look for authorization prompt on device screen
4. Verify with `adb devices`

</details>

<details>
<summary><b>Mac Keyboard Not Working</b></summary>

**Checklist:**
- [ ] Is scrcpy window focused?
- [ ] Is Mac in ABC (English) input method?
- [ ] Is physical keyboard configured? (Press `⌘K` to check)
- [ ] Does English typing work?

If English works but Chinese doesn't → Switch Mac to ABC input method.

</details>

<details>
<summary><b>Wireless Connection Failed</b></summary>

1. Ensure device and Mac on same Wi-Fi
2. Reconnect via USB, click "Enable Wireless via USB"
3. Check new IP address
4. Verify: `adb connect 192.168.1.100:5555`

</details>

<details>
<summary><b>App Won't Open (macOS Blocking)</b></summary>

First launch of unsigned app is blocked:
- **Method 1**: Right-click → Open
- **Method 2**: System Settings → Privacy & Security → "Open Anyway"

</details>

### Technology Stack

- **Frontend**: React 18 + TypeScript + Vite
- **Backend**: Rust + Tauri 2
- **Build Tool**: Bun
- **Dependencies**: adb + scrcpy 4.0+

### Contributing

Issues and Pull Requests are welcome!

**Development Setup:**
```bash
# Install dependencies
bun install

# Development mode
bun run tauri dev

# Run tests
bun run test

# Build release
bun run tauri build
```

### License

[MIT License](LICENSE)

### Acknowledgments

- [scrcpy](https://github.com/Genymobile/scrcpy) — Powerful Android mirroring tool
- [Tauri](https://tauri.app) — Cross-platform desktop framework

---

<div align="center">

**Made with ❤️ for macOS users**

[Report Bug](../../issues) · [Request Feature](../../issues) · [View Docs](../../wiki)

</div>
