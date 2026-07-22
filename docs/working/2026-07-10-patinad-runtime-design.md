# `patinad` 后台运行时设计

> 状态：Stage 0 至 Stage 2F 的 preview 能力迁移，以及 Stage 2F.1 的异常恢复与浏览器心跳数据语义已完成并验证；Stage 2F.1 transport/readiness/owner 收口、默认 owner 切换与客户端迁移待实施。
> 生命周期：本设计是当前 `patinad` 实施依据；后台接管稳定完成后移入 `docs/archive/`。

## 1. 目标

把当前由 Tauri desktop 拥有的后台追踪主链渐进迁入本地 daemon，使关闭桌面 UI 后仍能可靠记录，并让本机浏览器 UI、桌面客户端和未来 TUI / CLI 复用同一运行状态。

本设计不改变 Patina 的个人、本地优先边界。`patinad` 不是云服务，也不引入账号、团队协作或远程数据库。

## 2. 非目标

- 不在本轮删除 Windows 源码
- 不在本轮重写桌面 UI 或切换到 GPUI
- 不拆分独立仓库
- 不一次性把所有 Tauri command 改成 HTTP
- 不扩大 KDE、wlroots 或移动端支持
- 不为 MCP 提供任意文件操作能力

## 3. 当前 Stage 2F 状态

当前分支已经提供并验证：

- `patinad` 二进制入口
- 不依赖 `AppHandle` 的 SQLite pool 打开与 schema 准备
- Production / Local / Dev profile 隔离和 Tauri-free storage anchor 解析
- pending migration 与缺失自定义挂载的 fail-closed 行为
- desktop / daemon 按 profile 唯一 owner 的 `RuntimeLease`
- 按 profile 隔离、owner-only 的 API credential store
- desktop / daemon 共用的受限 HTTP 请求解析与生命周期
- 可选的最小 localhost API
- 与实际能力一致的 `/api/v1/health`、`/api/v1/openapi.json`
- listener、连接任务、SQLite pool 和 lease 的显式关闭顺序
- daemon 专项、生命周期和架构边界测试
- host-neutral `RuntimeContext`、clock 与 `RuntimeEventSink`
- 不依赖 `AppHandle` 的 API handlers 和共享 `ApiRuntimeContext`
- desktop / daemon 共用的完整只读 GET API 与按 surface 过滤的 OpenAPI
- desktop runtime snapshot provider 与 daemon 明确不可用状态
- tracking 启动自愈、电源封口和 watchdog 的共享数据/事件边界
- 真实临时 XDG daemon 进程下的只读 API、写请求拒绝和优雅退出验证
- daemon-owned 有界 `RuntimeEventHub`、进程内单调序号和 replay window
- bearer token 认证的 `/api/v1/events` SSE、`Last-Event-ID` replay、resync 信号和 keepalive
- `/api/v1/capabilities` 宿主/协议能力协商，不把未迁移 owner 误报为 ready
- event stream、listener、连接任务、SQLite pool 和 lease 的有序关闭
- 显式 `--serve-api --track` tracking preview，不改变默认 desktop owner
- daemon-owned tracking/watchdog、实时 tracker snapshot 与 session 写入
- tracking task 取消、有限退避重启，以及 SQLite/lease 之前的有序退出
- 正常退出按最后成功采样时间封口 active session
- host-neutral systemd-logind watcher，覆盖 Manager sleep/shutdown 与 Session lock/unlock/LockedHint
- daemon power task 的取消、退避重连，以及 power → tracking → SQLite 的关闭顺序
- `shutdown` 立即封口和重复 lock/suspend/shutdown 幂等语义
- 显式、可克隆、可取消的 Linux audio source，不依赖 Tauri host
- daemon 按 `audio_participation_enabled` 启停 PulseAudio/pipewire-pulse 播放流探测
- 音频 probe 故障与无音频分离，已暂停流不参与持续参与判断
- 显式、可克隆、可取消的 Linux MPRIS source，不依赖 Tauri host
- 多 MPRIS 播放器保留有界快照，当前窗口匹配优先于无关活动播放器
- 当前窗口对应播放器的 paused 状态可立即结束 media grace，而不会被其他播放器遮蔽
- 浏览器活动 HTTP transport 只绑定 loopback，并限制 header、body、请求时长与任务关闭；显式并发数量上限待 Stage 2F.1 补齐
- 浏览器 Token 校验、隐私规则、前台浏览器判断和 SQLite 写入不再依赖 `AppHandle`
- daemon tracking preview 从 profile 设置读取浏览器桥接端口和 Token；配置变更当前需要重启 daemon 才会生效
- tracking 事件会在离开浏览器、AFK 或暂停时封口网页段；异常退出按 active row 最后可信 `updated_at` 修复，不计入停机空白
- 浏览器 connected 使用 75 秒心跳宽限；desktop 与 daemon watchdog 每 15 秒检查一次，并在扩展过期时按最后成功上报时间封口
- desktop 继续通过薄 Tauri adapter 使用同一桥接核心

当前实现仍不能发布为正式后台服务，原因包括：

- API、SSE 和浏览器 bridge 已有请求限制与可等待关闭，但还没有各自明确的并发连接数量上限
- browser bridge readiness 当前在 bind 后写入，尚未跟随 listener/task 的意外退出自动失效
- `app/daemon/runtime.rs` 同时编排 tracking、power、audio、MPRIS、browser bridge 和 API，继续扩展前需要按 owner 拆分
- 浏览器端口和 Token 仅在 daemon 启动时读取，运行中修改需要重启
- daemon 尚无 systemd user service 和浏览器 UI
- Tauri desktop 尚未改为 daemon client

## 4. 目标结构

迁移期共享运行时结构：

```text
platform/linux ─┐
data/sqlite ────┼─> engine runtime ─> RuntimeEventSink
domain ─────────┘          │
                           ├─ patinad host
                           ├─ Tauri desktop host
                           └─ tests

patinad
  ├─ tracking / watchdog
  ├─ SQLite runtime write side
  ├─ power / audio / MPRIS
  ├─ browser activity bridge
  └─ local API + event stream

Tauri desktop
  ├─ window / tray / WebView
  ├─ Dashboard / History / Settings
  ├─ desktop updater
  └─ daemon client

Browser UI
  ├─ Dashboard / History / Data
  ├─ Apps / diagnostics
  └─ localhost API + event stream client

TUI / CLI
  └─ daemon client
```

## 5. 核心边界

### 5.1 `RuntimeContext`

持有运行时真正需要的数据库 pool、设置访问、clock 和状态对象。engine handler 和 tracking 主链依赖 context，不依赖 `AppHandle`。

### 5.2 `RuntimeEventSink`

提供最小事件出口。Tauri host 把事件映射为 Tauri event；daemon host 把事件写入本地 event stream；测试使用内存 sink。

该边界只表达事件，不承接业务判断或序列化所有权。

### 5.3 `RuntimeLease`

按 Production / Local / Dev profile 建立唯一后台写侧 owner。同一 profile 中，desktop embedded runtime 和 `patinad` 不能同时启动 tracking。

获取失败时必须返回现有 owner 的可诊断信息，不通过竞争端口或 SQLite lock 间接判断所有权。

### 5.4 Storage bootstrap

把 profile、XDG roots、data/WebView anchor、pending migration 和 fail-closed 校验提取为不依赖 Tauri 的启动边界。desktop 与 daemon 必须调用同一个 resolver。

自定义数据目录不可用时，不允许在默认位置创建替代数据库。

### 5.5 API runtime context

API handler 依赖 pool、runtime snapshots、settings 和平台诊断 provider。HTTP transport 只负责请求限制、鉴权、路由和响应，不拥有业务数据。

desktop 与 daemon 复用同一 transport 和 endpoint registry，OpenAPI 从实际启用的 endpoint 集合生成或校验。

Stage 2F.2 第一批已由 Axum + Tower 承接通用 HTTP API、SSE、并发预算和优雅关闭，并删除通用 API 的自写 parser、server loop 与 SSE writer。普通 API 和 SSE 分别使用 32 和 8 的 fail-fast 并发预算；API 只接受 loopback Host，有 Origin 时只允许 loopback HTTP(S) 或 `tauri://localhost`，无 Origin 的 Bearer 客户端保持可用。API 与浏览器扩展 bridge 保持独立 listener、credential 和 origin policy；浏览器 bridge 的 transport 迁移属于下一批。

### 5.6 Browser UI client

`patinad` 在 loopback 上提供静态浏览器 UI、HTTP API 和 event stream。第一版复用现有 React feature 与 read model，通过 browser runtime gateway 替换 Tauri IPC 和直接 SQLite 入口。

浏览器 UI 不直接打开数据库，也不获得 tray、任意文件选择、安装更新或窗口激活能力。浏览器使用 same-origin、HttpOnly、SameSite session，不获得长期 API Token；MCP、CLI 和 Agent 继续使用 owner-only Bearer Token。写侧能力开放前必须验证 CSRF、origin、loopback Host 和日志泄漏边界。

Tauri 当前继续作为桌面客户端。未来如果实测证明 GPUI 更适合 Linux，替换范围只限桌面客户端，不改变 daemon、浏览器 UI、TUI、MCP 或数据协议。

## 6. 分阶段实施

### 阶段 0：修正骨架

- 支持 Production / Local / Dev profile 隔离
- 复用 storage anchor 与 fail-closed 规则
- 建立 `RuntimeLease`
- 统一 HTTP transport
- 修正 OpenAPI 与路由能力声明
- 保持 tracking 由 desktop 拥有

验收：开发 daemon 不触碰生产库；自定义目录正确解析；第二个 owner 明确拒绝启动；daemon 不写 session。

### 阶段 1：共享运行时边界

状态：已完成。

- 引入最小 `RuntimeContext` 与 `RuntimeEventSink`
- 让 API handlers 摆脱 `AppHandle`
- 让 tracking 和 watchdog 的数据访问依赖共享 context
- desktop 继续作为默认 runtime host
- daemon 提供完整只读 API

验收：桌面行为无回归；daemon 只读 API 与现有 API 契约一致；两种 host 使用同一 handler。

### 阶段 2：daemon 接管后台

状态：Stage 2A 至 Stage 2F 的 preview 能力迁移、Stage 2F.1 数据语义和 Stage 2F.2 通用 API/SSE transport 已完成；浏览器 bridge transport、owner 收口、默认 owner 切换与客户端化待实施。

- 已完成：有界事件中心、受认证 SSE、replay/resync、能力协商和干净关闭
- 已完成：显式模式下 daemon 接管 tracking/watchdog、实时快照、session 写入和退出封口
- 已完成：共享 logind power source 与 daemon lock/suspend/resume/shutdown owner
- 已完成：daemon 接管 Linux audio source，按设置启停并在退出时取消
- 已完成：daemon 接管 Linux MPRIS source，多播放器按当前窗口优先解析并在退出时取消
- 已完成：daemon 接管 browser activity bridge，共用鉴权、隐私、记录、事件和受限请求生命周期
- 已完成：通用 API/SSE 使用 Axum + Tower，具有独立并发预算、loopback Host/origin 边界、task readiness 和有界关闭
- 待实施：浏览器 bridge transport/readiness 与 daemon owner 拆分
- 待实施：运行中设置写侧与完整本地 API owner
- 待实施：desktop 通过 daemon client 和 event stream 获取状态
- 待实施：desktop 不再启动第二套 tracker

验收：关闭 UI 后继续记录；重开 UI 恢复当前状态；AFK、锁屏、睡眠、恢复和异常封口正确；统计不倒退、不重复。

### 阶段 2F.1：后台稳定化

该阶段不增加用户功能，先把 preview 能力收敛为可长期运行的服务边界：

- 已完成 data owner：网页 active row 按 `updated_at` 恢复并受启动时间上限保护，不把停机空白计入 duration
- 已完成 web activity engine：30 秒扩展心跳使用 75 秒 connected 宽限；15 秒 watchdog 在过期后按最后成功上报时间封口，并在 data owner 中防止并发新上报被旧检查误封
- 已完成 verification first：跨夜崩溃恢复、心跳抖动、扩展消失和数据库观测边界已有测试，再进入 transport 与 owner 结构修改
- 已完成 API transport：Axum + Tower 替换通用 API 的自写 HTTP parser、server loop 和 SSE transport；普通 API 和 SSE 分别使用 32/8 的 fail-fast 并发预算
- 待完成 platform transport：browser bridge 迁移到成熟 transport 并设置独立并发上限
- 已完成 API boundary：API 使用严格 origin/CORS 与 loopback Host 校验；无 Origin 的 Bearer 客户端保持兼容
- 待完成 browser boundary：浏览器扩展保留独立 listener 和 Token，不复用通用 API 的 origin policy
- 部分完成 daemon health：通用 API listener/task readiness 已联动，意外退出会使 desktop 诊断降级或触发 daemon 受控停机；browser bridge task readiness 仍待迁移
- daemon ownership：把 tracking、power、audio、media、web activity 和 transport 生命周期移入对应 owner 模块，`app/daemon/runtime.rs` 只保留编排和关闭顺序
- workspace boundary：首个 daemon-backed 里程碑保持当前 Rust package，不把 Cargo workspace 重排混入 owner 迁移
- verification complete：继续覆盖连接饱和、listener 意外退出和有序 shutdown

验收：daemon 停机不增长 session 或网页活动；短暂心跳抖动不误报断开；扩展消失后网页段不会无限增长；连接压力不会产生无界任务；readiness 与实际服务状态一致。

### 阶段 3：Linux 服务化与桌面客户端切换

- 补齐 Patina Desktop 当前操作所需的 daemon 写侧 API、版本协商和受控 service restart
- Tauri 改为 daemon desktop client，并保留 tray、通知、文件选择和 updater
- 默认切换后 desktop 不启动或自动回退 embedded tracker；daemon 不可用时明确暂停、诊断和重启
- 一个 `patina` 产品包同时交付 Patina Desktop、`patinad` 和 systemd user unit
- DEB 不在 `postinst` 全局 enable；首次桌面启动在用户会话中迁移并启用服务
- 将“后台追踪随登录启动”与“桌面客户端随登录打开”拆成独立设置，启动时最小化只属于桌面客户端
- 首个 daemon-backed DEB 使用 beta 版本验证且只发布 DEB；AppImage 在解决 daemon 版本化解包与原子更新前不进入该发布
- embedded runtime 至少跨一个稳定版本保留为显式开发回滚路径

验收：登录后 daemon 可靠启动；关闭或退出 Tauri 后继续记录；重开 UI 恢复当前状态；服务崩溃由 systemd 恢复且不产生第二 owner；升级与卸载不误删用户数据。

### 阶段 4：独立仓库身份与稳定发布门槛

- daemon-backed beta 验收后，完整 monorepo 脱离 Windows 上游 fork network
- 保留 Git 历史、MIT 许可和 attribution，不拆分独立 `patinad` 仓库
- daemon-backed 稳定版前，单独验证 AppImage 的版本化 daemon extraction 与原子更新，或完成不破坏既有 updater 的退役迁移

验收：新仓库的 Actions、Release、Secrets 和 updater endpoint 可验证；现有 DEB 与 AppImage 用户都有明确且不会循环更新的迁移路径。

### 阶段 5：只读浏览器 UI

- `patinad` 提供 loopback browser UI、JSON API 和 event stream
- 第一版只覆盖 Dashboard、History、当前会话和诊断
- 复用现有 React feature，通过 transport-neutral browser gateway 获取数据
- 浏览器使用 same-origin HttpOnly session，长期 Bearer Token 不进入 JavaScript、URL 或浏览器存储

验收：浏览器 UI 不依赖 Tauri 即可回看数据；Tauri 与浏览器显示同一运行状态；恶意外部 Origin 无法读取 API；长期 Token 不进入浏览器历史、存储或普通日志。

### 阶段 6：受控写侧与新客户端

- 浏览器只读路径稳定后，再通过 CSRF 防护和操作确认逐步开放写侧
- MCP、CLI 和 Agent 保留 Bearer Token 认证，与浏览器 UI 共用业务契约但不共用凭据模型
- TUI 与可选 CLI

### 阶段 7：平台与客户端实现扩展

- KDE KWin provider
- 按 compositor 评估 wlroots provider
- 根据实测内存、启动、桌面集成和维护收益再独立评估 Tauri / GPUI
- 只有实测构建、二进制、资源或独立打包收益成立时才拆 Cargo workspace
- AppImage 只有在固定 service owner、版本化 daemon extraction 和 updater 原子切换得到独立验证后才恢复 daemon-backed 发布

## 7. 错误与安全策略

- storage anchor 损坏、挂载缺失或 schema 初始化失败时 fail-closed
- API 只监听 loopback；Bearer Token 文件保持 owner-only 权限并只供 MCP、CLI 和 Agent 使用
- browser UI 使用 same-origin HttpOnly session；API 拒绝任意外部 Origin，浏览器扩展使用独立 bridge credential
- daemon 正常停止前封口 active session；异常退出由下次启动检查 active row，但只能按最后可信观测时间封口，不能用下次启动时间填补停机空白
- 默认 owner 切换后，desktop 不自动启动 embedded tracker；服务故障必须可诊断并受控恢复
- desktop 与 daemon 版本不兼容时显示诊断，不静默使用不完整接口
- 迁移、清理、恢复和备份继续由 Rust owner 执行
- API、MCP、TUI 和 CLI 不获得任意路径删除或任意 SQL 能力
- 数据目录继续使用现有 `Patina` profile，不创建 `Patina Linux` 或 `patina_linux` 数据树

## 8. Windows 冻结与删除

`patinad` 稳定前：

- Windows 源码保留但冻结
- 不跟踪 Windows 上游功能
- 不新增 Windows CI、测试、发布或适配
- 不让 Windows 接口继续决定共享 runtime 的形状

`patinad` 稳定后：

- 以独立版本和独立执行计划删除 Windows cfg、依赖、源码和文档
- 先证明 Linux schema upgrade、release、updater 和构建不依赖被删路径
- 不把删除工作混入 tracking 正确性或数据迁移修复

## 9. 验证门槛

每一阶段至少覆盖：

- Production / Local / Dev 路径隔离
- 默认与自定义数据目录
- 缺失挂载、损坏锚点和数据库不可用
- 单实例、owner 竞争和端口冲突
- active session 的启动、切换、AFK、锁屏、睡眠、恢复和异常封口
- daemon 重启后的持续时间、重复记录和自愈
- UI 退出后继续记录及重新连接
- browser UI 与 Tauri desktop 的同源数据、event reconnect 和能力降级
- browser session、origin、CSRF 与长期 token 不落 URL
- API auth、请求限制、schema 与实际路由一致性
- daemon-backed `.deb` 与 systemd user service 的安装、升级、卸载和数据保留
- AppImage 的版本化 daemon extraction、固定 service owner 与 updater 原子切换独立验证
- `npm run check:full` 与 Linux release contract

阶段 0 和阶段 1 应优先使用 TDD 覆盖纯 context、path resolver、lease 和 endpoint registry，再进行真实 GNOME 环境手动验证。

## 10. 文档同步规则

- 能力承诺变化更新 `linux-platform-support.md`
- 实施顺序变化更新 `roadmap-and-prioritization.md`
- owner 或通道变化更新 `architecture.md`
- endpoint 行为变化同步更新 `api-index.md`、OpenAPI 和 `mcp-wrapper.md`
- 本设计完成使命后移入 `docs/archive/`，不长期保留为第二份架构母文档
