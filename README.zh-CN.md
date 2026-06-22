<div align="center">

<img src="src-tauri/icons/128x128.png" width="72" height="72" alt="Patina icon">

# Patina Linux Fork

Patina 的 Linux 移植与本地 AI/API 集成 fork。

[English](README.md) · 简体中文

![Platform](https://img.shields.io/badge/platform-Linux%20prototype%20%7C%20Windows%20upstream-4f6f8f)
![Built with Tauri](https://img.shields.io/badge/built%20with-Tauri%20v2-4f7f8f)
![Local first](https://img.shields.io/badge/data-local--first-5f7f68)
![Status](https://img.shields.io/badge/status-fork%20prototype-b07a3a)
[![License](https://img.shields.io/badge/license-MIT-6f647a)](LICENSE)

</div>

这个 fork 用来推进 Patina 的 Linux 移植。上游 Patina 是一个本地优先的 Windows 桌面时间追踪工具；这个 fork 目前重点放在 GNOME/Linux 前台窗口识别、浏览器网页活动记录、本地 HTTP API，以及面向外部 AI/MCP 的数据接口。

Linux 版本当前是可用的开发原型，还不是稳定发行版。

## 当前 fork 重点

- GNOME Wayland 下通过 GNOME Shell 扩展和 session D-Bus 获取前台窗口。
- 保留 Linux X11 fallback 方向。
- 提供本地 HTTP API，供脚本、agent 和 MCP wrapper 调用。
- 通过浏览器扩展同步当前活动网页。
- 支持 Chromium 扩展。
- 支持 Firefox / Zen 扩展。
- 设置页提供窗口追踪、本地 API、浏览器桥接、Linux 自启动诊断。
- 可以修复 Linux `~/.config/autostart/Patina.desktop` 的错误 `Exec`。

## 当前 Linux 状态

| 模块 | 状态 | 说明 |
|---|---|---|
| GNOME Wayland 窗口追踪 | 原型可用 | 依赖 GNOME Shell 扩展提供的 `org.patina.WindowTracker`。 |
| X11 追踪 | 计划 / 部分 | 作为 fallback 方向保留，尚不是主要验证路径。 |
| KDE / wlroots Wayland | 暂不承诺 | 后续需要按桌面环境分别适配。 |
| 本地 API | 已实现 | 监听 `127.0.0.1:14840`，使用 bearer token。 |
| MCP wrapper | 原型已实现 | `npm run mcp:patina`。 |
| Chromium 网页同步 | 已实现 | `extensions/chromium`。 |
| Firefox / Zen 网页同步 | 原型已实现 | `extensions/firefox`；签名 XPI 分发目前仍是手动流程。 |
| Linux 打包 | 未完成 | 当前优先开发运行路径，安装器后续再做。 |

## Linux 快速开始

### 1. 安装依赖

```bash
npm install
```

还需要 Rust 和你当前发行版所需的 Tauri Linux 构建依赖。

### 2. 安装 GNOME Shell 扩展

```bash
npm run extension:gnome:check
npm run extension:gnome:install
gnome-extensions enable patina-window-tracker@patina
```

如果 GNOME Shell 仍缓存旧扩展，注销后重新登录。

验证 D-Bus：

```bash
gdbus call --session \
  --dest org.patina.WindowTracker \
  --object-path /org/patina/WindowTracker \
  --method org.patina.WindowTracker.GetFocusedWindow
```

### 3. 运行 Patina

```bash
npm run tauri dev
```

应用会自动启动本地 API：

```text
http://127.0.0.1:14840
```

Token 路径：

```text
${XDG_DATA_HOME:-~/.local/share}/Patina/api_token
```

## 浏览器网页同步

Patina 可以通过浏览器扩展记录当前活动网页的 URL、标题和域名。扩展不会读取网页正文、表单内容、截图、剪贴板或浏览历史库。

### Chromium / Chrome / Edge

目录：

```text
extensions/chromium
```

打开 `chrome://extensions`，开启开发者模式，加载这个未打包扩展目录。

### Firefox / Zen

目录：

```text
extensions/firefox
```

开发临时加载：

```text
about:debugging#/runtime/this-firefox
```

选择 `extensions/firefox/manifest.json`。

如果要在 Firefox/Zen 里持久安装，直接安装已签名的 `.xpi` 包即可。

如果你修改扩展源码后需要重新构建，可以先用 `extensions/firefox/package.sh` 生成未签名包，再重新签名后分发。

## 本地 API

设置环境变量：

```bash
export PATINA_API_BASE="http://127.0.0.1:14840"
export PATINA_API_TOKEN="$(cat "${XDG_DATA_HOME:-$HOME/.local/share}/Patina/api_token")"
```

示例：

```bash
curl -s "$PATINA_API_BASE/api/v1/diagnostics" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

主要接口索引：

- [docs/api-index.md](docs/api-index.md)

当前已实现 diagnostics、current activity、sessions、summary、trend、web activity、apps、tracker settings 等接口组。

## MCP Wrapper

当前 MCP wrapper 会把 MCP tool call 转成本地 API 请求：

```bash
npm run mcp:patina
```

读取这些配置：

- `PATINA_API_BASE`
- `PATINA_API_TOKEN`
- `PATINA_API_TOKEN_FILE`

当前工具包括：

- `get_diagnostics`
- `get_current_activity`
- `query_sessions`
- `get_active_session`
- `get_today_summary`
- `get_week_summary`
- `get_activity_trend`
- `query_web_activity`
- `list_apps`
- `classify_app`

## 常用检查

```bash
npm run build
npm run test:settings
npm run extension:gnome:check
npm run extension:chromium:check
npm run extension:firefox:check
node --experimental-strip-types --experimental-specifier-resolution=node tests/patinaMcpScript.test.ts
cargo check --manifest-path src-tauri/Cargo.toml --quiet
```

## 上游产品背景

Patina 是一个面向个人桌面的本地优先时间追踪工具。它会自动记录前台应用，处理 AFK、锁屏、睡眠、崩溃恢复等边界，数据保存在本地 SQLite，并提供 Dashboard、History、Data、App Mapping 等回看和管理界面。

上游稳定方向仍是 Windows 优先。这个 fork 主要探索同一套产品能否在 Linux 上成立，并为外部 AI 分析暴露足够稳定的本地结构化数据。

## 文档

- Linux 设置：[docs/linux-development-setup.md](docs/linux-development-setup.md)
- API 索引：[docs/api-index.md](docs/api-index.md)
- Linux/API 设计：[docs/linux-port-and-api-design.md](docs/linux-port-and-api-design.md)
- 产品范围：[docs/product-principles-and-scope.md](docs/product-principles-and-scope.md)
- 架构规则：[docs/architecture.md](docs/architecture.md)

## 许可证

[MIT](LICENSE)
