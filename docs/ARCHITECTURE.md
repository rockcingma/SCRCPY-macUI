# 架构地图 — scrcpy-mac-ui

> 这份文档是代码的「单一事实来源」。PRD 说**做什么**,这份说**代码怎么组织**。
> 每次改动前先对照本文档;改动后若违反了这里的约定,要么改代码,要么更新文档。
> 文档与代码不一致 = bug 的温床(录屏 bug 就是这么来的)。

## 1. 全局结构

```
scrcpy-mac-ui/
├── docs/
│   ├── PRD.md            产品需求(做什么)
│   └── ARCHITECTURE.md   本文档(怎么组织)
├── src/                  前端 (React + TypeScript)
│   ├── types.ts          领域类型 + 预设表 + 浮窗按钮表(镜像后端)
│   ├── backend.ts        IPC 接口层(可注入,测试用 fake 替换)
│   ├── device.ts         设备状态推导(纯逻辑)
│   ├── store/settings.ts 设置持久化逻辑(纯逻辑)
│   ├── Launcher.tsx       主窗 UI(设备卡片 + 预设启动)
│   ├── FloatPanel.tsx     浮窗 UI(运行时操作面板)
│   ├── App.tsx            路由(#float vs 主窗)+ 生命周期事件桥接
│   └── styles.css         设计 token + 组件样式
└── src-tauri/src/        后端 (Rust + Tauri 2)
    ├── lib.rs            Tauri 命令注册 + AppState + 事件发射
    ├── error.rs         统一错误模型 AppError(serde-tagged)
    ├── adb.rs           设备检测 + adb 路径探测 + devices 解析
    ├── scrcpy.rs        scrcpy 进程 spawn/kill + 参数构建 + 输入校验
    └── keyinject.rs     按键注入(adb keyevent)+ 录屏/旋转等动作
```

## 2. 两个核心概念边界(最重要,违反就出 bug)

代码里有两类**性质完全不同**的用户操作,绝不能混淆:

### 预设 (Preset) — 启动配置
- **定义**:`types.ts` 的 `PRESETS` 数组
- **语义**:scrcpy **启动时**的一组参数(画质/帧率/比特率)。一次性的、无状态的。
- **载体**:主窗 `Launcher.tsx` 的预设网格
- **行为**:点击 → `launchScrcpy(serial, preset)` → 启动新的 scrcpy 进程
- **副作用**:会被记为「上次预设」(`onPresetUsed`),下次置顶
- **约束**:预设只能影响**启动那一刻**。任何"运行中才有意义"的功能都不是预设。

### 运行时操作 (Runtime Action) — 对运行中 scrcpy 的控制
- **定义**:`types.ts` 的 `FLOAT_BUTTONS` 数组 + `KeyAction` 枚举
- **语义**:scrcpy **已经在跑**时,对设备/进程的控制(Home/返回/截图/录制开关...)。可能有状态。
- **载体**:浮窗 `FloatPanel.tsx`
- **行为**:点击 → `sendKey(action)` → 后端分发到 adb/进程控制
- **副作用**:**不**记为「上次预设」(它不是启动配置)
- **约束**:运行时操作只在 scrcpy 运行时可用(浮窗只在 scrcpy 启动后显示)。

> **录屏 bug 的教训**:录屏被错误地放进了 PRESETS。它需要「开始→录制中→停止」的状态,
> 是典型的运行时操作,不是启动配置。放错类别导致:① 点击只是用空参数启动普通投屏
> ② 被记为「上次预设」污染主按钮。修复 = 把它移到运行时操作类别。

## 3. 数据流(点一个按钮发生什么)

### 3.1 启动投屏(预设)

```
[主窗] 用户点"高画质启动"
  │
  ▼ Launcher.tsx: launch(preset)
backend.launchScrcpy(serial, preset)
  │
  ▼ IPC: invoke("launch_scrcpy", {serial, args: preset.args})
[Rust] lib.rs: launch_scrcpy(serial, args)
  │
  ├─▶ scrcpy::launch(serial, args)  ── spawn scrcpy ──▶ 两个 tokio 任务消费 stdout/stderr
  │     (300ms 后检查是否秒退 → 失败则返回 stderr 末行)
  ├─▶ AppState.child = child;  AppState.serial = serial
  ├─▶ app.emit("scrcpy-started")  ─────────────┐
  └─▶ spawn_lifecycle_watcher()                │
        (轮询 child,退出时 emit "scrcpy-stopped")│
                                                ▼
[主窗] App.tsx 监听 "scrcpy-started"
  └─▶ 找到 float 窗 → show() + 定位到屏幕右边 + setAlwaysOnTop
```

### 3.2 运行时按键(浮窗)

```
[浮窗] 用户点"主屏幕"
  │
  ▼ FloatPanel.tsx: press("home")
  ├─▶ 80ms 蓝边闪烁(立即,不等后端)
  └─▶ backend.sendKey("home")
        │
        ▼ IPC: invoke("send_key", {action: "home"})
      [Rust] lib.rs: send_key(action)
        ├─ 取 AppState.serial
        ├─ Close      → stop_scrcpy()(杀进程 + emit scrcpy-stopped)
        ├─ Screenshot → take_screenshot_inner()(adb exec-out screencap → 桌面 PNG)
        ├─ Rotate     → keyinject::rotate_screen()(settings 读改写)
        └─ 其他       → keyinject::inject(action, serial, adb_path)
                          └─▶ adb -s <serial> shell input keyevent <code>
```

**关键约束**:按键走 `adb shell input keyevent`(Android 键码),**不走 AppleScript/osascript**。
原因见 §6。

## 4. 状态模型 (AppState)

后端 `lib.rs::AppState` 是唯一的运行时状态来源:

```rust
struct AppState {
    child:  Arc<Mutex<Option<Child>>>,   // 当前 scrcpy 子进程(None = 没在跑)
    serial: Arc<Mutex<Option<String>>>,  // 当前投屏的设备序列号(截图/按键需要)
}
```

**不变量 (invariants)**:
- `child = Some` ⟺ scrcpy 正在跑 ⟺ 浮窗应该可见
- 启动新 scrcpy 前必须先 kill 旧 child(避免孤儿进程)
- child 退出(任何原因)→ watcher emit `scrcpy-stopped` → 浮窗隐藏
- serial 在 launch 时写入,用于后续所有 adb 操作

前端无持久运行时状态——它通过事件(`scrcpy-started`/`scrcpy-stopped`)被动反映后端状态。
唯一的前端持久状态是**设置**(`store/settings.ts`:上次预设 + IP 历史),存 tauri-plugin-store。

## 5. 契约表(IPC + 事件)

### IPC 命令(前端 backend.ts ↔ 后端 #[tauri::command])

| 前端方法 | 后端命令 | 参数 | 返回 | 作用 |
|---|---|---|---|---|
| `adbAvailable()` | `adb_available` | — | `bool` | adb 二进制是否存在 |
| `listDevices()` | `list_devices` | — | `Device[]` | `adb devices -l` 解析 |
| `launchScrcpy(serial,preset)` | `launch_scrcpy` | `{serial, args}` | — | 启动 scrcpy |
| `connectWireless(ip)` | `connect_wireless` | `{ip}` | — | `adb connect ip:5555` |
| `sendKey(action)` | `send_key` | `{action}` | — | 运行时操作分发 |

> 改 IPC 时:前端 `backend.ts` 的方法签名、后端 `#[tauri::command]` 签名、
> `invoke_handler!` 注册列表,三处必须同步。漏一处 = 运行时报错但测试不报。

### 事件(后端 emit → 前端 listen)

| 事件 | 发射时机 | 监听方 | 处理 |
|---|---|---|---|
| `scrcpy-started` | scrcpy 成功启动后 | App.tsx(主窗) | show 浮窗 + 定位 |
| `scrcpy-stopped` | child 退出(任何原因) | App.tsx(主窗) | hide 浮窗 |

> 事件桥接由**主窗**负责,不是浮窗——因为浮窗启动时是隐藏的,首个事件到达时
> 它可能还没加载 JS。

### Capability(src-tauri/capabilities/default.json)

新增任何 `window.*` API 调用(show/hide/setPosition/startDragging/currentMonitor 等)
都必须在 capability 里加对应 `core:window:allow-*` 权限,否则运行时静默失败。

## 6. 关键技术约束(为什么这么做,别推翻)

这些是用真机调试换来的硬约束。改之前先读这里,否则会重蹈覆辙。

### 6.1 按键走 adb keyevent,不走 AppleScript

**约束**:所有运行时按键 = `adb shell input keyevent <code>`(Android 键码)。

**为什么不用 AppleScript**(PRD D1 原本选了它,实测推翻):
- AppleScript 的 `keystroke` **永远发给 macOS 当前焦点窗口**,`tell process "scrcpy"`
  只能限定 UI 查询,管不了按键投递目标。
- scrcpy 的快捷键(Cmd+H Home / Cmd+S 多任务 / Cmd+P 锁屏)**与 macOS 系统快捷键冲突**
  (Hide / Save / Print)。点浮窗时焦点在 scrcpy-mac-ui 自己,Cmd+H 把**自己的窗口**隐藏了。
- adb keyevent 直达 Android input 子系统:不依赖窗口焦点、不触发 macOS 快捷键、
  **不需要 Accessibility 权限**。延迟 ~100-400ms,对偶尔点导航键完全够用。

**键码映射**(`keyinject.rs::adb_command`):Home=3, Back=4, Recents=187, Lock=26,
VolumeUp=24, VolumeDown=25。通知栏用 `cmd statusbar expand-notifications`(比 KEYCODE 可靠)。

### 6.2 子进程 stdout/stderr 必须排空

scrcpy 输出量大,若不读 stdout/stderr,64KB 管道缓冲满后 scrcpy 会**阻塞死**。
`scrcpy::launch` spawn 后立即起两个 tokio 任务持续 drain 到 `LogRing`(环形缓冲,
存最近 200 行,stderr 末行用于启动失败提示)。

### 6.3 kill 用 SIGTERM→2s→SIGKILL 阶梯

`scrcpy::kill` 先 `start_kill`(礼貌终止),轮询 2 秒,仍存活才强杀。避免孤儿进程
和未 finalize 的录制文件。

### 6.4 输入校验(注入防护)

所有进设备的字符串先过白名单:serial `^[A-Za-z0-9]{8,32}$`,ip `IPv4[:port]`。
全部用 argv 传参,**从不** `sh -c` 拼接。校验在 `scrcpy.rs` 和 `keyinject.rs` 各有一份
(就近防护)。

### 6.5 dev 模式 codesign(scripts/dev.sh)

macOS 26 (Tahoe) 把权限授予绑定代码签名 hash。cargo 每次编译产生新 hash,
导致授权失效。`scripts/dev.sh` 在 build 后用稳定 identifier 重签。
**注**:改用 adb 方案后已不需要 Accessibility,此约束对按键不再关键,但 release
打包仍需稳定签名,脚本保留。

## 7. 测试策略

**铁律:测真实行为,不测 mock 的回声。**

录屏 bug 暴露了一个反面教材:前端测试用 fake backend,只验证「调用了 `sendKey('home')`」,
而真正出 bug 的命令构造层(当时是 osascript)完全没被测到 → 测试全绿但功能全坏。

### 分层

| 层 | 工具 | 测什么 | 不测什么 |
|---|---|---|---|
| Rust 纯函数 | `cargo test` | 参数构造(`build_keyevent_args`)、解析(`parse_devices`)、校验、键码映射 | 真实 adb/scrcpy 调用 |
| Rust 集成 | `cargo test` (tokio) | kill 进程阶梯、ring buffer | — |
| 前端纯逻辑 | vitest | 设备状态推导、设置去重/校验 | — |
| 前端组件 | vitest + testing-library | 渲染、交互分发、动效类、错误恢复 | 真实 IPC(用 fake backend) |

### 防回声原则

- **命令构造必须是纯函数 + 单测**:`build_keyevent_args(serial, code)` 返回 argv 数组,
  测试断言数组内容(含「不含 keystroke/osascript」这种盯防回归的反向断言)。
- fake backend 只能测「UI 是否正确调用了接口」,**测不到接口背后的真实命令**——
  所以真实命令一定要在 Rust 层有纯函数单测兜底。
- 改了一个会与系统交互的行为(按键/录制/截图),问自己:**有没有一个测试测的是真实构造的命令,而不是 mock 的返回?** 没有就补。

### 真机验证

涉及 adb/scrcpy 的改动,merge 前用真机跑一遍核心路径(`adb -s <serial> shell ...`
退出码 + 设备实际反应)。CI 跑不了真机,这步靠人。

## 8. 扩展指南(加功能前对照)

**加一个启动参数(画质类)** → 加进 `PRESETS`(types.ts)。无需动后端,`preset.args`
透传。

**加一个运行时操作(控制类)** → ① `KeyAction` 枚举加变体(types.ts + keyinject.rs 两处)
② `FLOAT_BUTTONS` 加按钮 ③ `keyinject.rs::adb_command` 加映射 ④ 若是 special(需额外状态/多步),
在 `lib.rs::send_key` 加分支 ⑤ 补 keyinject 纯函数测试 + FloatPanel 组件测试。

**加一个 IPC 命令** → 同步四处:`backend.ts` 接口 + 实现、`lib.rs` `#[tauri::command]`、
`invoke_handler!` 注册。

**加一个窗口 API 调用** → capability 加 `core:window:allow-*`。

**判断新功能是 Preset 还是 Runtime Action**(§2):问「它在 scrcpy **启动后**才有意义吗?」
是 → Runtime Action(浮窗);只影响**启动那一刻** → Preset。录屏的答案是「启动后」
(要开始/停止),所以是 Runtime Action——尽管它需要 `--record` 启动参数,停止=杀进程。

