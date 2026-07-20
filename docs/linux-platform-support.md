# Linux 平台支持矩阵

## 1. 文档定位

本文记录 Patina 当前对 Linux 桌面环境和平台能力的实际支持承诺。

它只描述当前事实与明确降级行为，不承接实施计划。产品范围见 [`product-principles-and-scope.md`](./product-principles-and-scope.md)，开发优先级见 [`roadmap-and-prioritization.md`](./roadmap-and-prioritization.md)，运行时边界见 [`architecture.md`](./architecture.md)。

## 2. 当前支持承诺

| 环境 | 支持级别 | 前台窗口来源 | 说明 |
| --- | --- | --- | --- |
| GNOME Wayland | 主要支持 | GNOME Shell 扩展 + session D-Bus | 当前 Linux 发布与验证主路径 |
| Linux X11 | 有限支持 | X11 / EWMH fallback | 已实现，真实环境验证少于 GNOME Wayland |
| KDE Plasma Wayland | 尚未支持 | 待设计 KWin provider | 不复用 GNOME 扩展，也不静默声称可用 |
| wlroots compositor | 尚未支持 | 按 compositor 评估 | 不把 wlroots 视为单一统一桌面接口 |
| Windows / macOS | 不支持 | 无 | 不进入当前 CI、Release 或维护承诺 |

GNOME Wayland 下，如果 `org.patina.WindowTracker` 没有 D-Bus owner，Patina 必须报告扩展未安装、未启用或 D-Bus 不可用，不能静默退回不可靠的 X11 查询。

## 3. Linux 平台能力

| 能力 | 当前实现 | 降级行为 |
| --- | --- | --- |
| 前台窗口 | GNOME extension D-Bus；X11 fallback | 明确诊断为 unavailable 或 unsupported |
| AFK | Mutter IdleMonitor；XScreenSaver fallback | 无可靠来源时不把未知状态伪装成正常样本 |
| 锁屏 / 睡眠 / 恢复 | systemd-logind 与桌面事件 | watcher 失败进入诊断和重启路径 |
| 音频参与信号 | PulseAudio API，兼容 pipewire-pulse | 不可用时跳过音频信号，不阻止窗口追踪 |
| 媒体参与信号 | MPRIS D-Bus | 不可用时跳过媒体信号 |
| 应用图标 | freedesktop 图标与进程信息 | 找不到时使用稳定 fallback |
| 浏览器网页活动 | Firefox / Zen 与 Chromium 扩展 | 未连接时仅保留窗口标题级数据 |
| 桌面通知 | freedesktop 通知 | 失败时记录错误，不改变 tracking 数据 |
| 自启动 | 当前为 XDG autostart desktop entry | Settings 显示状态并可修复错误 Exec；daemon-backed beta 将迁移为 systemd user service，并把后台追踪与桌面客户端启动拆成独立偏好 |
| 本地 API | `127.0.0.1` + Bearer token | 监听失败不应伪装成可连接 |

音频和媒体是持续参与判断的辅助信号，不是录音能力。Patina 不采集麦克风内容或系统音频内容。

## 4. GNOME 扩展边界

GNOME Shell 扩展只负责读取 Shell 已知的焦点窗口，并通过 `org.patina.WindowTracker` 暴露最小 D-Bus 接口。它不拥有 session 切分、分类、AFK 决策、数据库或 API。

扩展源码位于：

```text
extensions/gnome-shell/patina-window-tracker@patina/
```

扩展身份保持 `patina-window-tracker@patina`。产品转向 Linux-only 或引入 `patinad` 不构成修改扩展 UUID 和 D-Bus 名称的理由。

## 5. 浏览器扩展边界

Firefox / Zen 与 Chromium 扩展负责上报当前活动标签页的 URL、标题和域名。URL 最终暴露范围由 Patina 的隐私设置决定。

扩展不读取网页正文、表单内容、截图、剪贴板或完整浏览历史库。浏览器桥接不可用时，tracking 主链仍应继续工作。

## 6. 后续平台顺序

平台扩张默认按以下顺序评估：

1. 完成 GNOME Wayland、X11 和 `patinad` 的稳定验证。
2. 为 KDE Plasma Wayland 设计独立 KWin provider。
3. 根据真实用户环境分别评估 Sway、Hyprland 等 wlroots compositor。

在现有支持面仍有明显正确性缺口时，不扩大平台承诺。
