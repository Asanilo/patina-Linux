<div align="center">

<img src="src-tauri/icons/128x128.png" width="72" height="72" alt="Patina icon">

# Patina Linux Fork

Patina 的 Linux 移植与本地 AI/API 集成 fork。

[English](README.md) · 简体中文

![Platform](https://img.shields.io/badge/platform-Linux%20x86__64-4f6f8f)
![Built with Tauri](https://img.shields.io/badge/built%20with-Tauri%20v2-4f7f8f)
![Local first](https://img.shields.io/badge/data-local--first-5f7f68)
![Status](https://img.shields.io/badge/status-Linux--first%20development-b07a3a)
[![License](https://img.shields.io/badge/license-MIT-6f647a)](LICENSE)

</div>

![Patina dashboard](.github/assets/readme.zh-CN/dashboard.png)


这个 fork 是 Patina 的 Linux-first 版本，重点放在 GNOME/Linux 前台窗口识别、浏览器网页活动记录、本地 HTTP API，以及面向外部 AI/MCP 的数据接口。Windows 平台源码暂时保留作为历史兼容实现，但不进入默认 CI、Release 或当前支持承诺。

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
## 界面预览

|  |  |
| --- | --- |
| **今天**<br>![今天页面](.github/assets/readme.zh-CN/dashboard.png)| **历史**<br>![历史页面](.github/assets/readme.zh-CN/history.png) |
| **数据**<br>![数据页面](.github/assets/readme.zh-CN/data.png) | **应用**<br>![应用页面](.github/assets/readme.zh-CN/mapping.png) |
| **设置**<br>![设置页面](.github/assets/readme.zh-CN/settings.png) | **关于**<br>![关于页面](.github/assets/readme.zh-CN/about.png) |

## 当前 Linux 状态

| 模块 | 状态 | 说明 |
|---|---|---|
| GNOME Wayland 窗口追踪 | 原型可用 | 依赖 GNOME Shell 扩展提供的 `org.patina.WindowTracker`。 |
| X11 追踪 | 计划 / 部分 | 作为 fallback 方向保留，尚不是主要验证路径。 |
| KDE / wlroots Wayland | 暂不承诺 | 后续需要按桌面环境分别适配。 |
| 本地 API | 已实现 | 监听 `127.0.0.1:14840`，使用 bearer token。 |
| MCP wrapper | 原型已实现 | `npm run mcp:patina`。 |
| Chromium 网页同步 | 已实现 | `extensions/chromium`。 |
| Firefox / Zen 网页同步 | 原型已实现 | 已签名 XPI 可直接安装。 |
| Linux 打包 | 发布链已配置 | 后续版本 tag 会生成 x86_64 AppImage、`.deb` 和合并后的 updater 清单。 |

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

## Linux 安装包

正式发布工作流会生成：

- `Patina_<version>_amd64.AppImage`
- `Patina_<version>_amd64.deb`
- `patina-gnome-shell-extension-v<version>.zip`
- `patina-firefox-extension-v<version>.xpi`

Ubuntu / Debian 用户优先安装 `.deb`。它会把 GNOME Shell 扩展文件安装到系统扩展目录，但仍需为当前用户启用扩展：

```bash
gnome-extensions enable patina-window-tracker@patina
```

如果 GNOME Shell 已缓存旧版本，注销后重新登录。

AppImage 不会修改系统目录。下载后运行：

```bash
chmod +x Patina_<version>_amd64.AppImage
./Patina_<version>_amd64.AppImage
```

GNOME Wayland 用户还需要下载扩展 zip 并安装：

```bash
gnome-extensions install --force patina-gnome-shell-extension-v<version>.zip
gnome-extensions enable patina-window-tracker@patina
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

这个 fork 的当前产品与发布方向是 Linux/GNOME 优先，并为外部 AI 分析暴露稳定的本地结构化数据。其他桌面平台只有在形成独立维护能力后才会重新进入支持范围。

## 文档

- Linux 设置：[docs/linux-development-setup.md](docs/linux-development-setup.md)
- API 索引：[docs/api-index.md](docs/api-index.md)
- Linux/API 设计：[docs/linux-port-and-api-design.md](docs/linux-port-and-api-design.md)
- 产品范围：[docs/product-principles-and-scope.md](docs/product-principles-and-scope.md)
- 架构规则：[docs/architecture.md](docs/architecture.md)

## 许可证

[MIT](LICENSE)
