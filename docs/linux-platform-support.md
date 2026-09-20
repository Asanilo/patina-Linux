# Linux 平台支持矩阵

## 1. 文档定位

本文记录 Patina 当前对 Linux 桌面环境和平台能力的实际支持承诺。

它只描述当前事实与明确降级行为，不承接实施计划。产品范围见 [`product-principles-and-scope.md`](./product-principles-and-scope.md)，开发优先级见 [`roadmap-and-prioritization.md`](./roadmap-and-prioritization.md)，运行时边界见 [`architecture.md`](./architecture.md)。

## 2. 当前支持承诺

`Asanilo/patina-Linux` 的日常产品主线是 `main`，已合入 daemon 分离；当前发行版支持与验证范围仍限 Debian 系。下表区分桌面/显示协议，不能单凭“GNOME”或生成了 AppImage 就宣称其他发行版可用。版本、CPU 架构和包格式仍须随候选证据记录；上游 Linux PR 的首批支持矩阵另行讨论。

| 环境 | 支持级别 | 前台窗口来源 | 说明 |
| --- | --- | --- | --- |
| GNOME Wayland | 主要支持 | GNOME Shell 扩展 + session D-Bus | 当前 Linux 发布与验证主路径 |
| Linux X11 | 有限支持 | X11 / EWMH fallback | 已实现，真实环境验证少于 GNOME Wayland |
| KDE Plasma Wayland | 尚未支持 | 待设计 KWin provider | 不复用 GNOME 扩展，也不静默声称可用 |
| wlroots compositor | 尚未支持 | 按 compositor 评估 | 不把 wlroots 视为单一统一桌面接口 |
| Windows / macOS | 不支持 | 无 | 不进入当前 CI、Release 或维护承诺 |

GNOME Wayland 下，采样端先检查 `org.patina.WindowTracker1`，只有新名称明确没有 D-Bus owner 时才尝试旧 `org.patina.WindowTracker`；两者都不可用时必须报告扩展未安装、未启用或 D-Bus 不可用，不能静默退回 X11。新接口存在但响应失败或不合协议时不回退旧接口。实际可用的 GNOME companion 可以补足不完整的桌面环境标签；仅有名称 owner 的能力诊断不能替代实际采样健康状态。

## 3. Linux 平台能力

| 能力 | 当前实现 | 降级行为 |
| --- | --- | --- |
| 前台窗口 | GNOME extension D-Bus；X11 fallback | 明确诊断为 unavailable 或 unsupported |
| AFK | Wayland 使用 Mutter IdleMonitor；X11 可回退 XScreenSaver | 真实 0 毫秒有效；无可靠来源返回采样失败，按最后可信样本结算，恢复后不补记未知时间 |
| 锁屏 / 睡眠 / 恢复 | systemd-logind 与桌面事件 | watcher 失败进入诊断和重启路径 |
| 音频参与信号 | PulseAudio API，兼容 pipewire-pulse | 不可用时跳过音频信号，不阻止窗口追踪 |
| 媒体参与信号 | MPRIS D-Bus | 不可用时跳过媒体信号 |
| 应用图标 | freedesktop 图标与进程信息 | 找不到时使用稳定 fallback |
| 浏览器网页活动 | Firefox / Zen 与 Chromium 扩展 | 未连接时仅保留窗口标题级数据 |
| 桌面通知 | freedesktop 通知 | 失败时记录错误，不改变 tracking 数据 |
| 悬浮窗拖动 / 吸附 | X11 使用全局坐标吸附；原生 Wayland 由 compositor 处理拖动 | Wayland 不读写 GTK 伪全局坐标，不自动吸附或覆盖已保存的左右偏好；全局按键未知时等待本地指针事件结束拖动态 |
| 自启动 | 当前已发布稳定版仍使用 XDG autostart desktop entry；main 的 daemon-backed DEB 输入包含默认禁用的 `patinad.service`，并已接入后台/客户端登录偏好拆分和首次安全交接 | Settings 继续显示并修复旧 desktop entry；候选交接与登录验收按实际版本记录，源码合入不等于稳定支持，也不开放可能启动第二 owner 的普通设置写入 |
| 本地 API | `127.0.0.1` + owner-only Bearer token | daemon 可原子换端口/轮换 Token；冲突时保留旧 listener，轮换后旧 API/SSE 凭据失效 |

音频和媒体是持续参与判断的辅助信号，不是录音能力。Patina 不采集麦克风内容或系统音频内容。

## 4. GNOME 扩展边界

随产品分发的 GNOME Shell 扩展只负责读取 Shell 已知的焦点窗口，并通过旧 `org.patina.WindowTracker` 暴露最小 D-Bus 接口。它不拥有 session 切分、分类、AFK 决策、数据库或 API。

main 的消费端另可识别版本 1 的 `org.patina.WindowTracker1.GetSnapshot`，校验消息类型/大小、版本/状态和窗口字段，再解析应用身份。正常无窗口/overview 和明确锁屏必须携带全空窗口事实；当前映射为无活动窗口，仍要求可信 idle，不生成永久 logind 锁状态。锁屏时 idle 同时失效则保守按最后可信采样停止记录。未知状态、不可用状态、无法解析的窗口身份和损坏响应均为失败，不作为正常空桌面推进成功时间戳。

此兼容属于消费端源码能力；现有 version 3 扩展继续使用旧协议，GNOME Shell 42 是其声明范围。新协议扩展发行、legacy/ESM 双入口、更多 Shell 版本和真实锁屏/overview 验收仍需独立完成，不因消费端兼容自动扩大支持承诺。

当前扩展不提供悬浮窗移动、置顶或全局指针状态接口。Wayland 原生窗口的边缘吸附尚未实现，不能用 GTK 返回的 `(0, 0)` 推断左侧位置；详见 [GTK 窗口位置限制](https://docs.gtk.org/gtk3/method.Window.get_position.html)。能力判断应使用实际显示后端，而非仅使用 `XDG_SESSION_TYPE`，以兼容 Wayland 会话内的 X11 客户端。

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

## 7. 跨发行版安装格式（待评估）

Flatpak 尚未实现，也不属于当前 beta 发布物。跨发行版分发与跨桌面追踪是两个问题：改变安装格式不会补齐 KWin 或 wlroots provider。

后续比较两种方案：宿主原生 `patinad` + Flatpak 桌面客户端，以及完整 Flatpak 应用。前者更符合现有 C/S 边界，但仍需单独安装宿主后台，不能称为单包通用安装；后者必须重新验证后台生命周期、GNOME 扩展 D-Bus、logind/MPRIS、音频参与、系统凭据、文件选择与备份、客户端认证和升级 owner。不能直接搬用 DEB 的宿主 systemd 安装/控制流程，也不以开放整个宿主文件系统或整条 session bus 绕过边界。

Flatpak 默认有文件系统、进程和 D-Bus 等沙箱限制，应优先采用 portal 与最小权限。参考 [Sandbox Permissions](https://docs.flatpak.org/en/latest/sandbox-permissions.html) 和 [Desktop Integration](https://docs.flatpak.org/en/latest/desktop-integration.html)。具体架构取舍与发行矩阵需独立验收后再决定，不替代既有 AppImage 用户的更新或退役迁移承诺。
