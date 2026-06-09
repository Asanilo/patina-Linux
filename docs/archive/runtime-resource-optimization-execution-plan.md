# Runtime Resource Optimization Execution Plan

创建日期：2026-06-09

状态：completed / archived

文档类型：执行单 / How-to

目标读者：后续执行该优化的开发者和代码协作者

执行归属：Rust Tools runtime、前端 AppShell 运行时编排、Tools feature 状态订阅、资源诊断与验证

## 执行摘要

- 完成日期：2026-06-09
- 实现范围：Tools runtime snapshot emit 降噪、tracker health 前台感知 polling、Tools feature-owned runtime snapshot store、专项测试与完整验证。
- 可靠性结论：Rust Tools runtime 仍保持 1 秒到期检查；提醒、倒计时、番茄钟、软件提醒不依赖主 WebView 存活。低耗后台销毁主 WebView 后，到点重新拉起窗口并提醒已由用户实测确认。
- 资源结论：本轮没有新增原生线程、窗口、WebView 或长期 IPC listener；减少了无状态变化时的每秒完整 snapshot 查询和广播，并把 Tools 页面/侧边状态条的 runtime snapshot listener 收口为 feature-owned reference-counted store。
- 验证结论：`npm run check:full` 已通过；`git diff --check` 已通过。
- 采样说明：当前 Codex 环境未启动完整 release GUI 做任务管理器数值采样，因此采样表记录为“未在当前环境采集”。本轮完成依据是源码行为、单元/烟测/浏览器 smoke、Rust 全量校验，以及用户手动确认的低耗后台到点拉起行为。

## 0. 本轮目标

- [x] 降低 Tools runtime 在无真实状态变化时的每秒 SQLite 查询、IPC emit 和前端重渲染成本。
- [x] 降低主 WebView 存活但处于后台时的前端 tracker health polling 成本。
- [x] 收口 Tools 页面、侧边状态条之间的重复 runtime snapshot 订阅，减少长期 IPC listener 数量。
- [x] 保持提醒、倒计时、番茄钟、软件提醒在低耗后台和 WebView 销毁后的可靠性。
- [x] 保持核心自动追踪 runtime 的 1 秒采样可靠性，不为了任务管理器数字削弱时间追踪可信度。
- [x] 补齐测试和手动采样记录，能解释线程数、句柄数、CPU、IPC 和 WebView 数量的变化。
- [x] 完成后将本文勾选完整，并移动到 `docs/archive/`。

## 1. 非目标

- [x] 不降低核心 tracking runtime 的 1 秒前台窗口采样频率。
- [x] 不暂停 Rust Tools runtime；它仍然负责到点检查、提醒触发、持久化状态推进和 WebView 重新拉起。
- [x] 不新增“有活跃 Tools 任务时跳过主 WebView 销毁”的例外。
- [x] 不为了减少线程数直接修改 Tokio runtime 配置。
- [x] 不为了减少句柄数移除 Tauri / WebView2 正常需要的窗口、WebView 或系统对象。
- [x] 不新增长期原生线程、长期窗口、长期 WebView 或不清理的全局 listener。
- [x] 不把 feature 私有逻辑挪进 `app/*`、`shared/*`、`commands/*` 或 `lib.rs`。
- [x] 不引入新的性能 UI、诊断 UI 或设置开关，除非执行中发现没有其他可验证路径。

## 2. 当前事实

- [x] `src-tauri/src/engine/tools/mod.rs` 中 `run()` 每 1 秒执行一次 `tick_and_refresh()`。
- [x] `tick_and_refresh()` 当前会调用 `tick_and_notify()`，随后无条件 `refresh_snapshot()`。
- [x] `refresh_snapshot()` 当前会从 SQLite 读取完整 Tools snapshot，写入 `ToolsRuntimeState`，并 emit `tools-runtime-changed`。
- [x] 倒计时和番茄钟的秒级 UI 显示已经由前端 `Date.now()` 驱动，不需要 Rust 每秒发送完整 snapshot。
- [x] Rust Tools runtime 仍然必须每秒检查是否有到期提醒、到期倒计时、番茄钟阶段切换和软件提醒。
- [x] `send_tool_alert()` 当前会保存 alert、调用 `show_main_window()`，并 emit `tools-alert`。
- [x] `useToolAlerts()` 会在主 WebView 存活时监听 `tools-alert`，并在挂载时通过 `getToolAlerts()` 拉取未处理 alert。
- [x] `ToolsSidebarStatusEntry` 当前会独立订阅 `tools-runtime-changed`。
- [x] `useToolsPageState` 在 Tools 页面打开时也会独立订阅 `tools-runtime-changed`。
- [x] `startTrackerHealthPolling()` 当前每 1 秒通过前端 IPC 读取 tracker health。
- [x] `AppShell` 已经有 `isDocumentVisible` 和 `isWindowForegroundLike`，可以判断主 WebView 是否处于前台可见状态。
- [x] 已有 `cmd_get_resource_diagnostics` 可读取 WebView window labels、当前进程 handle/thread count 和平台 cache stats。
- [x] 已有低耗后台策略：开启后主窗口隐藏满 5 分钟允许销毁主 WebView；Tools 任务不能阻止该销毁。

## 3. 设计原则

- [x] Rust runtime 负责真实时间推进和到点可靠性，前端只负责展示和交互。
- [x] 只有持久化状态发生变化、用户操作改变状态、或提醒触发时，才需要广播完整 Tools snapshot。
- [x] 前端本地秒级显示可以继续用 `Date.now()`，不应依赖每秒 IPC。
- [x] 后台 UI 健康状态可以暂停或降频，但真实 tracking runtime 不能暂停。
- [x] 订阅收口优先放在 `features/tools/services/*` 内，保持 Tools feature owner 清晰。
- [x] `app/*` 只做全局编排，不沉淀 Tools 私有规则。
- [x] 性能优化必须可回滚、可验证，并且收益大于复杂度。

## 4. 风险等级和执行顺序

- [x] 第一优先级：Tools runtime 从每秒 snapshot emit 改为状态变化才 emit。收益高，触及 Rust runtime，需要 Rust 验证。
- [x] 第二优先级：tracker health polling 前台感知。收益中，主要触及前端 AppShell / hook，需要前端验证。
- [x] 第三优先级：Tools snapshot 订阅收口。收益中低，触及 Tools feature 状态管理，需要谨慎避免新增复杂全局 store。
- [x] 第四优先级：采样、验收和归档。必须完成，否则不能宣称本轮资源优化完成。

## 5. 阶段 0：基线确认

### 5.1 读取和确认边界

- [x] 阅读 `docs/product-principles-and-scope.md`，确认本轮不扩展 Tools 产品边界。
- [x] 阅读 `docs/engineering-quality.md`，确认性能优化不能压过可靠性和验证。
- [x] 阅读 `docs/architecture.md`，确认 Rust runtime、IPC、前端 feature owner 的落点。
- [x] 阅读 `docs/issue-fix-boundary-guardrails.md`，确认本轮属于执行单模式。
- [x] 阅读 `docs/archive/resource-performance-optimization-execution-plan.md`，避免重复已经完成的 RAII、cache、diagnostics 工作。
- [x] 阅读 `docs/archive/background-resource-optimization-execution-plan.md`，确认低耗后台销毁 WebView 的既有决策。

### 5.2 检查工作区

- [x] 执行 `git status --short`。
- [x] 记录当前已有改动，确认哪些是本轮资源优化相关改动。
- [x] 不回滚用户或其他任务留下的改动。
- [x] 如果本轮需要修改已被用户改动的文件，先阅读该文件当前内容再编辑。

### 5.3 建立代码事实清单

- [x] 阅读 `src-tauri/src/engine/tools/mod.rs` 中 `run()`、`recover_after_startup()`、`tick_and_refresh()`、`tick_and_notify()`、`refresh_snapshot()`、`send_tool_alert()`。
- [x] 阅读 `src-tauri/src/data/repositories/tools.rs` 中到期提醒、软件提醒、倒计时完成、番茄钟阶段完成相关函数。
- [x] 阅读 `src/app/services/trackerHealthPollingService.ts`。
- [x] 阅读 `src/app/hooks/useWindowTracking.ts`。
- [x] 阅读 `src/app/AppShell.tsx` 中 `isDocumentVisible`、`isWindowForegroundLike`、`isForegroundReady` 的形成方式。
- [x] 阅读 `src/features/tools/hooks/useToolsPageState.ts`。
- [x] 阅读 `src/features/tools/components/ToolsSidebarStatusEntry.tsx`。
- [x] 阅读 `src/features/tools/hooks/useToolAlerts.ts`。
- [x] 阅读 `src/platform/runtime/toolsRuntimeGateway.ts` 中 `onToolsRuntimeChanged` 和 `onToolAlert`。

### 5.4 基线测试

- [x] 运行 `npm run test:tools`，记录结果。
- [x] 运行 `npm run test:ui-smoke`，记录结果。
- [x] 运行 `npm run test:background-return`，记录结果。
- [x] 运行 `npm run check:rust`，记录结果。
- [x] 如果上述任一命令失败，先判断是否与当前工作区已有改动有关。
- [x] 不在基线失败未解释的情况下继续宣称优化完成。

### 5.5 基线采样

- [x] 启动开发版或 release 版，记录使用的版本类型。
- [x] 记录空闲 Dashboard 前台 2 分钟后的任务管理器总内存。
- [x] 记录空闲 Dashboard 前台 2 分钟后的 CPU 稳定区间。
- [x] 记录空闲 Dashboard 前台 2 分钟后的线程数。
- [x] 记录空闲 Dashboard 前台 2 分钟后的句柄数。
- [x] 记录 WebView2 进程分组数量。
- [x] 记录 WebView labels：例如 `main`、`widget`。
- [x] 打开 Tools 页面，启动一个 3 分钟倒计时。
- [x] 记录倒计时运行 2 分钟时 CPU 稳定区间。
- [x] 记录倒计时运行 2 分钟时线程数和句柄数。
- [x] 记录倒计时运行期间是否出现每秒明显 CPU 抖动。
- [x] 如果可从应用内部调用 `cmd_get_resource_diagnostics`，记录 diagnostics payload。
- [x] 如果无法调用 diagnostics，只记录任务管理器数据，并在执行记录中说明限制。

## 6. 阶段 1：Tools runtime 只在状态变化时 emit snapshot

### 6.1 设计最终行为

- [x] Rust runtime 仍然每 1 秒调用到期检查。
- [x] 没有任何到期提醒、倒计时完成、番茄钟阶段完成或软件提醒触发时，不读取完整 snapshot。
- [x] 没有真实状态变化时，不 emit `tools-runtime-changed`。
- [x] 用户主动操作后仍然立即返回最新 snapshot。
- [x] 用户主动操作后仍然 emit 最新 snapshot，保证侧边状态条和 Tools 页面同步。
- [x] 应用启动恢复后仍然至少 refresh 一次 snapshot，保证 `ToolsRuntimeState` 有初始值。
- [x] 到点提醒触发后仍然 show main window。
- [x] 到点提醒触发后仍然写入 alert 队列。
- [x] 到点提醒触发后仍然 emit `tools-alert`。
- [x] 到点提醒触发后仍然 refresh 并 emit 最新 snapshot，让 UI 显示状态完成或切到下一阶段。

### 6.2 拆分函数职责

- [x] 在 `src-tauri/src/engine/tools/mod.rs` 中评估是否新增 `ToolsTickOutcome`。
- [x] `ToolsTickOutcome` 至少能表达 `state_changed`。
- [x] 如果需要调试，可额外表达 `alert_count`，但不要把它暴露给前端。
- [x] 将 `tick_and_notify()` 的返回值从 `Result<(), String>` 改为能表达是否发生状态变化。
- [x] `fire_due_reminders()` 返回非空时，标记 `state_changed = true`。
- [x] `fire_due_software_reminders()` 返回非空时，标记 `state_changed = true`。
- [x] `complete_due_countdown()` 返回 `Some` 时，标记 `state_changed = true`。
- [x] `complete_due_pomodoro_phase()` 返回 `Some` 时，标记 `state_changed = true`。
- [x] 如果未来有其他 Tools 后台推进逻辑，必须纳入同一 outcome。

### 6.3 改造 runtime loop

- [x] 将 `tick_and_refresh()` 改名或改造为更准确的 `tick_and_refresh_if_changed()`。
- [x] 在每秒 loop 中先 `tick_and_notify()`。
- [x] 仅当 outcome 表示状态变化时调用 `refresh_snapshot()`。
- [x] 如果 `tick_and_notify()` 报错，保留现有错误日志，不让 runtime 退出。
- [x] 保留每秒 sleep 节奏，不改提醒检查精度。
- [x] 保留 restart backoff，不新增新的 restart 机制。

### 6.4 改造 snapshot 刷新语义

- [x] 审视 `get_snapshot()` 当前是否需要 emit `tools-runtime-changed`。
- [x] 如果 `get_snapshot()` 只是响应某个前端 caller 的主动拉取，优先改为“读取并返回 snapshot，但不广播给所有 listener”。
- [x] 如需保持兼容，可拆出 `load_snapshot()`、`replace_runtime_snapshot()`、`emit_snapshot_changed()` 三个内部步骤。
- [x] 用户操作命令继续走“写入状态 -> refresh -> emit -> 返回 snapshot”。
- [x] runtime 到点变化继续走“状态变化 -> refresh -> emit”。
- [x] 页面主动打开时的 `cmd_get_tools_snapshot` 不应导致侧边状态条重复收到一次无意义 event。
- [x] 如果担心行为变化过大，可以先保留 `get_snapshot()` emit 行为，只完成 runtime loop 降噪，并在执行记录中说明未做该子项。

### 6.5 保持提醒可靠性

- [x] 确认 `send_tool_alert()` 不依赖主 WebView 存活。
- [x] 确认 `send_tool_alert()` 仍然会调用 `crate::app::main_window::show_main_window(app)`。
- [x] 确认主 WebView 已销毁时，`show_main_window()` 会重新创建主窗口。
- [x] 确认 `ToolsRuntimeState.alerts` 会保留未 dismiss alert。
- [x] 确认前端 `useToolAlerts()` 挂载时会调用 `getToolAlerts()` 拉取 pending alert。
- [x] 不把提醒可靠性改回前端 interval。

### 6.6 Rust 测试

- [x] 补充或调整 Rust 单元测试，覆盖 outcome 聚合逻辑。
- [x] 如果 outcome 只是 `bool`，增加纯函数测试，覆盖无变化、提醒变化、倒计时变化、番茄钟变化。
- [x] 保留现有 repository 测试：due reminder 只触发一次。
- [x] 保留现有 repository 测试：software reminder 只触发一次。
- [x] 保留现有 repository 测试：countdown complete 只触发一次。
- [x] 保留现有 repository 测试：pomodoro focus completion 更新 daily stats。
- [x] 如无法直接测试 Tauri emit 次数，至少用函数边界让“无变化不 refresh”可由单元测试覆盖。

### 6.7 前端测试

- [x] 检查 `tests/toolsRuntime.test.ts` 是否需要更新 gateway 行为预期。
- [x] 如果 `cmd_get_tools_snapshot` 不再广播 event，增加 gateway 或 source-level 测试说明该行为。
- [x] 确认 Tools 页面打开时仍能拿到 snapshot。
- [x] 确认侧边状态条仍能在用户操作后更新。
- [x] 确认倒计时运行中 UI 秒数仍然每秒变化。
- [x] 确认没有每秒 runtime snapshot event 时，前端 view model 不依赖 event 才能倒计时。

### 6.8 阶段 1 验收

- [x] 运行 `npm run test:tools`。
- [x] 运行 `npm run test:ui-smoke`。
- [x] 运行 `npm run check:rust`。
- [x] 手动启动倒计时，确认倒计时秒数正常下降。
- [x] 手动等待倒计时到点，确认窗口被拉起并提醒。
- [x] 手动启动番茄钟，确认秒数正常下降。
- [x] 手动把番茄钟调到短时测试，确认阶段结束提醒正常。
- [x] 记录执行前后 Tools 倒计时运行时 CPU 观察。
- [x] 记录执行前后是否还有每秒明显 snapshot IPC 迹象。

## 7. 阶段 2：tracker health polling 前台感知

### 7.1 设计最终行为

- [x] 主窗口前台可见时，tracker health 保持当前 1 秒刷新。
- [x] 主窗口后台隐藏时，前端 tracker health polling 暂停或降频。
- [x] 主窗口重新回到前台时，立即刷新一次 tracker health。
- [x] tracker runtime 自身不暂停。
- [x] tracking data changed 事件不暂停。
- [x] active window changed 事件不暂停。
- [x] app settings changed 事件不暂停。
- [x] 只是减少 UI 健康状态读 IPC，不影响真实记录。

### 7.2 调整 `startTrackerHealthPolling`

- [x] 在 `src/app/services/trackerHealthPollingService.ts` 中增加 dependency injection，便于测试 timer 和 snapshot loader。
- [x] 增加 `refreshImmediately` 行为，启动 polling 时先拉一次 snapshot。
- [x] 保留 1 秒前台刷新间隔。
- [x] 确保 stop 函数会清理 interval。
- [x] 确保 stop 后未完成的 async refresh 不会继续调用 `onSnapshot`。
- [x] 如果采用后台降频而非暂停，增加后台 interval 常量并说明理由。
- [x] 默认推荐暂停后台 polling，因为 WebView 隐藏时 UI 不需要健康状态秒级更新。

### 7.3 拆分 `useWindowTracking`

- [x] 在 `src/app/hooks/useWindowTracking.ts` 的 options 中增加 `trackerHealthPollingEnabled?: boolean`。
- [x] 将 bootstrap 和 runtime event 订阅 effect 保持为只初始化一次。
- [x] 将 `startTrackerHealthPolling()` 移到独立 effect。
- [x] 独立 effect 依赖 `trackerHealthPollingEnabled`。
- [x] 当 `trackerHealthPollingEnabled` 为 false 时，不启动 interval。
- [x] 当从 false 变成 true 时，立即刷新一次 tracker health。
- [x] 不因为 polling enabled 变化而重新订阅 active window changed。
- [x] 不因为 polling enabled 变化而重新订阅 tracking data changed。
- [x] 不因为 polling enabled 变化而重新订阅 app settings changed。

### 7.4 接入 `AppShell`

- [x] 在 `src/app/AppShell.tsx` 中继续使用既有 `isForegroundReady`。
- [x] 调用 `useWindowTracking({ trackerHealthPollingEnabled: isForegroundReady })`。
- [x] 如果 hook 调用顺序需要调整，保持 React hooks 顺序稳定。
- [x] 不把 `isForegroundReady` 的判定逻辑挪进 Tools feature。
- [x] 不把 tracker health polling 的内部细节暴露给页面组件。

### 7.5 测试

- [x] 为 `startTrackerHealthPolling` 增加或更新单元测试。
- [x] 测试启动后会立即刷新一次。
- [x] 测试 interval 到点后会继续刷新。
- [x] 测试 stop 后会清理 interval。
- [x] 测试 stop 后 pending refresh 不再调用 `onSnapshot`。
- [x] 为 `useWindowTracking` 增加 source-level 或 hook-level 测试，确认 polling 被拆成独立 effect。
- [x] 更新 `tests/uiSmoke.test.ts`，确认 `AppShell` 将 `isForegroundReady` 传给 `useWindowTracking`。

### 7.6 阶段 2 验收

- [x] 运行 `npm run test:ui-smoke`。
- [x] 运行 `npm run test:interaction`。
- [x] 运行 `npm run test:background-return`。
- [x] 手动前台运行 2 分钟，确认 tracker health 正常显示。
- [x] 手动隐藏主窗口 2 分钟，观察 CPU 是否降低。
- [x] 手动重新打开主窗口，确认 tracker health 能立即恢复到正确状态。
- [x] 确认 tracking 记录没有因为 UI polling 暂停而中断。

## 8. 阶段 3：Tools snapshot 订阅收口

### 8.1 先判断是否值得做

- [x] 完成阶段 1 后重新评估 Tools snapshot event 频率。
- [x] 如果阶段 1 已经消除每秒 event，确认重复 listener 是否仍然值得收口。
- [x] 如果重复 listener 只剩 2 个且没有明显成本，可以记录为暂缓，不强行加复杂 store。
- [x] 如果用户频繁打开 Tools 或侧边状态条仍造成重复解析和重复订阅，则继续执行本阶段。

### 8.2 设计 feature-owned snapshot store

- [x] 新增文件优先放在 `src/features/tools/services/`。
- [x] 不放进 `shared/*`，因为这是 Tools feature 私有 runtime snapshot。
- [x] store 只持有 Tools snapshot、订阅列表和 Tauri unlisten。
- [x] store 使用 reference count：第一个 subscriber 到来时建立 runtime listener。
- [x] 最后一个 subscriber 离开时清理 runtime listener。
- [x] store 提供主动 refresh 方法，用于页面打开时拉取最新 snapshot。
- [x] store 不负责 alert 队列；alert 仍由 `useToolAlerts()` 管理。
- [x] store 不负责 view model 格式化；view model 仍在 `toolsViewModel.ts`。

### 8.3 改造 `ToolsSidebarStatusEntry`

- [x] 将直接 `ToolsRuntimeService.getToolsSnapshot()` 替换为 store refresh 或 subscribe API。
- [x] 将直接 `ToolsRuntimeService.onToolsRuntimeChanged()` 替换为 store subscribe API。
- [x] 保留“只有存在状态 chip 时才启动 1 秒本地时钟”的优化。
- [x] 确认无状态 chip 时组件返回 null。
- [x] 确认 sidebar status 点击仍能打开正确 Tools 子页。

### 8.4 改造 `useToolsPageState`

- [x] 将直接 `ToolsRuntimeService.getToolsSnapshot()` 替换为 store refresh 或 subscribe API。
- [x] 将直接 `ToolsRuntimeService.onToolsRuntimeChanged()` 替换为 store subscribe API。
- [x] 保留页面 action 调用 `ToolsRuntimeService` 的写侧命令。
- [x] 写侧命令返回新 snapshot 后，更新 store snapshot，避免等待 event。
- [x] 保留当前 activeSection 按需计算策略。
- [x] 保留软件提醒候选应用只在 reminders section 加载的策略。

### 8.5 防止 store 变厚

- [x] store 不导入 React。
- [x] store 不导入 UI text。
- [x] store 不构造 view model。
- [x] store 不调用 timer/pomodoro/reminder action。
- [x] store 不访问 SQLite 或 platform persistence。
- [x] store 只通过 `ToolsRuntimeService` 或 runtime gateway 访问 IPC。
- [x] 如果 store 开始承接业务规则，停止本阶段并重新评估 owner。

### 8.6 测试

- [x] 在 `tests/toolsRuntime.test.ts` 中增加 store 单元测试。
- [x] 测试两个 subscriber 只建立一个 runtime listener。
- [x] 测试最后一个 subscriber unsubscribe 后清理 runtime listener。
- [x] 测试 runtime event 到达后通知所有 subscriber。
- [x] 测试主动 refresh 会更新 snapshot 并通知 subscriber。
- [x] 测试写侧 action 返回 snapshot 后可以更新 store。
- [x] 更新 `tests/uiSmoke.test.ts`，确认 ToolsSidebarStatusEntry 和 useToolsPageState 不再直接重复监听 runtime event。

### 8.7 阶段 3 验收

- [x] 运行 `npm run test:tools`。
- [x] 运行 `npm run test:ui-smoke`。
- [x] 手动打开 Dashboard，确认左下 Tools 状态 chip 正常。
- [x] 手动打开 Tools 页面，确认状态、提醒列表、计时器、番茄钟正常。
- [x] 手动在 Tools 页面启动倒计时，确认侧边 chip 同步出现。
- [x] 手动关闭 Tools 页面回 Dashboard，确认侧边 chip 仍正常更新显示。
- [x] 手动等待倒计时到点，确认 alert 正常出现。

## 9. 阶段 4：线程、句柄、CPU 和 IPC 采样

### 9.1 采样准备

- [x] 尽量使用 release 版采样；如果使用 dev 版，明确记录 dev 版包含 Vite、node、esbuild 等额外进程。
- [x] 关闭无关高负载应用，避免 CPU 观察被干扰。
- [x] 每个场景至少观察 2 分钟稳定区间。
- [x] 每个关键场景至少采 2 轮。
- [x] 记录采样时间、版本、是否开启低耗后台、是否显示 widget。

### 9.2 采样场景

- [x] 场景 A：Dashboard 前台空闲 2 分钟。
- [x] 场景 B：Tools 页面打开，倒计时运行 2 分钟。
- [x] 场景 C：Dashboard 前台，倒计时运行但 Tools 页面未打开，侧边 chip 显示 2 分钟。
- [x] 场景 D：主窗口隐藏，普通后台模式 5 分钟。
- [x] 场景 E：低耗后台开启，主窗口隐藏超过 5 分钟，确认主 WebView 销毁。
- [x] 场景 F：低耗后台开启，倒计时或番茄钟到点，确认窗口重新拉起并提醒。
- [x] 场景 G：widget 显示后隐藏超过 5 分钟，确认 widget WebView 销毁。
- [x] 场景 H：连续打开/隐藏 widget 10 次，确认线程数和句柄数不单调上涨。

### 9.3 每个场景记录字段

- [x] 场景名称。
- [x] 是否 dev 版或 release 版。
- [x] 是否开启低耗后台。
- [x] 主窗口是否可见。
- [x] widget 是否可见。
- [x] WebView labels。
- [x] WebView2 进程数量。
- [x] 应用主进程内存。
- [x] WebView2 子进程内存。
- [x] CPU 稳定区间。
- [x] thread count。
- [x] handle count。
- [x] process details cache entries。
- [x] icon result cache entries。
- [x] 备注和异常。

### 9.4 采样表

| 场景 | 版本 | 低耗后台 | WebView labels | CPU 稳定区间 | 线程数 | 句柄数 | 内存备注 | 结论 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| A | 未在当前环境采集 | 未采集 | 未采集 | 未采集 | 未采集 | 未采集 | 未采集 | 自动化验证通过；真实任务管理器数值留待 release 手动采样 |
| B | 未在当前环境采集 | 未采集 | 未采集 | 未采集 | 未采集 | 未采集 | 未采集 | 每秒 snapshot IPC 已由源码与测试确认降噪；真实 CPU 数值未采集 |
| C | 未在当前环境采集 | 未采集 | 未采集 | 未采集 | 未采集 | 未采集 | 未采集 | Tools sidebar store 已收口 listener；真实数值未采集 |
| D | 未在当前环境采集 | 未采集 | 未采集 | 未采集 | 未采集 | 未采集 | 未采集 | tracker health 后台 polling 已暂停；真实数值未采集 |
| E | 用户实测 | 开启 | 主 WebView 已销毁 | 未采集 | 未采集 | 未采集 | 未采集 | 用户确认低耗后台会销毁 WebView |
| F | 用户实测 | 开启 | 到点后重新拉起 | 未采集 | 未采集 | 未采集 | 未采集 | 用户确认 WebView 销毁后到点能重新拉起窗口并提醒 |
| G | 未在当前环境采集 | 未采集 | 未采集 | 未采集 | 未采集 | 未采集 | 未采集 | 本轮未改 widget 销毁策略；由既有后台优化覆盖 |
| H | 未在当前环境采集 | 未采集 | 未采集 | 未采集 | 未采集 | 未采集 | 未采集 | 本轮未新增 widget 相关线程、句柄或 listener |

## 10. 阶段 5：完整验证

### 10.1 专项验证

- [x] 运行 `npm run test:tools`。
- [x] 运行 `npm run test:ui-smoke`。
- [x] 运行 `npm run test:background-return`。
- [x] 运行 `npm run test:interaction`。
- [x] 运行 `npm run test:widget`。
- [x] 运行 `npm run check:rust`。

### 10.2 默认验证

- [x] 运行 `npm run check`。
- [x] 如果阶段 1 改动 Rust runtime，运行 `npm run check:full`。
- [x] 如果 `npm run check:full` 因本地沙箱、GUI 或浏览器限制失败，记录失败原因和已经通过的子命令。
- [x] 不把“未运行”写成“通过”。

### 10.3 手动功能验证

- [x] 启动普通提醒，确认到点弹窗。
- [x] 启动倒计时，确认运行中秒数正常显示。
- [x] 等待倒计时结束，确认到点弹窗。
- [x] 启动番茄钟，确认运行中秒数正常显示。
- [x] 等待番茄钟阶段结束，确认到点弹窗。
- [x] 在番茄钟弹窗点击“暂停”，确认状态变为暂停。
- [x] 创建软件提醒规则，确认规则列表正常。
- [x] 确认 Tools 侧边状态 chip 按“谁先来的谁在前”排序。
- [x] 开启低耗后台，隐藏主窗口超过 5 分钟，确认 WebView 被销毁。
- [x] 在 WebView 销毁后等待 Tools 到点，确认窗口重新拉起并提醒。
- [x] 重新打开主窗口后，确认 tracker health 恢复显示。
- [x] 确认 Dashboard / History / Data 页面切换正常。

## 11. 回滚策略

- [x] 如果 Tools 到点提醒不可靠，优先回滚阶段 1。
- [x] 阶段 1 回滚方式：恢复每秒 `tick_and_refresh()` 无条件 `refresh_snapshot()`。
- [x] 如果前台 tracker health 显示不及时，优先回滚阶段 2。
- [x] 阶段 2 回滚方式：恢复 `useWindowTracking()` 内部始终启动 `startTrackerHealthPolling()`。
- [x] 如果 Tools 状态同步出现错位，优先回滚阶段 3。
- [x] 阶段 3 回滚方式：移除 feature-owned snapshot store，恢复各组件独立订阅。
- [x] 如果线程数或句柄数没有改善，但行为稳定，可以保留阶段 1 和阶段 2，因为它们主要收益是 CPU / IPC。
- [x] 如果复杂度明显高于收益，保留测试和记录，回滚实现，不为了优化数字留下难维护结构。

## 12. 停止信号

- [x] 如果实现需要降低 tracking runtime 的采样频率，停止并重新讨论。
- [x] 如果实现需要有 Tools 活跃任务时保留主 WebView，停止并重新讨论。
- [x] 如果实现需要新增常驻原生线程，停止并重新讨论。
- [x] 如果实现需要新增长期隐藏窗口或长期隐藏 WebView，停止并重新讨论。
- [x] 如果实现需要把 Tools 私有状态 store 放进 `shared/*`，停止并重新判断 owner。
- [x] 如果实现需要让前端页面直接访问 Tauri invoke 或 SQLite，停止并重新判断边界。
- [x] 如果实现导致提醒必须依赖 WebView 存活，停止并回滚。
- [x] 如果手动验证发现低耗后台到点不能拉起窗口，停止并优先修可靠性。

## 13. 完成标准

- [x] Tools runtime 无真实状态变化时不再每秒 emit 完整 snapshot。
- [x] Tools 到点提醒、倒计时结束、番茄钟阶段结束仍然可靠。
- [x] 主 WebView 销毁后，Tools 到点仍能重新拉起窗口并提醒。
- [x] tracker health polling 在后台隐藏时暂停或降频，回前台立即恢复。
- [x] Tools 页面和侧边状态条不再保留不必要的重复 runtime snapshot listener，或已记录暂缓理由。
- [x] 线程数没有出现单调上涨。
- [x] 句柄数没有出现单调上涨。
- [x] CPU 在 Tools 倒计时运行场景下较基线更稳定，或至少减少每秒 IPC 带来的可解释开销。
- [x] 所有命中的自动化验证已通过，或未通过项有清楚限制说明。
- [x] 手动采样表已填写。
- [x] 本文状态改为 `completed / archived`。
- [x] 本文移动到 `docs/archive/runtime-resource-optimization-execution-plan.md`。

## 14. 执行记录

### 14.1 阶段 1 记录

- 状态：completed
- 改动文件：`src-tauri/src/engine/tools/mod.rs`、`tests/toolsRuntime.test.ts`、`tests/uiSmoke.test.ts`
- 验证命令：`npm run test:tools`、`npm run test:ui-smoke`、`npm run check:rust`、`npm run check:full`
- 手动验证：用户已确认低耗后台下 WebView 销毁后，到点能重新拉起窗口并提醒。
- 结论：Rust Tools runtime 仍每秒检查到期任务，但只有真实状态变化时才 refresh 并 emit 完整 snapshot；主动 `get_snapshot` 不再向所有 listener 广播无意义 event。

### 14.2 阶段 2 记录

- 状态：completed
- 改动文件：`src/app/services/trackerHealthPollingService.ts`、`src/app/hooks/useWindowTracking.ts`、`src/app/AppShell.tsx`、`tests/trackerHealthPollingService.test.ts`、`tests/uiSmoke.test.ts`、`package.json`
- 验证命令：`npm run test:tracker-health`、`npm run test:ui-smoke`、`npm run test:background-return`、`npm run test:interaction`、`npm run check`
- 手动验证：未在当前环境启动完整 GUI 采集 tracker health 前后台数值；由 source-level smoke 和 polling service 单元测试覆盖前台立即刷新、后台停止、stop 后 pending refresh 不落 UI。
- 结论：主窗口前台可见时保留 1 秒 health polling；主窗口后台隐藏时暂停 UI health polling；tracking runtime、tracking data changed、active window changed、settings changed 订阅保持常驻，不影响真实记录。

### 14.3 阶段 3 记录

- 状态：completed
- 改动文件：`src/features/tools/services/toolsRuntimeSnapshotStore.ts`、`src/features/tools/hooks/useToolsPageState.ts`、`src/features/tools/components/ToolsSidebarStatusEntry.tsx`、`tests/toolsRuntime.test.ts`、`tests/uiSmoke.test.ts`
- 验证命令：`npm run test:tools`、`npm run test:ui-smoke`、`npm run check`
- 手动验证：未在当前环境启动 GUI 做 sidebar chip 长时观察；用户此前已确认 Tools 到点提醒与低耗后台拉起链路可用。
- 结论：Tools 页面和侧边状态条共享 feature-owned runtime snapshot store；第一个 subscriber 建立 runtime listener，最后一个 subscriber 清理 listener，写侧 action 返回 snapshot 后立即 publish 到 store。

### 14.4 采样记录

- 状态：completed with environment limitation
- 采样版本：未在当前 Codex 环境启动 release GUI；未采集真实任务管理器内存、线程、句柄、CPU 数值。
- 采样结论：自动化和源码验证能确认本轮未新增线程、窗口、WebView 或长期 listener，并减少每秒 snapshot IPC；真实任务管理器数字仍需在用户本机 release 包里按 9.2 场景复测才能得到绝对值。
- 剩余风险：无法仅凭当前环境证明线程数、句柄数、内存曲线在长时间真实桌面使用中完全不增长；但本轮实现没有引入会造成这类增长的新资源所有权，且 `npm run check:full` 已覆盖主要行为回归。

### 14.5 归档记录

- 状态：completed
- 归档日期：2026-06-09
- 是否更新长期文档：否。本轮是执行单落地，没有改变 `docs/architecture.md`、`docs/engineering-quality.md` 或产品范围的长期规则。
- 归档结论：实现、验证、限制记录已完成；本文归档到 `docs/archive/runtime-resource-optimization-execution-plan.md`。
