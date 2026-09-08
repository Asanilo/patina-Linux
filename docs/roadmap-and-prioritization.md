# 路线图与优先级规范

## 1. 文档定位

本文定义 `Patina` 当前阶段的长期路线主题与默认优先级判断规则。

它不是一次性开发计划，也不是细粒度 backlog，而是以后面对多个方向竞争时，用来回答这些问题的长期基线：

- 先做什么，后做什么
- 什么情况可以打断当前主线
- 什么类型的工作应该延后
- 架构整理什么时候应该让路，什么时候应该提升优先级

---

## 2. 与其他长期文档的关系

- [`product-principles-and-scope.md`](./product-principles-and-scope.md) 解决“什么值得做”；本文解决“值得做的事情先后怎么排”。
- [`architecture.md`](./architecture.md) 定义长期结构边界；本文定义什么时候应为结构收口让出优先级。
- [`issue-fix-boundary-guardrails.md`](./issue-fix-boundary-guardrails.md) 约束具体问题怎样分流与修复；本文约束这些事情在当前阶段的真实优先级。
- [`versioning-and-release-policy.md`](./versioning-and-release-policy.md) 约束发布前的版本与验证；本文约束什么主题值得进入当前发布线。

---

## 3. 当前阶段判断

当前仓库处于：

- `1.x` 稳定阶段
- 以个人、本地优先、Linux 桌面时间追踪为稳定产品边界，当前优先支持 GNOME Wayland
- 正在把后台追踪主链从 Tauri 桌面宿主渐进迁入 `patinad`
- 在兼容性与可维护性前提下继续围绕“可信、可长期使用、可持续演进”收口

因此当前路线不按“大扩张期”管理，而按“稳定维护期 + 核心体验打磨”管理。

这不是停止演进，而是要求新增能力先服从追踪正确性、数据安全、核心体验与长期维护性。

这个阶段最重要的不是同时开很多方向，而是把核心桌面产品打磨到：

- 更可信
- 更清晰
- 更稳
- 更容易持续维护

---

## 4. 当前阶段的北极星

当前阶段的北极星不是“功能数量最多”，而是：

**把 `Patina` 打磨成一个可信、可读、可控、可长期使用的个人桌面时间追踪工具。**

所有优先级判断，默认都应回到这句话。

如果某项工作能明显增强下面四个词中的至少一个，它通常更值得优先做：

- `可信`
- `可读`
- `可控`
- `可长期使用`

---

## 5. 路线图主题

当前阶段长期优先围绕以下 5 个主题推进。

## 5.1 主题一：追踪正确性与可信度

这是当前最高优先级主题。

包括但不限于：

- 前台应用识别准确性
- 会话切分正确性
- `AFK / 锁屏 / 睡眠` 边界
- 崩溃恢复、心跳、封口逻辑
- 用户可解释的统计一致性

只要这个主题存在明显缺口，其他锦上添花类功能都不应长期压过它。

## 5.2 主题二：数据安全与控制力

时间追踪产品只有在用户相信自己掌握数据时，才可能被长期使用。

包括但不限于：

- 本地数据库稳定性
- 备份 / 恢复
- 清理历史
- 标题记录控制
- 设置与数据行为的可解释性

当前基线中，本地存储位置与安全缓存控制已实现：活动数据库和 WebView 持久目录可独立迁移，迁移在重启时验证执行，旧目录保留；缓存维护只允许清理精确的 `WebKitCache`。后续工作应以回归验证和恢复可靠性为主，不再重复建设另一套存储向导。

本地自动备份也已进入当前基线：支持按本机时间每天或每周执行，只写入用户选择的本机目录；文件采用不覆盖创建并在发布后重新解析校验，失败会有限重试，异常退出会在下次轮询对账。系统默认保留最近 3 份已验证且有数据库归属记录的快照，清理前必须再次核对目录、文件类型、大小和摘要；无法确认归属时保留文件。覆盖恢复会停用并换代原计划。显式 WebDAV 备份已进入 daemon preview：密码存放于按 profile 隔离的系统凭据服务，上传从 daemon 自有 SQLite snapshot 完成；列表与恢复也由 daemon 有界读取、校验，并复用既有启动恢复状态机，不向 Desktop 暴露下载路径。自动上传 WebDAV 不属于当前基线，不能把手动配置隐式升级为后台任务。

平台中立的 Patina CSV 活动导入也已进入桌面端基线：导入前预览并在提交时复核文件指纹，记录保存在独立事实表中，可按批次删除；本机精确记录优先于外部精确记录，外部精确记录优先于小时汇总。小时汇总只进入聚合统计，不进入 History 时间线。导入事实随手动和定时备份保存，并纳入恢复、历史清理、标题清理和应用删除。Tai / Taix 等来源适配器不属于当前基线。

## 5.3 主题三：核心页面体验打磨

核心页面是当前产品价值的主要承载面。

包括但不限于：

- `Dashboard` 的可读性
- `History` 的回看效率
- `Data` 的长期活动理解效率
- `Data` 当前已支持应用、分类与网页趋势；网页趋势按需读取域名级活动并遵循现有分类、排除和隐私边界，不复制上游持久化聚合链路
- `Dashboard / History / Data / Classification` 的桌面 SQLite 读模型已能组合本机事实与外部导入事实；本机记录始终优先，局部小时范围先按覆盖比例折算，小时汇总不伪造成 History 时间线
- HTTP / MCP 的 Summary、Trend 与 Apps 已通过 Rust 共享活动读模型纳入外部导入事实，并与桌面端遵守同一优先级契约；`/sessions` 仍只暴露原生精确记录，小时汇总只用于聚合，不能在 API handler 中复制另一套统计规则
- 后续 Data 工作优先统一跨页面的日期、详情与筛选语义并补回归验证，不为单一图表新建后台聚合 worker 或第二套事实表
- 应用详情已由 `Dashboard / History / Data` 共用同一 owner，网站详情已由 `History / Data` 共用同一 owner；后续新增入口必须继续保持同一聚合、日期和时间线语义
- `Classification` 的管理清晰度
- `Tools` 的轻量主动工具体验
- `Settings` 的行为透明度
- `About` 的版本、更新与反馈入口清晰度
- Quiet Pro 一致性

## 5.4 主题四：关键路径上的结构收口

结构整理不是为了“更漂亮”，而是为了让关键路径更稳定、更少回流。

当前主题四关注的是：

- 防止前端 `app/*` 重新长厚
- 防止 `shared/*` 重新变成公共垃圾桶
- 防止 `platform/*` 重新变成无 owner 的外部适配混合层
- 防止 Rust `lib.rs`、`commands/*` 重新承接厚逻辑
- 防止已退出的根层 `src/lib/`、`src/types/` 被重新引入
- 让兼容壳继续变薄，而不是变成新主路径
- 建立不依赖 `AppHandle` 的共享运行内核、事件出口与数据上下文
- 让 tracking、watchdog、平台信号、本地 API 和浏览器桥接最终拥有唯一 daemon owner
- 让桌面 UI 逐步成为后台运行时的客户端，而不是继续拥有第二套 tracker

只有当结构问题已经开始影响修复效率、稳定性或正确性时，这个主题才应被明显提升优先级。

## 5.5 主题五：发布与开源可维护性

个人开源产品如果没有稳定的发布、文档与验证节奏，很容易变成“只有作者自己知道怎么发、怎么用”的仓库。

包括但不限于：

- 版本与发布流程稳定化
- README 与长期文档维护
- 安装、运行、验证路径清晰
- 回归检查可重复

当前仓库不再参与或持续跟踪上游 Windows 主线。外部项目和上游实现仍可作为普通技术参考，但不建立功能追平义务，也不以版本差异驱动路线图。

仓库中保留的 Windows 源码进入冻结兼容期：

- 不新增 Windows 功能、测试、安装和发布工作
- 不把 Windows 平台行为作为 Linux 设计的兼容约束
- 不在 `patinad` 稳定前并行开展大规模删除
- daemon 稳定后，以独立阶段删除 Windows cfg、依赖、源码和历史文档

### 5.6 当前实施主线：`patinad`

`patinad` 是当前唯一的架构实施主线。Linux `main` 作为已发布桌面产品的稳定功能基线，在 daemon-backed beta 完成前不再单独扩展一条持续追平 Windows 上游的功能线。上游改动只作为定期审查输入，满足以下条件之一时才进入当前实施序列：

- 修复计时正确性、数据安全、隐私或安全边界问题
- 对 Linux 同样成立，且能够落入现有明确 owner
- 能以较小冲突改善高频核心页面，同时不会让 embedded runtime 与 daemon 同时增长

Windows runtime、installer、updater、ARM/UWP 等平台专属实现不移植；纯功能扩张、本地化扩张和低优先级界面增强默认等 daemon-backed beta 后再评估。同步上游时按行为契约重新实现或选择性移植，不整体 merge `upstream/main`，也不以版本号追平作为完成标准。

当前分支收敛顺序固定为：

1. 把 Linux `main` 已验证的功能提交合入 `feature/patinad-daemon`，只做稳定基线到未来架构线的单向收敛。
2. 审计活动导入、定时备份和其他新增写路径，确保 daemon client 模式不会重新绕过 runtime/database owner。
3. 按语义审查上游 `1.9.5` 的计时边界、网页区间去重和备份恢复边界修复；只移植 Linux 仍缺失的部分。
4. 完成受控 backup restore，并分批收口 remote backup 的配置、上传、列表、下载与恢复衔接。
5. 完成首次启动迁移、systemd 服务控制、默认 owner 切换和双 owner 验收。
6. 发布 daemon-backed DEB beta 并完成登录启动、关闭 UI 后持续记录、崩溃恢复、升级、卸载和数据保留验证。

当前执行位置：第 1 步已完成单向合流；第 2 步已完成 owner 审计和 fail-closed 防护，活动导入已通过 owner-only 暂存票据收口，定时备份也已由 daemon 持有唯一调度任务、配置 API、运行状态与 SSE 失效通知，按应用删除已通过受确认的事务 API 和 typed client 收口。第 3 步已移植启动恢复、采样恢复、watchdog 竞态、browser bridge 重试、网页趋势区间去重、power lifecycle generation、暂停原子边界，以及网页段与活动原生浏览器 session 的持久化事务绑定。第 4 步已完成受控恢复及 remote backup owner 收口：非密钥配置、Linux 系统凭据、上传、列表、有界下载和启动恢复衔接均已落地。第 5 步 Stage 2H.3d 已完成受限 systemd 控制基础、登录偏好的持久化语义拆分、两阶段 owner 交接代码路径、交接失败诊断及本机显式重试后端；当前按“登录偏好应用 → 安全回滚 → Quiet Pro 控件 → 中断及 DEB 实机验收”推进。真实 user service mutation 最后随 DEB 实机验收执行。详细安全顺序以 [`working/2026-07-10-patinad-runtime-design.md`](./working/2026-07-10-patinad-runtime-design.md) 为准。

在第 6 步完成前，不再把新的上游大型功能只加入 Linux `main` 而不进入 patinad 架构线。

当前结构主线按以下顺序推进：

1. Stage 0 基础已完成：数据 profile、存储锚点、运行时唯一 owner、API 凭据、受限请求解析与生命周期、OpenAPI 一致性和优雅关闭已有自动验证。
2. Stage 1 已完成：共享 `RuntimeContext`、`RuntimeEventSink`、API runtime context、完整只读 GET API 和 host-neutral handler 已落地。
3. Stage 2A 已完成：本机有界 event stream、bearer 认证、replay/resync、能力协商和关闭语义已有自动验证。
4. Stage 2B tracking preview 已完成：显式 `--track` 模式由 daemon 接管 tracking/watchdog、实时快照、session 写入和退出封口；desktop 默认 owner 尚未切换。
5. Stage 2C power preview 已完成：共享 logind source 覆盖锁屏、解锁、休眠、恢复和关机，daemon 可在对应边界立即封口。
6. Stage 2D audio preview 已完成：Linux audio source 可由 daemon 显式拥有、取消和按设置启停，PulseAudio/pipewire-pulse 探测不再依赖 Tauri 全局运行时。
7. Stage 2E MPRIS preview 已完成：Linux media source 可由 daemon 显式拥有和取消，多播放器按当前窗口身份优先匹配，D-Bus 查询不依赖 Tauri 全局运行时。
8. Stage 2F browser bridge preview 已完成：显式 tracking 模式由 daemon 在 loopback 接收浏览器扩展上报，共用宿主无关的鉴权、隐私、记录和封口逻辑，并提供受限请求生命周期与可等待关闭。
9. Stage 2F.1 数据语义已完成：网页异常退出按最后可信观测时间恢复；浏览器心跳使用 75 秒宽限并由 watchdog 在过期时按最后上报封口；跨夜崩溃、心跳抖动、扩展消失和并发更新保护已有回归测试。
   网页写入还必须匹配当前活动的原生浏览器 session；两者通过持久化关系表绑定，原生 session 结束时由 SQLite 在同一事务内截断网页段。备份格式已向后兼容保存该关系，Replace/Merge 使用恢复后的 session ID 重建绑定。
10. Stage 2F.2 transport 已完成：desktop 与 daemon 共用的 API/SSE、独立浏览器 bridge 均已迁移到 Axum + Tower，不再保留自写 HTTP parser/server loop/SSE writer；普通 API、SSE、browser bridge 分别使用 32/8/8 的 fail-fast 并发预算。API 只允许 loopback Host/origin，bridge 只允许 loopback Host 与 Firefox/Chromium 扩展 Origin；listener readiness 跟随真实 task 生命周期，daemon API 意外退出会触发受控停机，bridge 意外退出会立即降级诊断状态。
11. daemon owner 拆分已完成：tracking、power、audio、media 与 web activity 的任务状态、重试、取消和退出封口已回到 `app/daemon/runtime/*` 对应 owner 模块；`app/daemon/runtime.rs` 只保留依赖装配、启动顺序和有序关闭，且未混入 Cargo workspace 重排。
12. Stage 2G Tools runtime preview 已完成：服务版本、协议上下限、write scope 协商、app mapping、classification、AFK threshold、tracking pause、运行中 browser/audio 配置，以及 Tools runtime owner、系统通知、SSE 与写侧 HTTP/MCP 已完成。
13. Stage 2H.1 local API configuration preview 已完成：daemon 从 profile 存储读取端口并迁移旧数据库 Token，运行中换端口采用预绑定/提交/切换，Token 原子轮换后撤销旧 bearer 与 SSE，会通过 HTTP/MCP 暴露不含密钥的确认状态。
14. Stage 2H.2 systemd service preview 已完成：DEB 构建输入包含 `patinad` 和默认禁用的 user unit；受控重启先持久化 owner-only ticket、返回 `202 pending`，再优雅退出并由 systemd 重启，下一实例确认同一 ticket。下一步仍需首次桌面启动迁移、服务启停设置和默认 owner 切换；切换后不自动回退 embedded tracker。
15. Stage 2H.3 分阶段完成默认切换：2H.3a 已完成 systemd 状态与迁移诊断；2H.3b.1/2 已完成安全 typed daemon client 和只读 runtime adapter；2H.3b.3 已完成显式 preview、embedded owner 隔离和真实 GNOME 验收；2H.3c.1 至 2H.3c.7b 已完成 Desktop 写侧、活动导入、定时备份、按应用删除、受控恢复和 remote backup owner 收口。2H.3d.1 已完成固定 unit 的受限控制基础，2H.3d.2 已完成后台追踪与 Desktop 登录偏好的持久化拆分，2H.3d.3a-c 已完成 fail-closed reservation、embedded 受控重启、旧 lease 屏障和 managed client readiness 确认代码路径，2H.3d.4a 已补交接状态及失败原因诊断；当前按显式重试、登录偏好应用、安全回滚和中断验收继续，DEB 实机通过前不宣称默认切换完成。
16. 用一个 `patina` 产品包同时安装 Patina Desktop、`patinad` 和 systemd user unit；首次桌面启动在用户会话中迁移旧 XDG autostart 并启用后台服务，把“后台追踪随登录启动”与“桌面客户端随登录打开”拆成独立设置。
17. 首个 daemon-backed DEB 先发布为 beta，验证关闭 UI 后持续记录、登录启动、崩溃重启、锁屏、睡眠、浏览器活动、升级、卸载和数据保留；该 beta 只发布 DEB，不发布无法稳定安装 service owner 的 AppImage。embedded runtime 至少保留一个稳定版本作为显式开发回滚路径。
18. beta 验收后让完整 monorepo 脱离 Windows 上游 fork network，保留 Git 历史、MIT 许可与 attribution；不拆分独立 `patinad` 仓库。
19. daemon-backed 稳定版发布前，必须单独决定并验证 AppImage 的版本化 daemon extraction 与原子更新，或设计对现有 AppImage 用户明确且不循环更新的退役迁移；不能让 DEB-only stable 悄悄破坏既有 updater contract。
20. daemon 稳定后建立只读本机浏览器 UI，先覆盖 Dashboard、History、当前会话和诊断；使用 same-origin HttpOnly session，不向前端 JavaScript 暴露长期 API Token。
21. 浏览器只读路径稳定后再开放受控写操作；MCP、CLI 和 Agent 继续使用 Bearer Token，并与浏览器 UI 复用同一业务 API 契约而非同一认证方式。
22. 之后开发 TUI / CLI 并开始 KDE Wayland 适配；桌面端是否从 Tauri 迁往 GPUI、是否拆 Cargo workspace，只按实测资源、构建和独立打包收益评估。
23. `patinad` 稳定后，单独分阶段删除冻结的 Windows 平台代码，不与 owner 切换、transport 迁移或数据修复混合。

每一阶段必须保持当前桌面主路径可用，不以一次性切换换取架构完成感。

---

## 6. 默认优先级顺序

在没有更强理由时，默认按以下顺序排序：

1. 会导致错误记录、错误统计、数据风险或明显信任受损的问题
2. 会明显影响高频主路径使用的问题
3. 会持续制造回归或拖慢修复效率的关键结构问题
4. 会提升核心页面理解效率与日常使用舒适度的改进
5. 低频增强、边缘功能和实验性想法

换句话说：

- `可信度问题` 高于 `体验润色`
- `高频主路径问题` 高于 `边缘功能扩展`
- `关键结构债务` 高于 `装饰性增强`
- `长期稳定收益` 高于 `短期新鲜感`

---

## 7. 评估维度

评估一个事项时，默认从下面 6 个维度判断：

## 7.1 对信任的影响

如果它会让用户怀疑：

- 记录是不是对的
- 统计是不是准的
- 数据是不是安全的

那它的优先级通常应上升。

## 7.2 对高频使用的影响

如果它发生在用户几乎每天都会经过的主路径上，优先级通常高于低频后台能力。

## 7.3 风险与可逆性

如果一个问题一旦发生后果较重，或事后难以修复，优先级应上升。

典型例子包括：

- 数据损坏
- 错误清理
- 持续串记
- 无法恢复的状态错误

## 7.4 受影响范围

影响多数用户、多条主路径或多个核心页面的问题，通常高于只影响单一边缘场景的问题。

## 7.5 结构杠杆

如果一个改动虽然不显眼，但能明显降低后续修复成本、减少边界回流或稳定关键链路，它应被看高一层。

## 7.6 机会成本

如果某个方向会占用大量时间，却只带来较弱产品收益，应被谨慎降级。

---

## 8. 什么情况可以打断当前主线

以下事项默认允许插队，必要时可立刻打断原计划：

## 8.1 可信度问题

- 会话明显记错
- 统计结果明显不可相信
- `AFK / 锁屏 / 睡眠` 边界失真
- 产品看起来在“偷偷多记时间”

## 8.2 数据安全问题

- 数据丢失风险
- 备份/恢复不可靠
- 清理操作可能误删
- 版本升级后存在数据兼容风险

## 8.3 主路径严重回归

- `Dashboard / History / Data / Classification / Tools / Settings / About` 的核心主路径被明显破坏
- Linux 桌面行为与当前承诺不一致
- 构建、启动、基础使用或发布流程出现阻塞

## 8.4 明显的长期边界回流

如果某处结构问题已经反复引发修复成本与新 bug，它可以临时上升到更高优先级。

---

## 9. 什么情况默认不应插队

以下事项默认不应打断当前主线，除非它们突然与核心价值直接相关：

- 低频边缘场景的体验润色
- 主要为了“看起来更高级”的装饰性调整
- 与当前主方向弱相关的复杂可视化
- 团队协作、云同步、账号体系等平台化扩展
- 为未来假想规模提前铺很重的系统

---

## 10. 当前阶段应主动压低优先级的方向

以下方向不是永远不做，而是在当前阶段应主动压低优先级：

- 云同步
- 账号体系
- 团队与组织功能
- 移动端优先体验
- 多平台同步铺开
- 重型 AI 洞察与自动分析
- 为展示效果服务的大型新表面

原因不是它们一定没价值，而是它们当前都更容易把资源从核心桌面产品上抽走。

---

## 11. 新需求进入路线前的默认判断

一个新需求要进入当前路线，默认需要回答清楚下面几个问题：

- 它服务当前核心用户吗
- 它增强了 `可信 / 可读 / 可控 / 可长期使用` 中的哪一项
- 它解决的是高频真实问题，还是低频想象性问题
- 它会不会显著增加维护复杂度
- 它是否能在 [`architecture.md`](./architecture.md) 的长期边界内稳定落地
- 它是否会把产品带向当前非目标范围

如果这些问题回答不清，默认先不进入当前路线。

---

## 12. 重构类工作的优先级规则

重构不应因为“代码看着不顺眼”就自动进入高优先级。

以下情况，重构值得进入当前路线：

- 已经阻碍高频修复
- 已经反复制造回归
- 已经让可信度相关问题难以修正
- 已经阻碍发布稳定性

以下情况，重构默认延后：

- 只是形式上不够整齐
- 不影响当前关键路径
- 只是“以后也许会更好维护”

---

## 13. 什么时候更新本文

本文维护的是长期排序规则与主题，而不是具体任务列表。

因此它的更新频率应低于一次性执行单，通常只在以下情况更新：

- 产品阶段变化
- 长期优先级顺序变化
- 原本的低优先级方向被正式提升
- 当前主线已经明显完成，进入下一阶段

---

## 14. 给 Codex 与后续协作者的默认约束

当存在多个可做方向时，默认行为应是：

- 先判断它属于哪个路线主题
- 再判断它是否真的应该排到当前阶段前列
- 不因为“做起来有意思”就抬高优先级
- 不因为“属于架构整理”就自动压过可信度问题
- 不因为“新功能更显眼”就自动压过高频主路径改进

如果一个提案明显越过当前阶段，正确做法通常不是立刻实现，而是先记录为未来方向，保持当前主线不被打散。
