# scrcpy-mac-ui

一个 macOS 原生的 scrcpy 控制器。消除命令行记忆负担——零参数启动、零快捷键记忆。

## 技术栈

- **桌面框架**: Tauri 2 (Rust 后端 + React 前端)
- **前端**: React + TypeScript + Vite
- **按键注入**: AppleScript → scrcpy 窗口 (50ms)
- **原生质感**: tauri-plugin-macos-vibrancy (真 NSVisualEffectView)

## 开发

```bash
# 安装依赖
bun install

# 开发模式 (需要 Rust/cargo)
bun run tauri dev

# 测试
bun run test              # 前端 (Vitest)
cargo test --manifest-path src-tauri/Cargo.toml  # 后端

# 构建
bun run tauri build
```

## 项目状态

**Phase 1 (MVP)** — 已完成代码,待验证:
- ✅ PRD v0.3 (CEO/Design/Eng 三轮评审通过)
- ✅ Tauri 2 脚手架 + React UI
- ✅ 前端单元测试 33 passing
- ✅ adb.rs (设备解析 6 态 + PATH 探测 + 单测)
- ✅ scrcpy.rs (spawn + 异步消费 stdout/stderr + kill 互锁 + 单测)
- ⏳ 等待 Rust 工具链完成安装以验证后端

详见 [docs/PRD.md](docs/PRD.md)。
