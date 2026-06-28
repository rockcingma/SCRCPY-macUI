# scrcpy-mac-ui — PRD v0.3 (FINAL)

**版本**：v0.3 | **日期**：2026-06-28 | **状态**：APPROVED — ready to implement
**评审**：CEO Review ✓ / Design Review (9/10) ✓ / Eng Review ✓

## 一、背景与目标

scrcpy 4.0 是命令行工具，痛点两层：启动参数难记 + 运行时快捷键难记。本应用消除两层记忆负担——零命令行、零快捷键记忆，所有操作通过按钮完成。

## 二、技术架构

```
桌面框架     Tauri 2 (Rust 后端 + WebView 前端)
前端         React + TypeScript + Tailwind CSS 4
按键注入     AppleScript → scrcpy SDL 窗口 (50ms 延迟，走 scrcpy 自己的快通道)
进程管理     tokio::process::Command spawn + 异步消费 stdout/stderr
设备检测     adb devices CLI + 节流轮询（前台 1Hz，后台暂停）
持久化       tauri-plugin-store
原生质感     tauri-plugin-macos-vibrancy（NSVisualEffectView 真原生）
分发         本地 cargo tauri build → 拖 .app 到 /Applications
```

**关键架构决策（来自评审）：**

| 决策 | 选择 | 理由 |
|---|---|---|
| 实现路径 | Tauri + WebView | React 熟悉，CC 加持下质量高 |
| 主操作 | 上次预设置顶 | 个人工具习惯固定，零思考负担 |
| 关闭行为 | 收到 menubar tray | macOS 原生习惯 |
| 按键注入 | AppleScript → scrcpy | adb keyevent 0.5–1s 延迟不可用 |
| Vibrancy | 原生插件 | 1 行代码换 100% 原生质感 |
| 测试 | 完整覆盖 | Boil the Lake，CC 加持下边际成本接近零 |

## 三、功能规格

### 3.1 启动器主窗（420×640，vibrancy 背景）

```
┌──────────────────────────────────────────┐
│ ● ● ●          scrcpy Controller          │
├──────────────────────────────────────────┤
│  🟢 Pixel 7                       [🔄]    │
│     R5CX21RJ6MX                          │
├──────────────────────────────────────────┤
│  ┌────────────────────────────────────┐  │
│  │  ⚡  高画质启动                    │  │  ← 上次预设
│  │  ·  1920px · 8M · 60fps           │  │
│  └────────────────────────────────────┘  │
│  ⊙ WiFi   ⊙ 游戏   ⊙ 省电                │  ← 次级 32px
│  ⊙ 演示   ⊙ 录屏                         │
├──────────────────────────────────────────┤
│  🔗 无线连接                             │
│  [ 192.168.____ : 5555 ]   [ 连接 ]       │
│  最近: 192.168.1.100 · 192.168.1.105     │
├──────────────────────────────────────────┤
│  ▸ 高级参数                              │
└──────────────────────────────────────────┘
```

### 3.2 运行时悬浮面板（48×540，竖排）

```
图标全部用 SF Symbols（导出为 SVG 内嵌，禁止 emoji）：
house.fill / arrow.left / square.stack / lock.fill /
camera.fill / speaker.wave.3.fill / speaker.wave.1.fill /
bell.fill / rotate.right.fill / xmark.circle.fill

交互：
- 默认位置：scrcpy 所在屏幕右边缘 -16px 垂直居中
- 拖动：mousedown 期间内存 state；mouseup 后 debounce 500ms 写盘
- 透明度：默认 88%，hover 100%
- 按下反馈：80ms accent outline 闪烁（无论后端是否成功）
- 失败反馈：再 150ms 红色闪烁
- 跨 Space：set_visible_on_all_workspaces(true) + level=floating
- 生命周期：scrcpy spawn 后 200ms 淡入；scrcpy exit event 后 1s 淡出
```

### 3.3 状态覆盖规范

**设备状态（6 种态）：**

| 状态 | UI 表现 |
|---|---|
| 加载 | 🔄 灰圆 + "正在检测设备..." |
| 空 | ⚫ 灰圆 + "未检测到设备" + 「如何启用？」可展开三步说明 |
| 未授权 | 🟡 黄圆 + "设备已连接，等待授权" + 操作提示 |
| adb 缺失 | 🟠 橙圆 + "未找到 adb" + 一键安装按钮（brew install android-platform-tools） |
| 成功 | 🟢 绿圆 + 设备型号 + 序列号灰小字 |
| 多设备 | 🔵 蓝圆 + 下拉选择器 + "共 N 台" |

**启动 scrcpy 流程：**
- 加载：主按钮变 "启动中..." + 4 阶段进度文本 + 禁用其他按钮
- 错误：顶部红横条 "{stderr 末行}" + [重试] [查看日志]，8s 自动收起
- 成功：主窗自动隐藏到 menubar，浮窗淡入

**无线连接：**
- 加载：按钮变 "连接中..." 3s 超时
- 错误：人话化映射，`failed to connect` → "目标设备未开启 5555 端口，请先 USB + adb tcpip 5555"
- 成功：IP 字段下方绿色 ✓"已连接"

**按键注入：**
- 默认 AppleScript 到 scrcpy 窗口（50ms）
- 失败：按钮 150ms 红闪 + menubar 红点
- 首次失败检测到 Accessibility 拒绝：弹窗引导授权 + 跳「系统设置 → 隐私 → 辅助功能」

### 3.4 视觉规范（CSS tokens）

```css
:root {
  --accent: #007AFF;
  --text: rgba(0,0,0,0.85);
  --text-secondary: rgba(0,0,0,0.55);
  --hairline: rgba(0,0,0,0.10);
  --radius-md: 8px;
  --radius-sm: 6px;
  --shadow-float: 0 4px 16px rgba(0,0,0,0.15);
  --space-unit: 8px;
  --font-display: -apple-system, BlinkMacSystemFont, "SF Pro Display";
  --duration-fast: 80ms;
  --duration-normal: 200ms;
}
@media (prefers-color-scheme: dark) {
  :root {
    --accent: #0A84FF;
    --text: rgba(255,255,255,0.92);
    --text-secondary: rgba(255,255,255,0.55);
    --hairline: rgba(255,255,255,0.10);
  }
}
```

字体 SF Pro Display 14px / 16px semibold；圆角全局 8px、按钮内 6px；间距 8px 网格；动效仅淡入 200ms + 按键反馈 80ms。

### 3.5 键盘导航

- Tab 顺序：设备 → 主预设 → 次预设组 → IP → 高级折叠
- 全局快捷键：`Cmd+1～5` 触发 5 个预设，`Esc` 关闭浮窗

## 四、错误模型（统一）

```rust
#[derive(Debug, thiserror::Error, serde::Serialize)]
#[serde(tag = "kind", content = "message")]
pub enum AppError {
  #[error("adb 未找到")]                AdbNotFound,
  #[error("设备未连接")]                DeviceNotConnected,
  #[error("scrcpy 启动失败: {0}")]      ScrcpyLaunchFailed(String),
  #[error("AppleScript 注入失败: {0}")] KeyInjectFailed(String),
  #[error("Accessibility 权限被拒")]    AccessibilityDenied,
  #[error("无线连接失败: {0}")]         WirelessConnectFailed(String),
  #[error("IO: {0}")]                   Io(String),
}
```

所有 `#[tauri::command]` 返回 `Result<T, AppError>`，前端 type-safe 接收。

## 五、关键实现规范

### 5.1 PATH 探测（M 系 Mac 兼容）
候选顺序：`/opt/homebrew/bin` → `/usr/local/bin` → `$HOME/Library/Android/sdk/platform-tools`。adb / scrcpy 同样处理。

### 5.2 子进程安全 spawn
严格用 `args` 不走 shell。输入白名单：serial `^[A-Za-z0-9]{8,32}$`，ip `^\d{1,3}(\.\d{1,3}){3}(:\d{1,5})?$`。

### 5.3 进程生命周期互锁
1. App 退出 → SIGTERM scrcpy → 2s → SIGKILL
2. App 启动 → pgrep -f scrcpy → 残留提示清理
3. child.wait() → emit "scrcpy-stopped" → 浮窗淡出
4. spawn 后立即两个 tokio 任务消费 stdout/stderr → 防 pipe buffer 阻塞

### 5.4 设备轮询节流
主窗可见 1Hz；收 menubar 停止；唤回立刻刷 + 恢复；手动刷新按钮立刻刷。

## 六、实现路线图

- **Phase 1（MVP）**：Tauri 双窗脚手架 + vibrancy；adb.rs（6 态 + PATH）+ 单测；scrcpy.rs（spawn + 异步消费 + kill 互锁）+ 单测；启动器 UI（设备卡片所有态 + 预设 + 上次预设记忆）
- **Phase 2（浮窗）**：FloatPanel（拖动 debounce + 跨 Space + level=floating）；keyinject.rs（osascript + Accessibility 处理）+ 集成测试；按键反馈动效
- **Phase 3（完善）**：无线连接 + IP 历史；录屏快捷启动；menubar tray + Cmd+1～5；Playwright + tauri-driver E2E 4 流程

## 七、测试策略

```
Rust 后端   cargo test    单元 + 集成（mock adb/scrcpy 二进制）
前端        Vitest + @testing-library/react
E2E         Playwright + tauri-driver    启动 / 浮窗按键 / 关闭 / 残留清理
```

完整覆盖：45 个 GAP 全部对应 test cases，按 Phase 同步写。

## 八、TODO / 未来扩展

- [ ] tauri-updater + GitHub Releases auto-update
- [ ] mDNS 自动发现局域网设备
- [ ] 多设备同时投屏
- [ ] APK 安装与文件管理面板
- [ ] 国际化（i18n）

## 九、Distribution

```bash
cargo tauri build
cp -R "src-tauri/target/release/bundle/macos/scrcpy-mac-ui.app" /Applications/
```

升级时重复（覆盖旧版），无需 notarize。
