# Patina Linux 移植 + HTTP API 设计文档

> 状态：设计稿 + 当前实现对照（Linux 原型已部分实现）
> 目标：将 Patina 从 Windows-only 扩展为 Linux 支持，并添加 HTTP API 层用于 AI 集成
> 当前 API 索引：见 [`api-index.md`](./api-index.md)

---

## 1. 目标与范围

### 1.1 目标

1. **Linux 平台支持**：在 X11 和 GNOME Wayland 环境下实现与 Windows 版本接近的追踪能力
2. **HTTP API 层**：为 AI agent 提供标准化的数据查询和管理接口
3. **最小改动原则**：复用现有 domain/data/engine/commands 层，仅替换 platform 层

### 1.2 非目标

- 通用 Wayland 原生支持（GNOME 先通过 Shell 扩展 + D-Bus 支持；KDE/wlroots 后续分别评估）
- macOS 支持
- 移动端
- CLI 工具（HTTP API 就绪后，CLI 可作为薄壳后补）
- 前端 UI 修改（Tauri 前端天然跨平台）

### 1.3 成功标准

- Linux X11 和 GNOME Wayland 环境下能自动追踪前台窗口、检测 AFK、识别媒体播放
- HTTP API 能被 `curl`、MCP server、或任意 AI agent 调用
- 现有 Windows 功能零回归

---

## 2. 现有架构分析

### 2.1 分层结构

```
┌─────────────────────────────────────────────────┐
│  Frontend (React/TS/Vite)                       │  跨平台，不改
├─────────────────────────────────────────────────┤
│  Tauri IPC (commands/)                          │  跨平台，不改
├─────────────────────────────────────────────────┤
│  Engine (engine/tracking/, tools/, widget/)      │  跨平台，不改
├─────────────────────────────────────────────────┤
│  Domain (domain/tracking/, settings/, tools/)    │  跨平台，不改
├─────────────────────────────────────────────────┤
│  Data (data/repositories/, sqlite_pool/)         │  跨平台，不改
├─────────────────────────────────────────────────┤
│  Platform (platform/windows/)                    │  ← 需要替换
└─────────────────────────────────────────────────┘
```

### 2.2 平台依赖热图

| 模块 | Windows API 依赖 | 跨平台？ |
|------|-----------------|---------|
| `platform/windows/foreground.rs` | `GetForegroundWindow`, `GetLastInputInfo`, `CreateToolhelp32Snapshot` 等 | 否 |
| `platform/windows/audio.rs` | `IAudioSessionManager2`, COM | 否 |
| `platform/windows/media.rs` | `GlobalSystemMediaTransportControlsSessionManager` | 否 |
| `platform/windows/power.rs` | `WM_WTSSESSION_CHANGE`, `WM_POWERBROADCAST` | 否 |
| `platform/windows/icon.rs` | `ExtractIconExW`, `WM_GETICON`, GDI | 否 |
| `platform/windows/notifications.rs` | `tauri-winrt-notification`, Registry | 否 |
| `platform/windows/input.rs` | `GetAsyncKeyState` | 否 |
| `platform/windows/resource.rs` | `GetProcessMemoryInfo`, `CreateToolhelp32Snapshot` | 否 |
| `platform/windows/window_activation.rs` | `SetForegroundWindow`, `BringWindowToTop` | 否 |
| `platform/windows/handles.rs` | RAII wrappers for HANDLE/HICON/HBITMAP | 否 |
| `platform/credentials.rs` | `CredReadW`/`CredWriteW`（已有 stub） | 部分 |
| `platform/webdav.rs` | 无（纯 reqwest） | 是 |
| `platform/web_activity_bridge.rs` | 无（纯 tokio TCP） | 是 |
| `platform/app_paths.rs` | 无（Tauri API） | 是 |
| `domain/*` | 无 | 是 |
| `data/*` | 无（sqlx/sqlite） | 是 |
| `engine/*` | 间接（通过 `use platform::windows::foreground as tracker`） | 是（需解耦） |

### 2.3 关键耦合点

引擎层对平台层的引用只有一处入口：

```rust
// engine/tracking/runtime.rs:19
use crate::platform::windows::foreground as tracker;

// engine/tracking/runtime/window_polling.rs:2
use crate::platform::windows::foreground as tracker;
```

`tracker` 模块提供：
- `WindowInfo` 结构体
- `get_active_window() -> WindowInfo`
- `has_meaningful_change(previous, next) -> bool`
- `cmd_set_afk_threshold(threshold_secs: u64)`

音频/媒体信号在 `engine/tracking/runtime/support.rs` 中被调用：

```rust
use crate::platform::windows::audio;
use crate::platform::windows::media;
```

---

## 3. 平台抽象策略

### 3.1 方案选择：cfg 条件编译 vs trait 抽象

| 方案 | 优点 | 缺点 |
|------|------|------|
| `#[cfg(target_os)]` 条件编译 | 零运行时开销，原项目已有此模式（`credentials.rs`） | 模块结构需对齐 |
| `trait Platform` 动态分发 | 架构更干净 | 改动量大，需改引擎层所有调用点 |

**决策：使用 `#[cfg]` 条件编译**，理由：
1. 与项目现有风格一致（`credentials.rs` 已用此模式）
2. 引擎层只需改 `use` 语句的路径，不需要引入 trait
3. 零性能开销
4. 改动最小

### 3.2 平台模块入口

在 `platform/mod.rs` 中用 cfg 选择子模块：

```rust
// platform/mod.rs
pub mod app_paths;
pub mod credentials;
pub mod web_activity_bridge;
pub mod webdav;

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "linux")]
pub mod linux;
```

在引擎层用 cfg 选择引用：

```rust
// engine/tracking/runtime.rs
#[cfg(target_os = "windows")]
use crate::platform::windows::foreground as tracker;

#[cfg(target_os = "linux")]
use crate::platform::linux::foreground as tracker;
```

### 3.3 公共接口契约

Linux 模块必须导出与 Windows 完全相同的公共 API：

```rust
// 必须导出的类型和函数

// foreground.rs
pub struct WindowInfo {
    pub hwnd: String,           // Linux: X11 window id (十六进制) 或空
    pub root_owner_hwnd: String, // Linux: 同 hwnd 或空
    pub process_id: u32,
    pub window_class: String,   // Linux: X11 class name 或空
    pub title: String,
    pub exe_name: String,       // 进程可执行文件名
    pub process_path: String,   // Linux: /proc/pid/exe 解析路径
    pub is_afk: bool,
    pub idle_time_ms: u32,
}

pub fn get_active_window() -> WindowInfo;
pub fn has_meaningful_change(previous: Option<&WindowInfo>, next: &WindowInfo) -> bool;
pub fn cmd_set_afk_threshold(threshold_secs: u64);
pub fn get_process_path(process_id: u32) -> String;
pub fn get_process_exe_name(process_id: u32) -> String;

// audio.rs
pub fn start_signal_source();
pub async fn get_sustained_participation_signal(window: &WindowInfo) -> SustainedParticipationSignalSnapshot;

// media.rs
pub fn start_signal_source();
pub async fn get_sustained_participation_signal(window: &WindowInfo) -> SustainedParticipationSignalSnapshot;

// power.rs
pub fn start(app_handle: AppHandle);

// icon.rs
pub fn get_icon_base64(exe_path: &str) -> Option<String>;
pub fn get_window_icon_base64(hwnd_text: &str) -> Option<String>;

// notifications.rs
pub fn send(app_id: &str, app_name: &str, title: &str, body: &str, icon_path: Option<PathBuf>) -> Result<(), String>;

// input.rs
pub fn is_primary_mouse_button_down() -> bool;

// resource.rs
pub fn current_process_resource_snapshot() -> ProcessResourceSnapshot;

// window_activation.rs
pub fn restore_to_foreground<R: Runtime>(window: &WebviewWindow<R>) -> Result<(), String>;
```

---

## 4. Linux 平台层详细设计

### 4.1 目录结构

```
platform/linux/
  mod.rs                  — 模块声明
  foreground.rs           — 窗口追踪 + AFK 检测
  audio.rs                — 音频会话监控
  media.rs                — MPRIS 媒体会话
  power.rs                — 系统电源/锁屏事件
  icon.rs                 — 应用图标获取
  notifications.rs        — 桌面通知
  input.rs                — 鼠标/键盘状态
  resource.rs             — 进程资源监控
  window_activation.rs    — 窗口激活/置顶
```

### 4.2 foreground.rs — 窗口追踪（核心）

**职责**：获取当前前台窗口信息 + AFK 检测

**实现方案：X11（Phase 1）**

```
数据流：
  XGetInputFocus()        → 当前聚焦窗口 id
  XGetWindowAttributes()  → 窗口可见性检查
  ewmh: _NET_WM_PID      → 进程 id
  /proc/{pid}/comm        → 进程名
  /proc/{pid}/exe → readlink → 进程路径
  XGetWMName()            → 窗口标题
  XFetchName() / XGetClassHint() → 窗口 class
  XScreenSaverQueryInfo() → 空闲时间（AFK 检测）
```

**依赖 crate**：
- `xcb` 1.4 — X11 协议绑定
- `x11rb` — 备选（纯 Rust，更安全）

**AFK 检测**：
- Windows: `GetLastInputInfo()` 返回自上次输入的毫秒数
- Linux: `XScreenSaverQueryInfo()` 返回 `idle` 字段（毫秒），功能完全对等
- 阈值逻辑复用现有 `AFK_THRESHOLD_SECS` 原子变量

**进程信息缓存**：
- 复用现有的 `ProcessDetailsCacheEntry` + LRU + TTL 逻辑
- 唯一变化：`get_process_details()` 从读 Windows API 改为读 `/proc`

**噪音过滤**：
- `domain/tracking/process_filters.rs` 中的 `should_track()` 需要增加 Linux 系统进程过滤列表
- 现有的 Windows 过滤列表（LockApp, SearchHost, svchost, dwm 等）保留
- 新增 Linux 过滤：`gnome-shell`, `kwin_wayland`, `Xorg`, `dbus-daemon`, `systemd`, `pipewire`, `pulseaudio`, `xdg-desktop-portal` 等

**Wayland 兼容（Phase 2）**：
- 方案 A：`wlr-foreign-toplevel-management` 协议（Sway/wlroots 系）
- 方案 B：`ext-foreign-toplevel-list` 协议（新标准，尚未普及）
- 方案 C：GNOME 通过 D-Bus `org.gnome.Shell` 扩展
- Phase 1 不实现，检测到 Wayland 时打印警告并使用降级模式（仅追踪 AFK）

**当前 GNOME Wayland 决策（2026-06-21）**：
- GNOME Wayland：使用 Patina GNOME Shell 扩展导出 `org.patina.WindowTracker`，Rust 通过 session D-Bus 调用 `GetFocusedWindow()` 获取焦点窗口。
- X11：保留直接 X11 fallback，但只作为 X11 session 路径；GNOME Wayland 下扩展不可用时应降级为诊断状态，不应静默依赖 X11 fallback。
- KDE / wlroots Wayland：暂不承诺。后续按平台分别适配 KWin 或 wlroots 协议，不把 GNOME 扩展方案误写成通用 Wayland 方案。
- 标题记录：默认可控，继续沿用按应用关闭标题记录的设置；关闭后 runtime 应清空标题但仍保留应用级统计。
- 扩展诊断：Patina UI 应提示 “GNOME 扩展未安装 / 未启用 / D-Bus 不可用 / 当前只能记录 AFK 或无法记录窗口”，而不是只显示普通统计空状态。
- 已验证现状：扩展启用后 `gdbus call --session --dest org.patina.WindowTracker --object-path /org/patina/WindowTracker --method org.patina.WindowTracker.GetFocusedWindow` 可返回 GNOME Wayland 焦点窗口的标题、app id、wm class、pid 和 window id。

**GNOME 扩展仓库化与安装流程（2026-06-21）**：
- 扩展源已放入 `extensions/gnome-shell/patina-window-tracker@patina/`，不再只依赖 `~/.local/share/gnome-shell/extensions/` 中的手工原型。
- `npm run extension:gnome:check` 校验 `metadata.json`、D-Bus name、`GetFocusedWindow` 方法和 `FocusedWindowChanged` signal。
- `npm run extension:gnome:build` 生成 `dist/extensions/gnome-shell/patina-window-tracker@patina/`。
- `npm run extension:gnome:install` 将扩展复制到 `${XDG_DATA_HOME:-~/.local/share}/gnome-shell/extensions/patina-window-tracker@patina/`。
- 安装后仍需 `gnome-extensions enable patina-window-tracker@patina`；如果 GNOME Shell 已缓存旧代码，需要注销并重新登录。
- 手动验证命令：
  `gdbus call --session --dest org.patina.WindowTracker --object-path /org/patina/WindowTracker --method org.patina.WindowTracker.GetFocusedWindow`

### 4.3 audio.rs — 音频会话监控

**职责**：检测哪些进程正在播放音频（用于"持续参与识别"）

**Windows 现状**：COM `IAudioSessionManager2` 枚举活跃音频会话

**Linux 方案**：

**方案 A：PulseAudio `subscribe`（推荐）**
- PulseAudio 提供 `pa_context_subscribe()` 监听 sink input 事件
- 通过 `pa_context_get_sink_input_info_list()` 枚举当前活跃的音频流
- 每个 sink input 包含 `application.process.id`，可关联到进程
- PipeWire 的 PulseAudio 兼容层完全支持此接口

**方案 B：D-Bus + PipeWire 原生**
- PipeWire 暴露 D-Bus 接口 `org.pipewire`
- 更底层，但不兼容纯 PulseAudio 系统

**决策：方案 A（PulseAudio API）**，因为 PipeWire 的 PulseAudio 兼容层是默认启用的，覆盖 95%+ 的 Linux 桌面。

**依赖 crate**：
- `libpulse-binding` 2.x — PulseAudio 客户端绑定

**实现结构**（与 Windows 版对齐）：
```
后台 tokio task（10s 间隔 reconcile）
  → spawn_blocking 调用 PulseAudio API
  → 构建 AudioSnapshot { sessions: Vec<AudioSessionFact> }
  → 通过 AUDIO_SIGNAL_SOURCE 全局状态暴露
  → get_sustained_participation_signal() 查询
```

### 4.4 media.rs — 媒体会话监控

**职责**：检测系统级媒体播放状态（MPRIS）

**Windows 现状**：`GlobalSystemMediaTransportControlsSessionManager`

**Linux 方案：MPRIS D-Bus**
- 标准接口：`org.mpris.MediaPlayer2.Player`
- 所有主流播放器支持（Firefox, Chrome, VLC, Rhythmbox, Spotify 等）
- 通过 D-Bus `ListNames()` 发现所有 `org.mpris.MediaPlayer2.*` 实例
- 查询 `PlaybackStatus` 属性获取 Playing/Paused/Stopped
- 查询 `DesktopEntry` 或 bus name 推断进程

**依赖 crate**：
- `zbus` 4.x — D-Bus 客户端（纯 Rust，不需要 libdbus）

**实现结构**：
```
后台 tokio task（10s 间隔 reconcile）
  → zbus 连接 session bus
  → ListNames() 扫描 mpris 播放器
  → Get PlaybackStatus for each
  → 构建 SustainedParticipationSignalSnapshot
```

### 4.5 power.rs — 电源/锁屏事件

**职责**：检测锁屏、睡眠、恢复事件，触发会话封口

**Windows 现状**：隐藏消息窗口接收 `WM_WTSSESSION_CHANGE` 和 `WM_POWERBROADCAST`

**Linux 方案：systemd-logind D-Bus**
- 接口：`org.freedesktop.login1.Manager`
- 信号：
  - `PrepareForSleep(bool)` — `true` = 即将睡眠, `false` = 恢复
  - `PrepareForShutdown(bool)` — 即将关机
  - `Lock` / `Unlock` — 锁屏/解锁（通过 `org.freedesktop.login1.Session`）
- 需要获取当前 session path：`org.freedesktop.login1.Manager.GetSessionByPID()`

**依赖 crate**：
- `zbus` 4.x（与 media.rs 共用）

**事件映射**：
| Windows 事件 | Linux 信号 | Patina 状态 |
|---|---|---|
| `WTS_SESSION_LOCK` | `Lock` | `"lock"` |
| `WTS_SESSION_UNLOCK` | `Unlock` | `"unlock"` |
| `PBT_APMSUSPEND` | `PrepareForSleep(true)` | `"suspend"` |
| `PBT_APMRESUMEAUTOMATIC` | `PrepareForSleep(false)` | `"resume"` |

### 4.6 icon.rs — 应用图标获取

**职责**：根据进程路径获取应用图标（base64 PNG）

**Windows 现状**：`ExtractIconExW` + HICON 转 PNG

**Linux 方案：freedesktop 图标规范**

```
流程：
  1. 从 .desktop 文件查找 Icon= 字段
     - 搜索路径：~/.local/share/applications/, /usr/share/applications/
     - 匹配规则：根据 exe_name 查找 Exec= 包含该命令的 .desktop 文件
  2. 在图标主题中查找
     - 搜索路径：~/.local/share/icons/, /usr/share/icons/, /usr/share/pixmaps/
     - 支持 SVG 和 PNG
     - 遵循 hicolor 主题回退链
  3. 如果找到 SVG → 用 resvg 渲染为 PNG → base64
  4. 如果找到 PNG → 直接用 image crate 加载 → base64
```

**依赖 crate**：
- `freedesktop-desktop-entry` 0.5 — .desktop 文件解析
- `resvg` 0.44 — SVG 渲染（可选，如果只支持 PNG 则不需要）

**缓存**：复用现有 `IconResultCacheEntry` + LRU + TTL 逻辑

### 4.7 notifications.rs — 桌面通知

**Windows 现状**：`tauri-winrt-notification` + Registry 注册

**Linux 方案：freedesktop 通知**

```
D-Bus 调用：
  bus: session
  dest: org.freedesktop.Notifications
  path: /org/freedesktop/Notifications
  method: Notify(app_name, replaces_id, icon, summary, body, actions, hints, expire_timeout)
```

**依赖 crate**：
- `notify-rust` 4.x — 已封装好 freedesktop 通知

### 4.8 input.rs — 输入状态检测

**Windows 现状**：`GetAsyncKeyState(VK_LBUTTON)` 检测鼠标左键

**Linux 方案**：

```
方案 A：X11 XQueryPointer
  - 查询指针位置和按钮状态
  - 与 foreground.rs 共用 X11 连接

方案 B：读 /dev/input/*
  - 需要 root 或 input 组权限
  - 更底层，不推荐
```

**决策：方案 A**，复用 foreground.rs 的 XCB 连接。

### 4.9 resource.rs — 进程资源监控

**Windows 现状**：`GetProcessMemoryInfo`, `GetProcessHandleCount`, 线程快照

**Linux 方案：/proc 文件系统**

```rust
// /proc/self/status → VmRSS, Threads
// /proc/self/fd/ → 目录项计数 = handle count
// /proc/self/stat → 第 14-17 字段 = cpu time

fn current_process_resource_snapshot() -> ProcessResourceSnapshot {
    let status = fs::read_to_string("/proc/self/status");
    // 解析 VmRSS, Threads
    let fd_count = fs::read_dir("/proc/self/fd").map(|d| d.count());
    ProcessResourceSnapshot { ... }
}
```

### 4.10 window_activation.rs — 窗口激活

**Windows 现状**：`SetForegroundWindow`, `BringWindowToTop`, `SetWindowPos`

**Linux 方案：X11 EWMH**

```
_NET_ACTIVE_WINDOW  → 设置活动窗口
_NET_WM_STATE       → 设置窗口状态（最大化、置顶等）
XRaiseWindow        → 提升窗口
```

### 4.11 credentials.rs — 凭证存储

**Windows 现状**：`CredReadW`/`CredWriteW`（已有 `#[cfg(not(windows))]` 返回 stub）

**Linux 方案：libsecret（GNOME Keyring / KWallet）**

```rust
#[cfg(target_os = "linux")]
pub fn save_webdav_backup_password(username: &str, password: &str) -> Result<(), String> {
    // 通过 D-Bus org.freedesktop.secrets 存储
    // 或简单方案：用 age 加密存到 ~/.config/patina/credentials.enc
}
```

**Phase 1 简化**：使用文件系统加密存储（`age` crate），不依赖 Keyring 守护进程。

### 4.12 domain 层调整

**process_filters.rs** — 新增 Linux 进程过滤列表：

```rust
#[cfg(target_os = "linux")]
fn is_system_process(exe_name: &str) -> bool {
    matches!(exe_name,
        "gnome-shell" | "kwin_wayland" | "kwin_x11" |
        "Xorg" | "Xwayland" | "dbus-daemon" |
        "systemd" | "systemd-journal" | "systemd-logind" |
        "pipewire" | "pipewire-pulse" | "wireplumber" |
        "pulseaudio" |
        "xdg-desktop-portal" | "xdg-desktop-portal-gnome" |
        "gsd-" /* gnome-settings-daemon variants */ |
        "at-spi-bus-launcher" | "at-spi2-registryd" |
        "gcr-prompter" | "gnome-keyring-daemon"
    )
}
```

**sustained_identity.rs** — 新增 Linux 路径映射：

```rust
#[cfg(target_os = "linux")]
pub fn sustained_participation_app_identity(exe_name: &str, process_path: &str) -> SustainedParticipationAppIdentity {
    match exe_name {
        "firefox" | "firefox-esr" => SustainedParticipationAppIdentity::Firefox,
        "chrome" | "chromium" | "chromium-browser" => SustainedParticipationAppIdentity::Chrome,
        "code" | "code-oss" => /* VS Code */,
        "vlc" => SustainedParticipationAppIdentity::Vlc,
        "zoom" | "ZoomLauncher" => SustainedParticipationAppIdentity::Zoom,
        "teams" | "teams-for-linux" => SustainedParticipationAppIdentity::Teams,
        _ => /* 路径匹配 fallback */,
    }
}
```

---

## 5. HTTP API 层设计

### 5.1 设计目标

- 供 AI agent（Claude Code、自定义脚本等）查询时间数据和执行管理操作
- 监听 `127.0.0.1`，仅本地访问
- Token 鉴权（复用现有 WebActivityBridge 的模式）
- JSON 请求/响应

### 5.2 与 WebActivityBridge 的关系

现有 `platform/web_activity_bridge.rs` 已经实现了一个 localhost HTTP 服务器，但它专用于浏览器扩展的网页活动同步，路由固定。

**决策：新建独立的 API 服务**，理由：
1. WebActivityBridge 的端口和 token 由浏览器扩展配置，生命周期独立
2. API 服务需要独立的端口（避免冲突）
3. API 服务的鉴权需求不同（API token vs 浏览器扩展 token）

### 5.3 端口与鉴权

```
默认端口：14840（可配置）
鉴权：Bearer token（首次启动生成，存入 settings）
绑定：127.0.0.1（仅本地）
协议：HTTP/1.1（不搞 HTTPS，本地通信不需要）
```

### 5.4 API 端点设计

#### 5.4.1 状态与健康

```
GET /api/v1/health
Response: { "status": "ok", "version": "1.7.0", "platform": "linux" }

GET /api/v1/current
Response: WindowInfo（当前前台窗口 + AFK 状态）
{
    "exe_name": "firefox",
    "title": "GitHub - patina",
    "process_id": 12345,
    "is_afk": false,
    "idle_time_ms": 5000,
    "process_path": "/usr/bin/firefox"
}
```

#### 5.4.2 会话查询

```
GET /api/v1/sessions?from={ms}&to={ms}&app={exe_name}&limit={n}
Response: { "sessions": [...] }

GET /api/v1/sessions/active
Response: 当前活跃会话（如果有）

GET /api/v1/summary/today
Response: {
    "date": "2026-06-19",
    "total_active_ms": 28800000,
    "apps": [
        { "exe_name": "firefox", "total_ms": 12000000, "percentage": 41.7 },
        { "exe_name": "code", "total_ms": 9000000, "percentage": 31.2 }
    ],
    "categories": [
        { "name": "开发", "total_ms": 15000000 },
        { "name": "浏览", "total_ms": 12000000 }
    ]
}

GET /api/v1/summary/range?from={ms}&to={ms}
Response: 同上结构，自定义时间范围

GET /api/v1/summary/week
Response: 最近 7 天汇总

GET /api/v1/trend?period=week|month&granularity=day
Response: { "data_points": [{ "date": "...", "active_ms": ..., "top_app": "..." }] }
```

#### 5.4.3 应用管理

```
GET /api/v1/apps
Response: { "apps": [{ "exe_name": "...", "display_name": "...", "category": "...", "excluded": false }] }

POST /api/v1/apps/{exe_name}/classify
Body: { "category": "开发" }
Response: { "ok": true }

POST /api/v1/apps/{exe_name}/rename
Body: { "display_name": "VS Code" }
Response: { "ok": true }

POST /api/v1/apps/{exe_name}/exclude
Body: { "excluded": true }
Response: { "ok": true }

POST /api/v1/apps/{exe_name}/title-recording
Body: { "enabled": false }
Response: { "ok": true }
```

#### 5.4.4 设置查询

```
GET /api/v1/settings/tracker
Response: {
    "idle_timeout_secs": 180,
    "timeline_merge_gap_secs": 180,
    "tracking_paused": false
}

POST /api/v1/settings/tracker/afk-threshold
Body: { "seconds": 300 }
Response: { "ok": true }
```

#### 5.4.5 工具（Phase 2）

```
GET /api/v1/tools/reminders
POST /api/v1/tools/reminders
DELETE /api/v1/tools/reminders/{id}

GET /api/v1/tools/pomodoro
POST /api/v1/tools/pomodoro/start
POST /api/v1/tools/pomodoro/stop
```

#### 5.4.6 浏览器活动

```
GET /api/v1/web-activity?from={ms}&to={ms}
Response: { "segments": [...] }
```

### 5.5 错误响应格式

```json
{
    "error": {
        "code": "not_found",
        "message": "Session not found"
    }
}
```

HTTP 状态码：200 / 400 / 401 / 404 / 500

### 5.6 实现架构

```
engine/api/
  mod.rs              — 模块声明
  server.rs           — HTTP 服务器（tokio TCP，复用 web_activity_bridge 模式）
  router.rs           — 路由分发
  handlers/
    mod.rs
    health.rs         — /health, /current
    sessions.rs       — /sessions, /summary/*
    apps.rs           — /apps/*
    settings.rs       — /settings/*
    tools.rs          — /tools/*
    web_activity.rs   — /web-activity
  auth.rs             — Token 鉴权中间件
  types.rs            — 请求/响应类型定义
```

**与 Tauri 状态的交互**：
- API handlers 通过 `AppHandle` 访问 Tauri managed state
- 复用 `commands/` 层已有的业务逻辑
- 不直接操作数据库，通过 `data/` 层的 repository/service

**启动时机**：
- 在 `app/runtime.rs` 的 `setup()` 中，紧跟 `web_activity_bridge::start()` 之后
- 需要等 SQLite 初始化完成

### 5.7 MCP Server 集成（后续）

HTTP API 就绪后，可以加一个轻量 MCP server wrapper：

```
mcp-patina-server
  → 连接 http://127.0.0.1:14840
  → 注册 tools: query_sessions, get_summary, classify_app, ...
  → Claude Code 直接调用
```

这不在本次设计范围内，但 API 设计为此预留了基础。

---

## 6. 文件变更清单

### 6.1 新增文件

```
src-tauri/src/platform/linux/
  mod.rs
  foreground.rs
  audio.rs
  media.rs
  power.rs
  icon.rs
  notifications.rs
  input.rs
  resource.rs
  window_activation.rs

src-tauri/src/engine/api/
  mod.rs
  server.rs
  router.rs
  auth.rs
  types.rs
  handlers/
    mod.rs
    health.rs
    sessions.rs
    apps.rs
    settings.rs
    tools.rs
    web_activity.rs
```

### 6.2 修改文件

| 文件 | 改动 |
|------|------|
| `src-tauri/Cargo.toml` | 添加 Linux 依赖 + cfg 条件依赖 |
| `src-tauri/src/platform/mod.rs` | 添加 `#[cfg(target_os = "linux")] pub mod linux;` |
| `src-tauri/src/engine/tracking/runtime.rs` | `use` 语句改为 cfg 条件 |
| `src-tauri/src/engine/tracking/runtime/window_polling.rs` | 同上 |
| `src-tauri/src/engine/tracking/runtime/support.rs` | audio/media 的 use 语句改为 cfg 条件 |
| `src-tauri/src/app/runtime.rs` | startup 序列中添加 Linux 平台初始化 + API 服务启动 |
| `src-tauri/src/app/bootstrap.rs` | 添加 API 相关的 managed state |
| `src-tauri/src/domain/tracking/process_filters.rs` | 添加 Linux 进程过滤列表 |
| `src-tauri/src/domain/tracking/sustained_identity.rs` | 添加 Linux 应用身份映射 |
| `src-tauri/tauri.conf.json` | 添加 Linux bundle 配置 |

### 6.3 不修改的文件

- 整个 `domain/` 层（除 process_filters 和 sustained_identity）
- 整个 `data/` 层
- 整个 `commands/` 层
- 前端所有文件
- `platform/windows/` — 保留不动
- `platform/webdav.rs`, `platform/web_activity_bridge.rs`, `platform/app_paths.rs`

---

## 7. 依赖变更

### 7.1 Cargo.toml 新增

```toml
# HTTP API 层
axum = "0.8"
tower = "0.5"
tower-http = { version = "0.6", features = ["cors"] }

# Linux 平台
[target.'cfg(target_os = "linux")'.dependencies]
xcb = { version = "1.4", features = ["xlib_xcb", "screensaver", "randr"] }
zbus = { version = "4", default-features = false, features = ["tokio"] }
freedesktop-desktop-entry = "0.5"
notify-rust = "4"
libpulse-binding = "2"
procfs = "0.16"
```

### 7.2 条件编译的现有依赖

```toml
# Windows-only（添加 cfg）
[target.'cfg(target_os = "windows")'.dependencies]
windows = { version = "0.62.2", features = [...] }
tauri-winrt-notification = "0.7.2"
```

### 7.3 前端无需变更

---

## 8. 实施阶段

### Phase 1：Linux 平台层（X11）

**目标**：Linux X11 下能跑起来，追踪功能基本对等

| 步骤 | 内容 | 预估 |
|------|------|------|
| 1.1 | 搭 `platform/linux/` 骨架，所有文件导出相同签名的 stub | 0.5 天 |
| 1.2 | `foreground.rs` — X11 窗口查询 + /proc 进程信息 + AFK 检测 | 3 天 |
| 1.3 | `power.rs` — D-Bus login1 锁屏/睡眠事件 | 1 天 |
| 1.4 | `media.rs` — MPRIS D-Bus 媒体会话 | 1 天 |
| 1.5 | `audio.rs` — PulseAudio 音频会话 | 2 天 |
| 1.6 | `icon.rs` — freedesktop 图标查找 | 1.5 天 |
| 1.7 | 其余小文件（notifications, input, resource, window_activation） | 1 天 |
| 1.8 | `process_filters.rs` + `sustained_identity.rs` 适配 | 0.5 天 |
| 1.9 | `Cargo.toml` 依赖 + cfg 条件编译 | 0.5 天 |
| 1.10 | 集成测试 + 修复 | 2 天 |
| **合计** | | **13 天** |

### Phase 2：HTTP API 层

**目标**：AI agent 能通过 HTTP 查询数据和管理应用

| 步骤 | 内容 | 预估 |
|------|------|------|
| 2.1 | `engine/api/` 骨架 — axum 服务器 + 鉴权中间件 | 1 天 |
| 2.2 | `/health` + `/current` 端点 | 0.5 天 |
| 2.3 | `/sessions` + `/summary/*` 查询端点 | 1.5 天 |
| 2.4 | `/apps/*` 管理端点 | 1 天 |
| 2.5 | `/settings/*` 端点 | 0.5 天 |
| 2.6 | 集成到 app startup + 配置（端口、token） | 0.5 天 |
| 2.7 | 测试 + 文档 | 1 天 |
| **合计** | | **6 天** |

### Phase 3：增强（后续）

- Wayland 支持
- CLI 薄壳（`patina query --today`）
- MCP Server 包装
- WebSocket 实时推送（当前窗口变化通知）
- 凭证存储（libsecret）

### 当前实现缺口（2026-06-26）

本文最初按“未开始实现”编写；当前仓库已经有 Linux 原型和 API 原型，但仍未完成以下内容：

**已实现或基本可用**：
- `platform/linux/` 模块已存在，GNOME Wayland 焦点窗口可通过 GNOME Shell 扩展 + D-Bus 获取。
- X11 fallback、Mutter IdleMonitor AFK、`/proc` 进程路径/进程名解析、进程信息缓存已存在。
- 本地 HTTP API 服务已能监听 `127.0.0.1:14840`，并实现 `/health`、`/current`、`/sessions`、`/sessions/active`、`/summary/today`、`/summary/range`、`/summary/week`、`/trend`、`/web-activity`、`/apps`、部分 `/settings`。
- Chromium 浏览器扩展目录已存在，`platform/web_activity_bridge.rs` 仍监听浏览器活动桥接端口。
- `/api/v1/diagnostics` 已暴露 window tracking、tracker runtime probe 和 browser bridge 状态，供外部 AI/MCP 调用方判断数据可信度。
- Settings 已有 Linux/API 诊断面板，包含窗口追踪、Local API 监听/token 状态、浏览器扩展连接状态。
- Linux 音频会话 probe 已修复 threaded PulseAudio mainloop 未启动导致的超时；在 PipeWire + pipewire-pulse 环境下 live probe test 可完成。
- GNOME 扩展源、校验、build 与本地安装脚本已进入仓库。
- `npm run mcp:patina` 已提供最小 MCP wrapper，当前覆盖 diagnostics、current、active session、today/week summary 和 web activity 查询。
- 默认 CI 与 Release 已改为 Linux-only：Ubuntu runner 直接生成 AppImage、`.deb`、Linux updater manifest 和浏览器/GNOME 扩展，不再依赖 Windows job。

**部分实现但需要修正**：
- `/summary/*` 已统一使用区间重叠与边界裁剪口径，并把活跃 session 计算到响应采样时刻。
- `/summary/today` 与 `/summary/week` 已改用本地日/周边界；`/summary/range` 仍按调用方传入的毫秒范围执行，后续如暴露 timezone 应在响应中明确。
- `/sessions/active` 已返回实时 duration、app、title、exe、continuity group 和 sampled_at；后续可继续补分类、标题记录开关后的脱敏口径。
- `/trend` 已支持 `period=week|month&granularity=day`，按本地日期切分跨天 session，并把 active session 计到当前时间；后续可补 hour/week granularity、分类趋势和 timezone 字段。
- `/web-activity` 已支持按时间、域名和 limit 查询浏览器活动片段，并默认返回非隐身 `http` / `https` 页面的完整 URL；后续需要补 API 设置里的 URL 脱敏/诊断说明。
- Wayland 下 X11 fallback 曾触发 XCB panic；GNOME Wayland 路径应优先扩展 D-Bus，D-Bus 不可用时明确降级。

**尚未实现**：
- `/api/v1/tools/*`。
- MCP wrapper 的 sessions/trend/apps/settings 写侧工具。
- API token/port 的完整 UI 管理。
- KDE Wayland、wlroots Wayland 适配。
- Patina UI 中更完整的平台能力矩阵。

---

## 9. 风险与缓解

| 风险 | 影响 | 缓解 |
|------|------|------|
| GNOME Wayland 扩展不可用 | 前台窗口追踪失效或降级 | 在 UI 中显示扩展未安装/未启用/D-Bus 不可用诊断；不静默退回不可靠路径 |
| KDE/wlroots Wayland 暂无适配 | 前台窗口追踪不可承诺 | 后续按 KWin/wlroots 协议分别适配；当前文档只承诺 GNOME Wayland |
| PulseAudio 未安装 | 音频检测失效 | graceful degradation：检测到无 PulseAudio 时跳过音频信号 |
| X11 连接断开 | 追踪停止 | 现有的 watchdog + restart loop 机制会自动重启 |
| API 端口冲突 | API 不可用 | 端口可配置；启动失败时打印警告但不阻止主程序 |
| domain 层进程过滤不全 | Linux 系统进程被计入统计 | 初始列表基于 GNOME/KDE 常见进程，后续通过社区反馈补充 |
| libpulse-binding 编译问题 | 构建失败 | 备选方案：通过 D-Bus 调用 PulseAudio（纯 Rust，无需 C 库绑定） |

---

## 10. 测试策略

### 10.1 单元测试

- `platform/linux/foreground.rs`：`WindowInfo` 构建、进程名解析、缓存逻辑
- `domain/tracking/process_filters.rs`：新增 Linux 过滤条目的测试
- `engine/api/`：路由匹配、鉴权、错误处理

### 10.2 集成测试

- 启动追踪引擎 → 切换窗口 → 验证会话记录
- 锁屏/解锁 → 验证会话封口
- 播放音频 → 验证持续参与信号
- `curl` 调用 API → 验证响应

### 10.3 手动测试矩阵

| 场景 | GNOME (X11) | KDE (X11) | i3 |
|------|------------|-----------|-----|
| 前台窗口追踪 | | | |
| AFK 检测 | | | |
| 锁屏事件 | | | |
| 睡眠/恢复 | | | |
| 音频检测 | | | |
| MPRIS 媒体 | | | |
| 应用图标 | | | |
| 桌面通知 | | | |
| HTTP API | | | |
