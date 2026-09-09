# `patinad` 后台运行时设计

> 状态：Stage 0 至 Stage 2H.2、Stage 2H.3a systemd 诊断、Stage 2H.3b typed daemon client、Stage 2H.3c 写侧 owner 收口，以及 Stage 2H.3d.1/2 和 2H.3d.3a-d 默认 owner 交接均已完成自动验证；Stage 2H.3d.4a-e 已补齐交接诊断、显式重试、登录偏好应用、安全回滚后端和 Quiet Pro 设置控件，当前进入 daemon-backed DEB 实机验证。
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

## 3. 当前 Stage 2H.3 状态

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
- 浏览器活动 HTTP transport 使用 Axum，只绑定 loopback，具有 64 KiB body、5 秒 handler、8 请求并发和 5 秒关闭预算
- 浏览器 bridge CORS 只回显 `moz-extension://` 或 `chrome-extension://` Origin，不再使用 `Access-Control-Allow-Origin: *`；无 Origin 的本机诊断请求仍可用
- 浏览器 Token 校验、隐私规则、前台浏览器判断和 SQLite 写入不再依赖 `AppHandle`
- daemon tracking preview 从 profile 设置读取浏览器桥接和 local API 端口/Token；audio、browser bridge 和 local API 配置可由对应 runtime owner 在线应用
- tracking 事件会在离开浏览器、AFK 或暂停时封口网页段；异常退出按 active row 最后可信 `updated_at` 修复，不计入停机空白
- 浏览器 connected 使用 75 秒心跳宽限；desktop 与 daemon watchdog 每 15 秒检查一次，并在扩展过期时按最后成功上报时间封口
- desktop 继续通过薄 Tauri adapter 使用同一桥接核心
- daemon tracking、power、audio、media 与 web activity 的任务状态、重试、取消和退出封口已拆入对应 `app/daemon/runtime/*` owner 模块；聚合 `runtime.rs` 只保留依赖装配和有序关闭

当前实现仍不能发布为正式后台服务，原因包括：

- API listener 端口/Token 已迁移到 daemon owner；systemd service 和可验证 restart ticket 已完成 preview
- desktop 可通过用户会话 D-Bus 查询 `patinad.service` 的安装、启用和运行状态；设置诊断可识别 unit 缺失、systemd 不可用和提前启用造成的 owner 冲突
- desktop Rust host 已有仅连接 `127.0.0.1`、不跟随重定向、带响应大小与请求超时限制的 typed `patinad` client；Bearer Token 不进入 Debug 或前端 JavaScript
- client 会先验证 `runtime_host=daemon`、协议版本范围、tracking ownership 和 event stream capability；可连接的 desktop API、错误 Token 与不兼容协议不会被误判为 daemon ready
- Rust host 已能组合 `/current`、`/sessions/active` 和 `/events` 为只读 runtime snapshot；`/current.sampled_at_ms` 用于过滤早于快照的 replay 事件
- adapter 先建立 SSE 再读取快照，首次从 sequence `0` 使用有界 replay；断线携带最后确认 cursor 重连，`resync-required` 会清除 cursor 并完整重读
- JSON、SSE、认证失败、错误 runtime host、replay gap、窗口切换刷新和 watch shutdown 已使用真实 Axum loopback server 验证
- DEB 尚未自动启用 systemd user service，desktop 也尚未切换为 daemon client
- daemon 尚无浏览器 UI
- Tauri desktop 默认启动尚未切换为 daemon client
- 默认启动仍使用 embedded owner；只有显式传入 `--daemon-client-preview` 才进入 daemon client 模式
- preview 模式不获取 desktop runtime lease，不执行启动存储迁移，不启动 embedded tracker、watchdog、API、browser bridge、Tools、audio/media/power 或 remote status owner
- preview 模式只打开已存在且具有当前 schema 的 SQLite 数据库，保留窗口、tray、desktop behavior 和 updater
- daemon `/current` 已通过 protocol 2 返回完整 `runtime_snapshot`；客户端把它镜像到原有 Tauri tracking state，并转发现有 `active-window-changed` / `tracking-data-changed` 事件
- daemon 连接断开时清除 live snapshot，不把旧窗口继续显示为当前状态；daemon 本身不随 desktop 退出
- Token 不存在或 daemon 暂不可达时 preview desktop 仍可打开，并在 client runtime state 中保留明确错误
- 真实 GNOME 会话验收确认 desktop 与 daemon 可使用同一 production profile 共存，runtime lease 始终属于 daemon，desktop 退出后 daemon API 和 tracking snapshot 继续更新
- daemon client 恢复路径已改为 owner-only 归档暂存、持久预约、systemd 受控重启和启动维护恢复；新实例在后台任务启动前提交恢复事务，Desktop 跨重启按 request ID 查询终态

### 3.1 当前收敛批次

本批次解决 Linux 稳定产品线与 daemon 架构线继续分叉的问题，不改变 `main` 的已发布行为，也不把尚未完成的 daemon 反向合入稳定分支。

实施顺序：

1. 将 Linux `main` 自 patinad 分支点之后的已验证提交单向合入本分支，保留活动详情、应用/分类/网页趋势、安全活动导入与本地定时备份。
2. 对合入能力逐项决定 owner：只读查询可继续复用 transport-neutral read model；导入、备份调度、恢复和任何 SQLite mutation 必须进入 daemon 写侧清单，不能因兼容 embedded 模式而在 daemon client 模式直接打开写连接。
3. 对照上游 `1.9.5` 的行为修复审查暂停、锁屏、休眠、采样失败、网页区间去重和备份恢复边界。已有 daemon 等价保护时补回归测试，不复制 Windows runtime；确有缺口时在 tracking、web activity 或 backup owner 内修复。
4. 完成 backup/restore 与 remote backup 的 owner 收口。受控恢复、remote upload、列表和下载恢复衔接均已实现；远端归档不经过 Desktop 路径，直接复用 owner-only 暂存、重启前预约和 daemon 启动维护模式。
5. 完成首次启动迁移、服务启停、默认 daemon owner 和双 owner 防护，再进入 daemon-backed DEB beta。

本批次验收：

- patinad 分支包含 Linux `main` 的稳定功能，版本与 changelog 状态不倒退
- daemon client 模式不存在新增的直接 SQLite 写路径或第二个后台调度 owner
- 上游正确性修复已逐项记录为“已有等价保护、已移植或明确不适用”
- frontend、真实浏览器 smoke、Rust、Clippy、bundle 与架构边界门禁全部通过
- 合流结果只推送 patinad 分支，不改变当前 Linux Release

### 3.2 本轮正确性审计

| 行为契约 | 当前结论 | 后续动作 |
| --- | --- | --- |
| 启动恢复边界 | 已移植 | active session 只使用持久化的最后成功窗口采样封口；缺失、越界或未来采样不会把停机时间计入使用时长。 |
| 采样失败后的当前会话恢复 | 已移植 | 同一应用刷新 metadata，错误残留的 active app 会按当前成功采样重新切分。 |
| watchdog 与新会话竞态 | 已移植 | watchdog 只封口不晚于其观测边界启动的 active session，不能用旧采样关闭新会话。 |
| browser bridge 端口临时占用 | 已移植 | Axum listener 保留旧配置的原子换端口语义，并使用有界退避自动恢复。 |
| Data 网页趋势重叠区间 | 已移植 | 按浏览器来源和规范域名求区间并集，不重复计算重叠心跳或重复数据，也不填补真实空白。 |
| lock / suspend 与 in-flight probe | 锁屏已通过实机验收，休眠待验收 | Desktop 与 daemon 共用 lifecycle generation、pending stop 和 transition gate；旧窗口探测不能在 lock/suspend 后恢复 active 状态。`1.9.0-beta.3` 的真实 GNOME 锁屏验收确认活动在锁定后封口、解锁后重新开始，约 168.8 秒锁屏区间未计入活动，daemon PID、restart count 和唯一 lease owner 均未变化。休眠/恢复仍需实机验证。 |
| 暂停与 in-flight probe | 已移植 | `tracking_paused=true` 与 active session 封口在同一 SQLite 事务提交，托盘、Desktop 设置与 daemon API 通过 transition gate 更新 lifecycle generation，旧采样不能在暂停后续写。 |
| 网页活动与原生浏览器 session 绑定 | 已移植 | 网页写入必须匹配当前活动的同名浏览器 session，并持久化 relation；原生 session 结束时 SQLite trigger 在同一事务内截断网页段。备份格式向后兼容保存 relation，Replace/Merge 都使用恢复后的 session ID 重建关系。 |
| restore 的 active timing 边界 | 已移植 | daemon maintenance restore 把 active session、title sample 和网页段封口到备份导出时及各自最后可信观测的上限，不把停机时间补入活动。 |

### 3.3 合入功能的写侧边界

- 活动详情、应用/分类/网页趋势和导入数据聚合属于只读能力，可以继续复用 transport-neutral read model。
- 历史清理由 Rust data owner 统一覆盖原生 session、导入事实和网页活动；标题清理覆盖原生与导入标题；按应用删除只按 executable 清理原生和导入事实，网页历史继续使用独立的域名删除语义。daemon API 已覆盖这三类维护操作。
- 活动导入提交、批次列表和批次删除已由 daemon owner 接管：Desktop 只把预览后未变化的 CSV 写入 profile 控制目录中的 `0700` 暂存目录和 `0600` 随机票据文件，API 只传票据、文件名与预览指纹；daemon 一次性消费文件并重新检查 128 MiB 上限、SHA-256 和 CSV 内容。API 不接受任意本机路径或大文件正文，该入口不作为 MCP 通用文件读取工具。
- 定时备份已由 daemon owner 接管：调度循环只随 tracking owner 启动，配置和运行状态通过 `/api/v1/backups/schedule` 读写，变更通过 SSE 通知 Desktop 重读；embedded owner 仅保留为兼容路径，同一 profile 不得同时运行两套调度器。
- 按应用删除已由 daemon data owner 接管：请求必须显式确认并限定 1 至 512 个 executable，可选时间范围必须同时提供完整半开区间；原生和导入事实、批次计数在同一事务更新，Desktop 只接收删除计数和刷新事件。
- 备份恢复已由 daemon owner 接管：Desktop 只预览和创建 owner-only 随机暂存票据，daemon 验证后预约 systemd restart，新实例在启动后台任务前执行单事务恢复；daemon client 模式不回退为 Desktop 直接写库。
- remote backup 的 URL、用户名、远端目录和最近完成时间已通过 daemon app-settings owner 写入；密码按 profile 存入系统凭据服务，不进入 SQLite、HTTP 响应、日志、OpenAPI 示例或 MCP 输出。
- 显式上传、远端列表与恢复下载已由 daemon 串行执行。Desktop daemon-client 模式只发送非密钥配置、索引 ID、策略与显式确认；daemon 校验 index 派生路径，有界下载实际归档，并直接复用受控启动恢复状态机，不返回本机路径。

### 3.4 Stage 2H.3c.6 受控恢复完成状态

受控恢复已按四个可独立验证的小批次完成，避免一次同时修改备份格式、文件边界、daemon 启动顺序和 Desktop 交互：

1. **备份关系完整性**：在现有 `web_activity_segments` 备份条目中加入向后兼容的 `native_session_id`，Replace/Merge 使用恢复后的 session ID 映射重建 `web_activity_native_sessions`，并覆盖旧备份无该字段的兼容测试。
2. **owner-only 暂存与预约**：复用活动导入的随机 ticket 思路，但使用独立恢复目录和持久 reservation。Desktop 只暂存已预览且指纹一致的归档；daemon API 只接受 ticket、SHA-256、大小、Replace/Merge、`confirmed: true`，并在请求 systemd restart 前再次验证归档。
3. **启动维护恢复**：新 daemon 在 SQLite migrations 之后、API credential/settings 加载和所有后台 task 之前执行 reservation。恢复事务先把归档中的 active timing 封口到备份产生时的可信上限，再恢复 sessions、title samples、网页关系、普通 settings、Tools 和导入数据；local API、browser bridge、remote status 与 WebDAV 目标等主机集成设置保留当前值且不从归档补入。成功/失败都持久化可查询终态，失败不覆盖为成功也不循环重启。
4. **Desktop typed client 与重连状态**：恢复命令在 daemon client 模式下创建预约并进入重连等待，按 restore request ID 查询 completed/failed；embedded 模式暂时保留现有兼容实现。文件选择和预览仍留在 Desktop，不把任意路径、归档正文或 restore 能力暴露给 browser UI、MCP、CLI/Agent。

安全与删除约束：暂存根目录必须是当前 profile control root 下的真实 `0700` 目录，文件必须是新建 `0600` 普通文件；拒绝 symlink、hard-link 替换、超限文件、内容/指纹变化和跨 profile ticket。成功后只删除 reservation 精确绑定的暂存文件；失败文件保持 owner-only 供显式重试或取消，不做模糊路径清理。任何阶段失败都必须保持原 SQLite 数据可继续启动。

恢复事务同时写入以 request ID 和 archive SHA-256 约束的 durable receipt。若进程在数据库提交后、reservation 标记完成前退出，新实例只补记 completed 和清理精确暂存文件，不重复执行 Replace/Merge。失败 reservation 不自动重试；显式取消仅允许 failed 状态，并保留失败原因供诊断。

### 3.5 当前批次：remote backup owner

1. **已完成 owner 审计**：已枚举 WebDAV secret、测试连接、上传、下载、列表和 remote-status 路径；remote-status 是独立兼容能力，不与 WebDAV backup 混为同一 owner。
2. **已完成 fail-closed 与设置收口**：daemon-client 模式下 WebDAV 非密钥设置不再直接写 SQLite；普通 app-settings 白名单只接受 URL、用户名、远端目录和完成时间，拒绝密码键。
3. **已完成凭据与上传 owner**：Linux 密码按 Production/Local/Dev profile 存入 Secret Service；Windows 冻结兼容路径保留 Credential Manager。`POST /api/v1/backups/remote/upload` 只接受非密钥配置和 `confirmed: true`，daemon 从自身 pool 生成 snapshot、使用 `0700` 临时目录与 `0600` 文件、复核归档并串行上传，最后删除精确临时文件。
4. **已实现列表与有界下载**：远端 index 由 daemon 以 1 MiB 上限读取，逐项验证产品、版本、ID、重复项、大小和 ID 派生路径；归档下载使用 `create_new`、`0600` 和 512 MiB 上限，超限或失败只删除本次临时文件。
5. **已完成并验证恢复衔接**：用户先根据索引元数据确认；daemon 下载并复核实际归档及其索引元数据后写入 2H.3c.6 的 owner-only staging，直接预约同一 systemd 启动恢复。调度失败时先检查票据是否已被 reservation 持有，无法证明安全时保留文件。前端/集成门禁、516 项 Rust 测试和 Clippy 已通过；跨 systemd 重启的真实远端服务验收并入 Stage 2H.3d 的 DEB 实机清单。

### 3.6 下一阶段：Stage 2H.3d 默认 owner 切换

Stage 2H.3d 不做一次性切换，按下面五个可回滚批次推进：

1. **2H.3d.1 systemd 控制基础（已实现，固定 unit 安装、启动与崩溃恢复已通过实机验收）**：`platform/linux` 已补齐固定 `patinad.service` 的 enable/disable/start/stop、8 秒超时、幂等短路与操作后复核；`app` 层在 Production embedded 启动前会停止提前运行的 packaged daemon，Dev/Local 不受影响。当前不开放通用 unit 名称、shell 命令、HTTP、MCP 或 UI 开关；`1.9.0-beta.1` 已确认 packaged unit 能由首次交接启用并启动，且一次受控 `SIGKILL` 后由 systemd 以新 PID 自动恢复。设置页启停与登录偏好 mutation 仍归入 2H.3d.5 的剩余实机验收。
2. **2H.3d.2 登录偏好拆分（数据语义已实现）**：已新增 host-owned `background_tracking_at_login`，并保留 `launch_at_login` 作为“桌面客户端随登录打开”；旧数据库首次打开时，新键只在缺失时继承旧值，此后不再被旧键覆盖，新安装保持现有默认行为。`start_minimized` 仍只依赖桌面客户端偏好；备份 Replace/Merge 保留当前机器的后台登录偏好。普通 UI patch、HTTP 和 MCP 仍不能直接写入后台服务偏好；首次 owner 交接已按该值应用固定 unit，日常设置与失败对账留给 2H.3d.4 专用入口。
3. **2H.3d.3 两阶段 owner 交接（代码、中断自动化与 DEB 正常路径实机验收已完成）**：第一进程只写入 owner-only cutover reservation、启用 unit 并安排受控重启，不在 embedded tracker 存活时启动 daemon；新 Desktop 进程读 reservation 后进入 daemon-client 模式，启动并协商 daemon，成功后才提交完成状态。`1.9.0-beta.1` 已确认 reservation 提交为 `completed`、Production lease 始终属于 daemon，Desktop 关闭和重开均未抢占追踪或 API。daemon 不可用或版本不兼容时显示暂停与修复诊断，不自动回退 embedded；这些失败路径仍需设置页实机重试/回滚验收。
   - **2H.3d.3a reservation 基础（已实现）**：`app/runtime_owner_cutover` 已提供 `prepared → activating → completed/failed` 持久状态机、请求 ID 约束、profile 校验、32 KiB 读取上限、owner-only `0600` 原子文件和幂等转换。文件缺失时允许 embedded；除后续显式完成的 `rolled-back` 外，任何 reservation 都选择 daemon-client 或 fail-closed 方向，failed 状态不隐式重试，损坏、不可信或 profile 错配文件直接 fail closed。本批不启用或启动 unit。
   - **2H.3d.3b embedded 准备与重启（已实现，DEB 正常路径已通过实机验收）**：仅 Production 且 user manager、固定 unit 和状态检查可用时触发；先持久化 reservation，再按后台登录偏好 enable/disable 固定 unit 并校准独立 Desktop autostart，随后请求 Tauri 受控重启且不启动 embedded runtime。Dev/Local、unit 缺失和 systemd 不可用时继续旧 embedded 路径；写入 reservation 后的失败会持久化为 failed。
   - **2H.3d.3c daemon-client 激活与确认（已实现，DEB 正常路径已通过实机验收）**：新进程由 reservation 自动选择 managed client，先把状态推进到 activating，并等待旧 Desktop `RuntimeLease` 释放后才启动固定 unit；client 在 15 秒内轮询 capability，只有 runtime host、协议、tracking owner 与 `tracking.ready` 全部成立才标记 completed。实机确认 Desktop 与 daemon 同时存在时端口、lease 和 capability 仍由 daemon 持有，Desktop 退出后记录继续增长。永久协商错误立即失败，暂时不可达可重试；failed、损坏和不可信 reservation 均不回退 embedded。显式 preview 不参与该状态机。
   - **2H.3d.3d 中断恢复自动化（已完成，service 崩溃恢复已通过实机验收）**：状态机测试覆盖每个持久化边界的重启 owner 决策、重复启动、错误 request ID、service failed、API 未就绪、版本不兼容和旧 Desktop 尚未释放 lease；`1.9.0-beta.1` 实机向固定 service 注入一次 `SIGKILL` 后，systemd restart count 增加、PID 更新、daemon lease/API/tracking 恢复且 SQLite `quick_check` 保持 `ok`。
4. **2H.3d.4 设置与回滚入口（已实现，待 DEB 实机验收）**：在 Quiet Pro Settings 中提供后台服务状态、启停和显式回滚。停用 daemon 前必须先封口并停止服务，确认 RuntimeLease 已释放后才能预约下一次 embedded 启动；不允许两个 owner 同时运行，也不把服务管理暴露给浏览器 UI、MCP 或 Agent。
   - **2H.3d.4a 交接诊断（已实现）**：Tauri 专属诊断同时返回固定 unit 与 owner cutover 状态，区分未请求、准备、激活、完成、失败及 reservation 损坏；Settings 对接管中、接管失败、managed 正常和 managed 服务停止使用不同状态与提示，并展示有界失败原因，不暴露 Token。
   - **2H.3d.4b 显式重试（已实现，待 DEB 实机验收）**：仅允许本机 Tauri command 在确认后重试 failed/blocked 交接；资格检查发生在任何 systemd 变更之前，随后停止可能残留的 daemon、等待 lease 释放、以当前登录偏好和新 request ID 原子重建 owner-only reservation，再受控重启。损坏 reservation 的显式替换不跟随 symlink，也不修改其目标；重试未开放给 HTTP、MCP、browser UI 或普通 app-settings patch。
   - **2H.3d.4c 登录偏好应用（已实现，待 DEB 实机验收）**：后台追踪开关只修改 `background_tracking_at_login` 并对账固定 unit 的 enable/disable，不把“当前运行”与“下次登录启动”混成同一语义；Desktop 登录和启动最小化继续走独立 XDG autostart 偏好。专用 Tauri command 以 completed reservation 记录持久意图，再应用 unit 并同步 SQLite 镜像；managed Desktop 启动时按 reservation 重新对账 unit 和 host-owned 数据，因此任一步中断都能在后续启动继续收敛。systemd 状态与意图不一致时诊断显示 `preference-mismatch`，普通 settings patch 不能绕过专用入口。
   - **2H.3d.4d 显式回滚（已实现，待 DEB 实机验收）**：本机确认式 Tauri command 先持久化 `rolling-back`，再让 systemd 停止 daemon，使 tracking/web session 通过正常 shutdown 封口；确认 lease 释放后禁用 unit、对账 Desktop autostart、保存后台登录偏好，最后提交 `rolled-back` 并受控重启。`rolling-back` 中断仍保持 client/fail-closed，可重复恢复；只有 `rolled-back` 才允许 embedded，且 embedded 启动会再次停用意外残留的 unit。损坏 reservation 可被原子替换，不跟随或修改 symlink 目标。
   - **2H.3d.4e Quiet Pro 控件（已实现，待 DEB 实机验收）**：Settings 的后台服务诊断区按后端能力和 reservation 状态显示登录启动、重试和回滚控件；重试与回滚必须经过确认，单一 action 状态会在操作期间禁用重复提交。`prepared/activating` 等进行中状态不开放变更，`rolling-back` 只允许幂等继续回滚，服务管理仍不开放给 HTTP、MCP、browser UI 或普通设置 patch。
5. **2H.3d.5 DEB 成品与实机验收（进行中）**：覆盖首次迁移中断、重复执行、unit 缺失、systemd 不可用、服务崩溃、Token/端口不一致、旧 XDG autostart、pending storage migration 和自定义挂载目录。最后在已安装 DEB 上验证登录启动、关闭 UI 后持续记录、重开 UI、锁屏/睡眠、浏览器活动、升级、卸载与数据保留。
   - **2H.3d.5a 成品静态验证（已实现）**：发布工作流在上传前解包最终 `.deb`，核对 `patina` 包名、版本、`amd64` 架构、Patina Desktop 与 `patinad` 可执行文件、固定 user unit、安全选项、GNOME 扩展 UUID，并拒绝通过维护脚本提前 enable/start `patinad.service`。该检查不安装软件，也不替代真实用户会话验收。
   - **2H.3d.5b DEB-only beta 发布契约（已实现）**：带预发布后缀的 daemon-backed 版本只构建和上传 `.deb`、对应签名、DEB updater 元数据及扩展资产；稳定 tag 仍保留 AppImage、DEB 和通用 AppImage fallback。发布说明、bundle target、资产复制、GitHub Release 附件和 `latest.json` 平台项由同一版本策略决定，并有自动化防止 beta 混入 AppImage。预发布 manifest 只挂在对应 prerelease，不替换稳定 `/releases/latest/`；专用 beta 自动更新通道不属于首次实机验收前置条件。
   - **2H.3d.5c 已安装包实机验收（进行中）**：只读验收采集器、旧版升级前基线、数据目录外的可恢复备份、`1.9.0-beta.1` DEB 安装、静态成品校验和完整 release gate 已完成。首次 owner 交接已达到 `completed`；关闭 Desktop 后 daemon 继续记录，重开 Desktop 未形成第二 owner；固定 service 崩溃后由 systemd 自动恢复；Firefox/Zen 扩展也已在 daemon 重启后重新连接浏览器桥接。`beta.1 → beta.2 → beta.3` 连续覆盖安装及受控 daemon 重启已确认 restart ticket、实例切换、数据库完整性、计数不倒退和扩展重连。`beta.2` 暴露的 completed reservation Desktop 重开错误等待健康 daemon lease 已在 `beta.3` 修复：实机启动跨过原 5 秒故障窗口，未再输出 lease timeout，daemon PID 与 lease 不变。`beta.3` 的 GNOME 锁屏/解锁实机验收也已通过：活动在锁屏后封口，解锁后才恢复，中间约 168.8 秒未计入使用时长，service 与 daemon lease 保持稳定。剩余关口是睡眠/恢复与音频/MPRIS 边界、后台登录偏好对账、设置页显式回滚与再次接管，以及卸载后的数据保留。

2H.3d.5c 使用同一 working 文档收口，不再新建一次性顶层文档。仓库提供 `npm run release:inspect-installed-patinad -- ...` 作为只读证据采集器；它只检查固定包路径、systemd 状态、owner 文件、SQLite `quick_check` 和裁剪后的 capability，不输出 API Token、窗口标题或 URL，不安装软件、不控制服务、不覆盖已有证据文件。输出文件使用 `create_new` 和 `0600`。

实机验收必须按以下顺序执行：

1. **升级前基线与可恢复备份**：关闭不必要的写入操作，通过设置页导出一份已验证的结构化备份，并把它保存在当前 Patina 数据目录之外；记录现有包版本、数据库完整性和行数基线。没有可读取的备份不得进入安装步骤。
2. **安装后、首次切换前**：安装静态验证已通过的 `X.Y.Z-beta.N` DEB，立即确认 Desktop、`patinad` 和 unit 来自同一包；维护脚本不得启动或启用 unit，用户数据与旧 XDG autostart 仍存在。
3. **首次 owner 交接**：启动 Desktop，完成显式迁移与受控重启；确认 reservation 为 `completed`、lease owner 为 `daemon`、systemd service active，并且 capability 同时报告 daemon runtime、tracking ready 和 managed service ready。任一条件失败均保持 fail-closed，先使用设置页重试或回滚，不手工删除 owner 文件。
4. **常驻与 Linux 信号**：关闭 Desktop 后保持正常操作，确认 session 与网页活动继续增长；再验证 GNOME 锁屏/解锁、睡眠/恢复、Zen/Firefox 网页同步、音频参与和 MPRIS，检查每个边界都封口且统计不倒退、不重复。
5. **崩溃恢复与偏好对账**：记录 `ExecMainPID`/`NRestarts` 后只对固定 `patinad.service` 注入一次失败，确认 systemd 生成新 PID、API 恢复、仍只有 daemon lease；分别验证后台登录偏好开关与 unit enable 状态收敛。
6. **显式回滚与再次接管**：从设置页执行回滚，确认 service 停止、reservation 为 `rolled-back` 且 daemon lease 释放；随后使用显式重试重新完成 managed 状态。不得通过删除 lock/reservation 模拟成功。
7. **升级与卸载**：用后一 beta 覆盖前一 beta，确认包内 Desktop/daemon 协议一致、数据和设置保留；卸载包后确认包属文件消失而数据库、备份与 Token 留存且数据库仍通过 `quick_check`。完成后可重新安装当前候选包继续使用。

建议证据命令如下；每个 `--output` 必须使用尚不存在的绝对路径：

```bash
npm run release:inspect-installed-patinad -- --phase baseline --expected-version 1.8.3 --output /tmp/patina-before-beta.json
npm run release:inspect-installed-patinad -- --phase installed --expected-version "$BETA_VERSION" --output /tmp/patina-beta-installed.json
npm run release:inspect-installed-patinad -- --phase managed --expected-version "$BETA_VERSION" --output /tmp/patina-beta-managed.json
npm run release:inspect-installed-patinad -- --phase rolled-back --expected-version "$BETA_VERSION" --output /tmp/patina-beta-rolled-back.json
npm run release:inspect-installed-patinad -- --phase uninstalled --output /tmp/patina-beta-uninstalled.json
```

以上文件只证明采集时刻。关闭 UI 后持续记录、锁屏/睡眠、浏览器活动和崩溃重启仍需在动作前后各采集一次，并核对 PID、restart count、session/web row counts 与时间边界；单份“最终正常”快照不能替代中断过程证据。

2H.3d.3d 的自动化证据矩阵：

| 故障或中断点 | 安全行为 | 自动化证据 |
| --- | --- | --- |
| `prepared` / `activating` / `completed` 后进程退出 | 重启后仍选择 daemon client，且只有前三种状态允许启动或确认服务 | `every_persisted_boundary_keeps_one_safe_startup_owner` |
| `failed` 或 `rolling-back` 后进程退出 | 保持 fail-closed；失败不隐式重试，回退中不启动 embedded | `every_persisted_boundary_keeps_one_safe_startup_owner`、`failed_cutover_never_falls_back_to_embedded_or_retries_implicitly` |
| `rolled-back` 已提交 | 只有该最终状态恢复 embedded owner | `rollback_keeps_the_client_owner_until_all_external_work_is_complete` |
| 桌面端重复启动或重复状态推进 | `prepare`、`mark_activating`、`mark_completed` 和 rollback 均幂等 | `prepare_is_idempotent_and_creates_an_owner_only_marker`、`activation_and_completion_are_durable_and_idempotent`、`rollback_from_failure_clears_failure_and_is_idempotent` |
| 旧进程携带过期 request ID | 不能激活、完成或标记新 reservation 失败 | `wrong_request_id_cannot_advance_the_reservation` |
| service 启动失败 | 持久化 `failed`，后续启动不回退 embedded | `every_persisted_boundary_keeps_one_safe_startup_owner` |
| daemon API 未 ready 或暂时不可达 | 在有界确认窗口内重试 | `cutover_confirmation_retries_startup_and_fails_fast_on_protocol_errors` |
| daemon 协议不兼容或 runtime host 错误 | 立即停止重试并持久化失败路径 | `cutover_confirmation_retries_startup_and_fails_fast_on_protocol_errors`、`only_permanent_negotiation_errors_abort_cutover_immediately` |
| 旧 Desktop 尚持有 RuntimeLease | 新 owner 等待；超时则拒绝继续，不抢占 lease | `release_barrier_waits_for_the_previous_owner_without_taking_ownership`、`release_barrier_times_out_while_an_owner_is_alive` |

切换约束：开发版和现有已安装稳定版继续默认 embedded；只有完成 reservation 的安装版才默认 daemon client。`--daemon-client-preview` 在 beta 验收期继续保留，显式 embedded 回滚只用于开发/故障恢复并必须经过 RuntimeLease。Stage 2H.3d.5 通过前不发布 daemon-backed stable，也不恢复 AppImage 发布。

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

Stage 2F.2 已由 Axum + Tower 承接通用 HTTP API、SSE、浏览器 bridge、并发预算和优雅关闭，并删除两套自写 parser/server loop 与 SSE writer。普通 API、SSE 和浏览器 bridge 分别使用 32、8 和 8 的 fail-fast 并发预算。API 只接受 loopback Host，有 Origin 时只允许 loopback HTTP(S) 或 `tauri://localhost`；bridge 只接受 loopback Host，有 Origin 时只允许 `moz-extension://` 或 `chrome-extension://`。无 Origin 的本机 Bearer 客户端保持可用，两类 listener 继续使用独立 credential 和 origin policy。

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

状态：Stage 2A 至 Stage 2H.2 的 preview 能力迁移、数据语义、loopback transport、daemon owner 收口和 systemd restart handoff 已完成；显式 Desktop client 的只读 runtime 和首批写侧 command 已接通，默认 owner 切换仍待实施。

- 已完成：有界事件中心、受认证 SSE、replay/resync、能力协商和干净关闭
- 已完成：显式模式下 daemon 接管 tracking/watchdog、实时快照、session 写入和退出封口
- 已完成：共享 logind power source 与 daemon lock/suspend/resume/shutdown owner
- 已完成：daemon 接管 Linux audio source，按设置启停并在退出时取消
- 已完成：daemon 接管 Linux MPRIS source，多播放器按当前窗口优先解析并在退出时取消
- 已完成：daemon 接管 browser activity bridge，共用鉴权、隐私、记录、事件和受限请求生命周期
- 已完成：通用 API/SSE 和浏览器 bridge 使用 Axum + Tower，具有独立并发预算、各自 Host/origin 边界、task readiness 和有界关闭
- 已完成：daemon owner 拆分，聚合 runtime 只保留装配和有序关闭
- 已完成：显式 `--daemon-client-preview` 通过 typed daemon client 和 event stream 获取 tracking 状态，且不启动第二套 tracker
- 已完成首批写侧转发：AFK threshold、tracking pause、audio participation、classification 与 Tools 通过 daemon API 执行
- 已完成：browser runtime、local API 配置和白名单内普通 app settings 由 Desktop Rust host 转发给 daemon；端口或 Token 变化会主动重建 client 与 event stream
- 已完成：Settings session cleanup 与历史窗口标题清除由 Rust data owner 事务执行；daemon-client 模式通过要求 `confirmed: true` 的 typed daemon API 执行，前端不再直接发删除 SQL
- 已完成：本地备份导出使用跨表 SQLite snapshot transaction 和 owner-only 原子文件发布；备份读取已限制 archive/entry/解压总量并拒绝符号链接与重复 ZIP entry
- 已完成：活动导入提交/列表/删除通过 owner-only 暂存票据和 typed client 迁移到 daemon，Desktop 不向 API 发送任意路径或大文件正文
- 已完成：本机定时备份由 daemon 唯一持有调度循环、配置与运行状态；Desktop typed client 可读取和显式确认完整配置，任务关闭会等待当前归档安全结束
- 已完成：按应用删除通过受确认的 daemon API 在单事务内清理原生和导入事实并返回计数；Desktop preview 不再直接打开 SQLite 写入
- 已完成：受控 backup restore 以及 remote backup 非密钥配置、Linux 系统凭据和显式上传的 daemon owner 迁移
- 已完成并验证：remote backup 列表、有界下载与受控恢复衔接；daemon client 不再把下载路径交回 Desktop

验收：关闭 UI 后继续记录；重开 UI 恢复当前状态；AFK、锁屏、睡眠、恢复和异常封口正确；统计不倒退、不重复。

### 阶段 2F.1：后台稳定化

该阶段不增加用户功能，先把 preview 能力收敛为可长期运行的服务边界：

- 已完成 data owner：网页 active row 按 `updated_at` 恢复并受启动时间上限保护，不把停机空白计入 duration
- 已完成 web activity engine：30 秒扩展心跳使用 75 秒 connected 宽限；15 秒 watchdog 在过期后按最后成功上报时间封口，并在 data owner 中防止并发新上报被旧检查误封
- 已完成 verification first：跨夜崩溃恢复、心跳抖动、扩展消失和数据库观测边界已有测试，再进入 transport 与 owner 结构修改
- 已完成 API transport：Axum + Tower 替换通用 API 的自写 HTTP parser、server loop 和 SSE transport；普通 API 和 SSE 分别使用 32/8 的 fail-fast 并发预算
- 已完成 platform transport：browser bridge 使用 Axum 并设置独立 8 请求并发上限
- 已完成 API boundary：API 使用严格 origin/CORS 与 loopback Host 校验；无 Origin 的 Bearer 客户端保持兼容
- 已完成 browser boundary：浏览器扩展保留独立 listener 和 Token，只回显 Firefox/Chromium 扩展 Origin，不复用通用 API 的 origin policy
- 已完成 transport health：通用 API listener/task readiness 已联动，意外退出会使 desktop 诊断降级或触发 daemon 受控停机；browser bridge 正常退出、panic 或 abort 均立即把 readiness 降级
- 已完成 daemon ownership：tracking、power、audio、media 和 web activity 生命周期已移入对应 owner 模块，`app/daemon/runtime.rs` 只保留编排和关闭顺序
- workspace boundary：首个 daemon-backed 里程碑保持当前 Rust package，不把 Cargo workspace 重排混入 owner 迁移
- verification complete：继续覆盖连接饱和、listener 意外退出和有序 shutdown

验收：daemon 停机不增长 session 或网页活动；短暂心跳抖动不误报断开；扩展消失后网页段不会无限增长；连接压力不会产生无界任务；readiness 与实际服务状态一致。

### 阶段 3：Linux 服务化与桌面客户端切换

- 已完成第一批写侧基础：capabilities 暴露服务版本、协议上下限和 write scopes；tracking owner daemon 开放事务化 app mapping、classification、AFK threshold 与 tracking pause，默认 daemon 仍严格只读
- 已完成 runtime settings owner：audio participation 可热切换；browser bridge 端口、Token、启停和 URL 隐私以完整配置原子应用，换端口失败时保留旧 listener 与旧存储
- 已完成：Tools runtime tick、启动恢复、Linux 通知、SSE 事件和 HTTP/MCP 写侧由 daemon owner 接管
- 已完成 Stage 2H.1：API listener 换端口使用预绑定/提交/切换，Token 文件原子轮换并撤销旧 bearer/SSE，HTTP/MCP 响应不返回密钥
- 已完成 Stage 2H.2：DEB 构建输入包含 daemon 与默认禁用的 user unit；受控 restart 使用跨实例持久化 ticket，手工 preview 不可误触发
- 已完成 Stage 2H.3a：通过用户会话 D-Bus 查询 systemd user service 真实状态，识别旧 desktop autostart 的迁移条件，并在默认 owner 切换前保持服务启用动作关闭
- 已完成 Stage 2H.3b.1：typed loopback client、Bearer 认证、runtime host 与协议协商，以及真实 API transport 回归测试
- 已完成 Stage 2H.3b.2：`/current`、active session 与标准 SSE parser 已接入只读 runtime adapter，具有 subscribe-before-read、cursor replay、resync 全量重读、有限重连和显式 shutdown
- 已完成 Stage 2H.3b.3：显式 preview 模式不运行 embedded tracker/API/browser/Tools，并复用现有 tracking commands 和前端事件；自动化与真实 GNOME 会话已验证唯一 daemon lease、完整状态读取，以及 desktop 退出后 daemon 持续追踪
- 已完成 Stage 2H.3c.1：受管 daemon client state 承接 AFK threshold、tracking pause、audio participation、classification 和全部 Tools command；Tools SSE 失效通知会重读完整 snapshot，embedded 模式保持原行为
- 已完成 Stage 2H.3c.2 核心设置迁移：browser/local API 配置与普通 app settings 通过 daemon API 写入；运行中换端口或轮换 Token 会通过共享 revision 主动重建 Desktop client、SSE、Tools 刷新和诊断请求。Desktop 会在发起首个资源写入前校验完整设置 patch；多个专用 runtime endpoint 之间不承诺跨资源事务，后续若开放非 UI 调用方，需升级为 daemon 侧统一批量命令或提供明确补偿语义
- 已完成 Stage 2H.3c.3：活动导入的文件选择/暂存归 Desktop，文件复核、数据库提交和批次删除归 daemon；暂存票据一次性消费并受路径、权限、大小和指纹约束
- 已完成 Stage 2H.3c.4：定时备份配置、调度 tick、运行状态、安全文件发布和保留策略归 daemon；Desktop 通过 typed client 读写，SSE 只发送失效通知，客户端重读完整 snapshot
- 已完成 Stage 2H.3c.5：按应用删除要求显式确认和有界 executable/range，请求通过 typed client 交给 daemon，在同一事务维护原生、导入事实和批次计数
- 已完成 Stage 2H.3c.6：受控恢复使用 owner-only 暂存、systemd restart、启动维护事务和 durable receipt
- 已完成 Stage 2H.3c.7a：WebDAV 非密钥设置走 daemon app-settings，Linux 密码走 profile-scoped Secret Service，显式上传由 daemon 从自己的 SQLite snapshot 执行
- 已实现 Stage 2H.3c.7b：remote backup 列表、ID 派生路径校验、有界下载与下载归档进入受控恢复状态机的衔接；完成门禁后 Stage 2H.3c 写侧收口
- 已完成 Stage 2H.3d.1：固定 unit 的受限 systemd 控制基础，以及 Production embedded 启动前的单 owner 防护
- 已完成 Stage 2H.3d.2 数据语义：后台追踪与桌面客户端登录偏好分键，旧值只作一次性缺省来源，host-owned 偏好不随备份覆盖目标机器；当前尚未开放日常设置 UI
- 已完成 Stage 2H.3d.3a-d：owner-only reservation、Production embedded 受控重启、旧 lease 释放屏障、managed client 激活、readiness 确认及全部持久状态的中断恢复自动化；真实 unit mutation 尚待 DEB 验收
- 已完成 Stage 2H.3d.4a：交接 reservation 与 fixed unit 状态合并为 Tauri 专属诊断，Settings 能明确显示 pending、failed、managed 和 managed-blocked
- 已完成 Stage 2H.3d.4b：本机确认式重试先验证状态、停止 daemon 并等待 lease，再以新 reservation 受控重启；Settings 只在 failed/blocked 状态开放该入口
- 已完成 Stage 2H.3d.4c 后端：completed reservation 持有后台登录意图，专用 Tauri command 串行应用 systemd 与 SQLite 镜像，启动路径负责中断后对账
- 已完成 Stage 2H.3d.4d 后端：`rolling-back → rolled-back` 保证 daemon 正常封口、lease 释放和 unit 禁用发生在 embedded 恢复之前，中断不会产生双 owner
- 已完成 Stage 2H.3d.4e：Quiet Pro 诊断区按状态开放后台登录偏好、显式重试和安全回滚，危险操作确认且执行期间禁止重复提交
- Stage 2H.3d.5a 成品静态验证和 2H.3d.5b DEB-only beta 发布契约已完成；当前只剩 2H.3d.5c 已安装包实机验收
- Tauri 改为 daemon desktop client，并保留 tray、通知、文件选择和 updater
- 默认切换后 desktop 不启动或自动回退 embedded tracker；daemon 不可用时明确暂停、诊断和重启
- 一个 `patina` 产品包同时交付 Patina Desktop、`patinad` 和 systemd user unit
- DEB 不在 `postinst` 全局 enable；首次桌面启动在用户会话中迁移并启用服务
- “后台追踪随登录启动”与“桌面客户端随登录打开”的持久化语义已拆分；启动时最小化只属于桌面客户端，首次交接会应用服务状态，日常修改待专用设置入口
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
