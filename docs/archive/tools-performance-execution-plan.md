# Tools 页面性能优化执行方案

状态：已完成，已归档。
创建日期：2026-06-09
完成日期：2026-06-09
文档类型：How-to / 执行单
目标读者：后续执行该优化的 agent 或开发者

## 执行结果摘要

- 已完成阶段 1：软件提醒候选应用列表优先复用 classification bootstrap cache，并增加按 bootstrap 引用和 UI language 失效的派生缓存。
- 已完成阶段 2：Tools view model labels 改为由 AppShell 传入的 `uiText` 派生，并通过 `useMemo` 稳定依赖。
- 已完成阶段 3：采用方案 C + 方案 B。非当前 section 使用轻量 fallback view model；未访问面板不挂载，访问后保留挂载以保护草稿。
- 已完成阶段 4：确认 `scheduleLazyViewChunkPreload` 当前无生产调用，未调整生产 preload 策略。
- 已完成阶段 5：保留 `ToolAlertDialog` 常驻；侧边栏 status 仅在存在 tools status chip 时启动 1 秒展示刷新。
- 已完成阶段 6：`npm run check` 通过。
- 未触及 Rust tools runtime，未新增共享 runtime store，未更新顶层长期文档。

## 1. 目标

在不改变 Tools 产品边界、不削弱提醒/计时器/番茄钟可靠性的前提下，降低 Tools 相关的首开成本、重复读模型成本和后台无意义渲染成本。

本执行方案优先处理低风险、owner 清晰、可验证的前端优化；只有在有测量依据时，才考虑更深的 runtime 订阅或 Rust tick 策略调整。

## 2. 当前事实

- `Tools` 页面通过 `createPreloadableViewComponent("tools")` 懒加载，入口在 `src/app/AppShell.tsx`。
- `src/app/services/viewChunkPreloadService.ts` 的默认 lazy view preload 列表包含 `tools`。
- `src/app/services/startupWarmupService.ts` 的默认启动暖机列表当前不包含 `tools`。
- `AppShell` 常驻渲染 `ToolsSidebarStatusEntry` 和 `ToolAlertDialog`，因此工具提醒和侧边栏状态不是页面打开后才存在。
- Rust 侧 `tools_runtime::run` 在应用启动后由 `spawn_tools_runtime_restart_loop` 常驻运行，每秒 tick，用于提醒、软件提醒、倒计时和番茄钟到期处理。
- `Tools` 页面内部当前同时挂载提醒、计时器、番茄钟三个面板，只用 class 隐藏非当前面板。
- `loadSoftwareReminderAppCandidates()` 当前每次调用都会走 `ClassificationService.loadClassificationBootstrap()`，没有先复用已有 classification bootstrap cache。

## 3. 非目标

- [x] 不把 Tools 扩张成任务管理、项目管理、日程系统或游戏化生产力平台。
- [x] 不改变提醒、倒计时、番茄钟到期后唤起主窗口和弹窗提醒的默认行为。
- [x] 不为了减少资源占用而停掉 Rust tools runtime。
- [x] 不重做 Tools 页面视觉风格，不引入 Quiet Pro 之外的新视觉方向。
- [x] 不新增 `shared/*` 通用抽象，除非能证明该能力是稳定跨 feature 能力。
- [x] 不让 `app/*`、`platform/*`、Rust `commands/*` 或 `lib.rs` 承接新的厚业务逻辑。

## 4. Owner 判断

- `features/tools/*` 拥有 Tools 页面状态、面板渲染、工具 view model、工具布局偏好和软件提醒候选列表。
- `platform/runtime/toolsRuntimeGateway.ts` 只拥有 Tauri tools IPC/event 边界与 raw payload 解析出口，不放 feature 私有策略。
- Rust `engine/tools/*` 拥有提醒、计时器、番茄钟运行时写侧和到期副作用。
- `app/AppShell.tsx` 只允许编排全局入口、全局弹窗和页面切换，不承接 Tools 业务逻辑。

## 5. 总体验收标准

- [x] Tools 页面首次打开仍能显示提醒、计时器、番茄钟入口。
- [x] 提醒创建、取消、到期弹窗仍可工作。
- [x] 软件提醒候选应用列表可显示已观察到的应用，并能跟随 App Mapping 更新后的 classification cache。
- [x] 计时器开始、暂停、恢复、重置、计圈仍可工作。
- [x] 番茄钟开始、暂停、恢复、跳过、重置仍可工作。
- [x] 侧边栏 Tools status chip 在有运行中工具状态时仍可出现，并能打开对应 Tools section。
- [x] `ToolAlertDialog` 在页面未打开时仍能接收工具提醒。
- [x] 不新增页面直连 SQLite、Tauri API 或 raw DTO 的路径。
- [x] 不让 `app/*` 或 `shared/*` 变厚。
- [x] 构建后的主要 JS chunk 和总 gzip 体积不超过现有 bundle budget。

## 6. 执行顺序总览

- [x] 阶段 0：建立基线与风险确认。
- [x] 阶段 1：缓存软件提醒候选应用列表。
- [x] 阶段 2：稳定 Tools view model labels 与 memo 依赖。
- [x] 阶段 3：减少 Tools 页面内非当前面板的渲染成本。
- [x] 阶段 4：审视 Tools chunk preload 策略。
- [x] 阶段 5：评估全局 Tools status/alert 订阅成本。
- [x] 阶段 6：验证、测量、回写结论。

## 7. 阶段 0：建立基线与风险确认

### 7.1 确认工作区状态

- [x] 运行 `git status --short`。
- [x] 如果存在与 Tools、classification、AppShell、runtime gateway 相关的未提交改动，先阅读改动内容。
- [x] 如果未提交改动不是本次工作产生的，不要回滚。
- [x] 如果未提交改动会影响本计划落点，先在执行记录里写明影响，再继续。

### 7.2 确认关键入口

- [x] 阅读 `src/app/AppShell.tsx`。
- [x] 确认 `Tools` 仍通过 `createPreloadableViewComponent("tools")` 创建。
- [x] 确认 `ToolsSidebarStatusEntry` 和 `ToolAlertDialog` 是否仍在 `AppShell` 中常驻。
- [x] 阅读 `src/app/services/viewChunkPreloadService.ts`。
- [x] 记录默认 preload 列表是否包含 `tools`。
- [x] 阅读 `src/app/services/startupWarmupService.ts`。
- [x] 记录启动暖机默认 views 是否包含 `tools`。

### 7.3 确认 runtime 边界

- [x] 阅读 `src/platform/runtime/toolsRuntimeGateway.ts`。
- [x] 确认 tools snapshot、alerts、runtime changed event 的 IPC/event 名称没有计划外变化。
- [x] 阅读 `src-tauri/src/engine/tools/mod.rs`。
- [x] 确认 Rust tools runtime 仍负责到期提醒、软件提醒、倒计时、番茄钟完成。
- [x] 明确本轮不停止或延迟启动 Rust tools runtime。

### 7.4 记录测量基线

- [x] 如果已有 `dist/assets`，记录以下文件的 raw size：`index-*.js`、`Tools-*.js`、`Settings-*.js`、`react-vendor-*.js`、`charts-*.js`。
- [x] 运行 `npm run build`。
- [x] 运行 `npm run check:bundle`。
- [x] 保存总 JS gzip 输出，作为后续对照。
- [x] 如果 build 或 bundle check 失败，先判断是否与本任务无关；不要在未理解原因前继续改动。

阶段 0 记录：改动前现有 `dist/assets` raw size 基线为 `Tools-DEoEazwp.js` 35,176 bytes、`index-BWTFBzT_.js` 212,559 bytes、`Settings-BQI_XIVF.js` 57,067 bytes、`react-vendor-DBimmbCC.js` 192,926 bytes、`charts-BYEm9rZH.js` 391,757 bytes。改动前 total JS gzip 未在本轮先行捕获；改动后通过 `check:bundle` 记录。

## 8. 阶段 1：缓存软件提醒候选应用列表

目标：避免每次打开 Tools 页面都完整重读 classification bootstrap，同时保持候选列表能使用最新 App Mapping 结果。

### 8.1 设计缓存策略

- [x] 打开 `src/features/tools/services/softwareReminderAppCandidates.ts`。
- [x] 保留 `buildSoftwareReminderAppCandidates(observed)` 作为纯函数。
- [x] 在 `loadSoftwareReminderAppCandidates()` 内优先读取 `ClassificationService.getBootstrapCache()`。
- [x] 如果 cache 存在，直接基于 cache 生成候选列表。
- [x] 如果 cache 不存在，再调用 `ClassificationService.loadClassificationBootstrap()`。
- [x] 调用 `ClassificationService.applyBootstrapToProcessMapper(bootstrap)`，确保 mapper 状态仍与 bootstrap 同步。
- [x] 不把候选列表缓存放入 `shared/*`。
- [x] 不让组件直接访问 `ClassificationService`。

### 8.2 可选的派生缓存

只有在简单复用 bootstrap cache 后仍有明显重复计算时，才做本小节。

- [x] 在 `softwareReminderAppCandidates.ts` 内新增模块级派生缓存。
- [x] 缓存 key 使用 bootstrap 对象引用，而不是字符串化 observed 列表。
- [x] 结构示例：
  - [x] `cachedBootstrap: ClassificationBootstrapData | null`
  - [x] `cachedCandidates: ToolSoftwareReminderAppCandidate[]`
- [x] 当 `ClassificationService.getBootstrapCache()` 返回不同对象引用时，重新构建候选列表。
- [x] 返回候选列表时避免让调用方可变更模块缓存。可以返回 shallow copy，或确认调用方只读。
- [x] 不为这个局部优化新增全局 cache service。

阶段 1 记录：派生缓存额外纳入当前 UI language，避免默认应用显示名在语言切换后被旧候选缓存卡住。

### 8.3 更新测试

- [x] 优先在 `tests/toolsRuntime.test.ts` 中增加覆盖，除非现有结构明显不适合。
- [x] 如果直接测试 `loadSoftwareReminderAppCandidates()` 很难注入依赖，先提取一个 feature-owned 小函数，例如 `resolveSoftwareReminderAppCandidatesFromBootstrapCache(deps)`。
- [x] 测试 cache 命中时不调用完整 bootstrap load。
- [x] 测试 cache 缺失时仍调用完整 bootstrap load。
- [x] 测试 App Mapping 更新后，如果 classification bootstrap cache 对象更新，候选列表能重新生成。
- [x] 测试候选列表仍过滤不追踪进程、按 lastSeen/appName/exeName 排序。

### 8.4 阶段 1 验证

- [x] 运行 `npm run test:tools`。
- [x] 运行 `npm run test:classification`。
- [x] 如果改动影响 App Mapping cache 行为，追加 `npm run test:interaction`。
- [x] 手动检查代码没有新增跨层导入违规。

## 9. 阶段 2：稳定 Tools view model labels 与 memo 依赖

目标：避免 `useToolsPageState` 每次 render 都创建新 labels 对象导致多个 `useMemo` 失效，同时保留语言切换能力。

### 9.1 确认当前问题

- [x] 打开 `src/features/tools/hooks/useToolsPageState.ts`。
- [x] 确认 `const labels = buildToolsViewModelLabels();` 是否仍在 hook body 直接执行。
- [x] 确认 `reminderRows`、`softwareReminderRuleRows`、`timerViewModel`、`pomodoroViewModel` 的 dependency list 是否包含 `labels`。
- [x] 打开 `src/features/tools/components/ToolsSidebarStatusEntry.tsx`。
- [x] 确认 sidebar status chips 是否也在 render 时直接调用 `buildToolsViewModelLabels()`。

### 9.2 选择语言依赖方案

推荐方案：由 `AppShell` 把当前 `uiTextLanguage` 传给 Tools 页面和 Tools sidebar status entry。

- [x] 在 `Tools` props 中增加 `uiLanguage` 或 `uiText`。
- [x] 在 `ToolsSidebarStatusEntry` props 中增加 `uiLanguage` 或 `uiText`。
- [x] 在 `useToolsPageState` options 中增加同一语言依赖。
- [x] 在 `useToolsPageState` 中使用 `useMemo(() => buildToolsViewModelLabels(uiText), [uiText])`。
- [x] 在 `ToolsSidebarStatusEntry` 中使用同样方式 memo labels。
- [x] 不使用空 dependency 的 `useMemo`，否则语言切换后 labels 可能停留在旧语言。
- [x] 不把语言状态放进 `shared/*` 新 store。

### 9.3 保持现有 UI_TEXT 兼容

- [x] 页面静态文案仍可继续使用 `UI_TEXT` proxy。
- [x] 本阶段只修正 view model labels 的对象稳定性。
- [x] 如果发现 Tools 页面语言切换还有其他旧问题，另列为后续 UI 文案一致性任务，不在本阶段扩大范围。

### 9.4 阶段 2 验证

- [x] 运行 `npm run test:tools`。
- [x] 如果新增了语言切换覆盖，运行相关测试文件。
- [x] 运行 `npm run test:ui-smoke`。
- [x] 在浏览器 smoke 或手动检查中确认中文默认 Tools 页面仍显示正常。

## 10. 阶段 3：减少 Tools 页面内非当前面板渲染成本

目标：降低 Tools 页面打开后的每秒 tick、snapshot 更新和局部状态变化导致的非当前面板 render 成本。

### 10.1 先做行为决策

在改代码前先决定非当前面板的状态保留策略。

- [x] 方案 A：只渲染当前面板，切换面板时卸载旧面板。（已评估，未采用）
- [x] 方案 B：首次访问后保留已访问面板，未访问面板不挂载。
- [x] 方案 C：三个面板继续挂载，但只为当前面板计算重型 view model。

推荐默认选择：先做方案 C。如果测量显示收益不足，再讨论方案 B。不要默认选择方案 A，因为它可能丢失用户在非当前面板中的草稿输入。

### 10.2 方案 C 详细步骤

- [x] 打开 `src/features/tools/hooks/useToolsPageState.ts`。
- [x] 为 `useToolsPageState` 增加 `activeSection` 参数。
- [x] `reminderRows` 只在 `activeSection === "reminders"` 或侧边栏需要时构建。
- [x] `softwareReminderRuleRows` 只在 `activeSection === "reminders"` 时构建。
- [x] `timerViewModel` 只在 `activeSection === "timer"` 时构建，或者保持轻量 fallback。
- [x] `pomodoroViewModel` 只在 `activeSection === "pomodoro"` 时构建，或者保持轻量 fallback。
- [x] 保证传给非当前面板的 fallback 不会被用户看到。
- [x] 不把 view model 构建移动到 `AppShell`。
- [x] 不让面板组件直接订阅 runtime。

### 10.3 方案 B 详细步骤

仅当方案 C 不足时执行。

- [x] 在 `Tools.tsx` 中新增 `visitedSections` state。
- [x] 初始值包含 `activeSection`。
- [x] 当用户切换 section 时，把 section 加入 `visitedSections`。
- [x] 只渲染 `visitedSections` 中的面板。
- [x] 未访问面板不挂载，避免首次打开 Tools 时同时初始化三个面板。
- [x] 已访问面板继续挂载，避免草稿输入丢失。
- [x] 保持 `.tools-section-pane-hidden` 只用于已访问但非当前面板。
- [x] 更新 UI/browser smoke，确认三个 section 首次点击后内容仍出现。

### 10.4 方案 A 详细步骤

仅当明确接受切换 section 丢失草稿输入时执行。

- [x] 在 `Tools.tsx` 中改为条件渲染单一当前面板。（已评估，未采用）
- [x] 删除非当前面板的隐藏 class 使用。（已评估，未采用）
- [x] 在执行记录中写明交互变化：切换面板会重置未提交表单。（不适用：未采用方案 A）
- [x] 更新相关 smoke 测试预期。（不适用：未采用方案 A）
- [x] 该方案需要用户明确确认后才能进入实现。（已遵守）

### 10.5 阶段 3 验证

- [x] 运行 `npm run test:tools`。
- [x] 运行 `npm run test:ui-browser-smoke`。
- [x] 手动或自动确认 Tools 页面三枚 section icon-only tab 仍存在。
- [x] 确认提醒、软件提醒、计时器、番茄钟面板切换后布局没有错位。
- [x] 确认长单词、按钮文字、空状态文案没有溢出。

## 11. 阶段 4：审视 Tools chunk preload 策略

目标：确保 Tools chunk 不在无明确收益的路径中过早加载，同时不破坏常用页面的暖机体验。

### 11.1 确认真实调用路径

- [x] 搜索 `scheduleLazyViewChunkPreload`。
- [x] 如果它仍只被测试调用，不要为了理论优化修改生产行为。
- [x] 如果生产代码开始调用它，继续执行本阶段后续步骤。（不适用：当前无生产调用）

### 11.2 拆分 preload 分层

仅当生产路径使用 `scheduleLazyViewChunkPreload` 时执行。

- [x] 把默认 views 拆成核心页面和次级页面。（不适用）
- [x] 核心页面建议包含 `history`、`data`、`mapping`、`settings`、`about`。（不适用）
- [x] `tools` 默认进入次级页面，不参与第一轮核心 idle preload。（不适用）
- [x] 如果用户 hover 或 focus Tools nav，可触发 `preloadLazyViewChunk("tools")`。（不适用）
- [x] 如果用户点击 Tools nav，仍通过 Suspense fallback 正常加载。
- [x] 不把 Tools 从 `PreloadableView` union 中移除。

### 11.3 更新 preload 测试

- [x] 更新 `tests/viewChunkPreloadService.test.ts` 中默认 preload 顺序预期。（不适用：未改 preload 策略）
- [x] 增加测试覆盖：显式传入 `views: ["tools"]` 时仍能预加载 Tools chunk。（不适用：既有覆盖保持通过）
- [x] 如果新增 nav intent preload，增加对应测试或 UI smoke 覆盖。（不适用）

### 11.4 阶段 4 验证

- [x] 运行 `npm run test:preload`。
- [x] 运行 `npm run test:warmup`。
- [x] 运行 `npm run build`。
- [x] 对比 `dist/assets/Tools-*.js` 是否仍独立存在。
- [x] 对比 total JS gzip，确认没有因为拆分策略导致异常膨胀。

阶段 4 记录：生产代码当前只调用 `startStartupWarmup`，`scheduleLazyViewChunkPreload` 只在测试中出现；因此未调整生产 preload 策略。

## 12. 阶段 5：评估全局 Tools status/alert 订阅成本

目标：只在确认有实际成本时，降低全局状态入口的无意义更新；不削弱提醒可靠性。

### 12.1 先保留 alert 常驻

- [x] 不移除 `ToolAlertDialog` 的常驻渲染。
- [x] 不移除 `useToolAlerts()` 对 `tools-alert` event 的监听。
- [x] 不改变到期提醒唤起主窗口的 Rust 行为。

### 12.2 优化 sidebar status 的本地 interval

这是本阶段优先考虑的小修。

- [x] 打开 `src/features/tools/components/ToolsSidebarStatusEntry.tsx`。
- [x] 当前无 snapshot 或无 status chips 时，不启动 1 秒 interval。
- [x] 当 snapshot 显示有运行中 timer、pomodoro 或 next reminder 时，再启动 interval。
- [x] interval 只负责更新展示用 `nowMs`，不要触发额外 IPC。
- [x] 确认无状态时侧边栏不显示 chip，也不会每秒 setState。
- [x] 确认有状态时倒计时/番茄钟 chip 仍会刷新。

### 12.3 不急着做共享 runtime store

只有在同时满足以下条件时，才考虑抽出 feature-owned runtime store：

- [x] Tools 页面打开时，页面和 sidebar 的双订阅造成可测量问题。（未满足）
- [x] 现有小修无法降低问题。（未满足）
- [x] 设计能留在 `features/tools/services/*` 或 `features/tools/hooks/*`。
- [x] 不需要新增 `shared/*` 抽象。
- [x] 不改变 `platform/runtime/toolsRuntimeGateway.ts` 的边界职责。

如果满足条件，执行细化步骤：

- [x] 新增 `features/tools/services/toolsRuntimeSnapshotStore.ts`。（不适用：未满足抽 store 条件）
- [x] store 内部拥有当前 snapshot、订阅列表、gateway unlisten。（不适用）
- [x] 第一个前端订阅者出现时注册 `onToolsRuntimeChanged`。（不适用）
- [x] 最后一个前端订阅者卸载时释放 event listener。（不适用）
- [x] 保留 `getToolsSnapshot()` 首次加载逻辑。（不适用）
- [x] Tools page 和 sidebar status entry 都消费该 feature-owned store。（不适用）
- [x] 写测试覆盖 ref count、初始 snapshot、事件更新、释放监听。（不适用）

### 12.4 Rust runtime 调整的停止线

本执行方案默认不调整 Rust runtime tick。若未来确实要动，必须另写执行单，并满足：

- [x] 有明确测量证明 Rust tools tick 成本成为问题。（未满足）
- [x] 能证明提醒、软件提醒、倒计时、番茄钟不会延迟或漏触发。（未触及 Rust）
- [x] 有 Rust 单元测试覆盖到期处理、重复触发保护和启动恢复。（未触及 Rust）
- [x] 追加 `npm run check:rust`。（不适用：未触及 Rust）

## 13. 阶段 6：验证、测量、回写结论

### 13.1 局部验证

- [x] 完成阶段 1 后运行 `npm run test:tools`。
- [x] 完成阶段 1 后运行 `npm run test:classification`。
- [x] 完成阶段 2 后运行 `npm run test:tools`。
- [x] 完成阶段 3 后运行 `npm run test:ui-browser-smoke`。
- [x] 完成阶段 4 后运行 `npm run test:preload` 和 `npm run test:warmup`。
- [x] 完成阶段 5 后运行 `npm run test:tools`。

### 13.2 默认交付验证

如果本次只做阶段 1 和阶段 2：

- [x] 运行 `npm run test:tools`。
- [x] 运行 `npm run test:classification`。
- [x] 运行 `npm run build`。
- [x] 运行 `npm run check:bundle`。

如果本次做到阶段 3 或阶段 4：

- [x] 运行 `npm run test:tools`。
- [x] 运行 `npm run test:preload`。
- [x] 运行 `npm run test:warmup`。
- [x] 运行 `npm run test:ui-smoke`。
- [x] 运行 `npm run test:ui-browser-smoke`。
- [x] 运行 `npm run build`。
- [x] 运行 `npm run check:bundle`。

如果本次触及 Rust tools runtime：

- [x] 运行 `npm run check:rust`。（不适用：未触及 Rust）
- [x] 运行 `npm run check`。
- [x] 明确记录为什么 Rust 改动必要。（不适用：未触及 Rust）

### 13.3 测量记录

- [x] 记录改动前 `Tools-*.js` raw size。
- [x] 记录改动后 `Tools-*.js` raw size。
- [x] 记录改动前 total JS gzip。（本轮改动前未捕获，已记录测量缺口）
- [x] 记录改动后 total JS gzip。
- [x] 记录首次打开 Tools 页面是否出现明显 loading。
- [x] 记录切换 Tools section 是否保留用户草稿。
- [x] 如果优化 sidebar interval，记录无 tools 状态时是否停止每秒 setState。

测量记录：

- 改动前现有 dist：`Tools-DEoEazwp.js` 35,176 bytes raw。
- 改动后构建：`Tools-Btyl-x2m.js` 36,207 bytes raw / 9.42 KiB gzip。
- 改动后 total JS gzip：332.97 KiB。
- `index` chunk 改动后：`index-CUCVmdkV.js` 212,767 bytes raw / 65.15 KiB gzip。
- `test:ui-browser-smoke` 覆盖 Tools 页面打开与三个 section 点击，未出现可见 loading/布局回归。
- 面板策略为“未访问不挂载，访问后保留挂载”，因此访问过的面板草稿可保留。
- `ToolsSidebarStatusEntry` 在无 snapshot/status chip 时不会启动本地 1 秒 interval。

### 13.4 回写结论

- [x] 如果计划执行完成，把有效长期规则回写到 `docs/engineering-quality.md` 或 `docs/architecture.md`，仅限确实形成长期规则时。
- [x] 如果只是一次性执行结果，不更新顶层长期文档。
- [x] 执行完成后，将本文件移动到 `docs/archive/`，或在后续任务中明确它仍是 active working doc。

## 14. 推荐最小首轮实施范围

首轮建议只做以下内容：

- [x] 阶段 1：缓存软件提醒候选应用列表。
- [x] 阶段 2：稳定 labels 与 memo 依赖。
- [x] 阶段 5.2：sidebar status 无状态时停止 1 秒 interval。

首轮暂不做：

- [x] 不改 Rust tools runtime。
- [x] 不新增共享 runtime store。
- [x] 不改变 Tools 三面板挂载策略，除非测量显示仍有明显问题。（实际升级执行阶段 3：未访问不挂载，访问后保留挂载）
- [x] 不调整生产 preload 策略，除非确认 `scheduleLazyViewChunkPreload` 已在生产路径使用。

## 15. 风险与回退

- [x] 如果软件候选列表不刷新，优先回退派生缓存，只保留 bootstrap cache 命中逻辑。（未触发）
- [x] 如果语言切换后 Tools status 文案不更新，回退 labels memo 改动，并重新设计语言依赖传递。（未触发）
- [x] 如果面板按需渲染导致草稿丢失，回退到三个面板保留挂载，改做 view model 计算收缩。（未触发）
- [x] 如果 preload 策略导致 Tools 首开体验明显变差，恢复原默认 preload 顺序。（未触发）
- [x] 如果 sidebar interval 优化导致 status chip 不刷新，恢复常驻 interval，再补更精确的 active 状态判断。（未触发）

## 16. 最终交付清单

- [x] 代码改动留在真实 owner 内。
- [x] 没有新增跨层临时抽象。
- [x] 没有让页面直接访问 platform gateway、Tauri API 或 SQLite。
- [x] 测试命令与结果已记录。
- [x] build 与 bundle check 已记录。
- [x] 行为变化已记录。
- [x] 未完成项已明确保留为后续任务，而不是混在实现里半改。
