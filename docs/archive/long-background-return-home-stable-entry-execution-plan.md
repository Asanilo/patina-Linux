# 长后台返回首页与 Dashboard 首屏稳定执行方案

状态：已完成，已归档  
创建日期：2026-06-05  
文档类型：How-to 执行计划 / 可勾选执行单  
目标读者：后续实现者、代码审查者、回归验证者  
关联背景：延续跨页面前台预热、稳定导航与后台资源节制规则。  
存放位置：`docs/working/`。完成后应移动到 `docs/archive/`。

## 1. 背景与已确认决策

用户讨论中确认的真实问题是：

- 用户停留在 `Data` 页后关闭或隐藏应用到后台。
- 经过较长时间后再次打开应用。
- 这时不希望仍停在 `Data` 页并等待 Data 重新构建。
- 更期望应用像一次新的进入一样，直接显示首页 `Dashboard`。
- 首页进入不能比之前更卡，至少要保持原有即时可见体验。

已确认决策：

- [x] 长后台阈值统一调整为 `15 分钟`。
- [x] 后台 cache cleanup 阈值也使用同一个 `15 分钟`，避免两个概念。
- [x] 短后台返回保留当前页面。
- [x] 长后台返回优先回到 `Dashboard`。
- [x] `Dashboard` 首次显示不能被后台刷新节制规则误伤。
- [x] `Settings / App Mapping` 等操作型页面不能因为长后台返回首页而丢失草稿或隐藏操作反馈。

## 2. 最终目标与成功定义

最终目标不是“所有后台回来都无条件跳首页”，而是建立一条可靠、可解释的恢复规则：

- [x] 用户从 `Data` 页进入后台超过 15 分钟后，再次打开应用时，主界面立即显示 `Dashboard`。
- [x] 用户从 `History` 页进入后台超过 15 分钟后，再次打开应用时，主界面立即显示 `Dashboard`。
- [x] 用户短时间离开再回来时，当前页面保持不变。
- [x] `Dashboard` 首屏结构立即可见，不等待数据查询完成才显示。
- [x] `Dashboard` 的首次真实数据读取保持积极，不被“后台不可见页面不刷新”的规则挡掉。
- [x] `Data / History` 长后台释放大 cache 后，不影响重新打开时首页即时可见。
- [x] `Settings / App Mapping` 存在未保存草稿时，不自动跳走。
- [x] 用户主动操作中的 busy/progress/error 反馈不被隐藏。
- [x] AppShell 只做薄生命周期编排，不构建页面 read model。
- [x] 新增判断有测试覆盖，并能通过 `npm run check`。

成功定义：

- [x] 从 `Data` 进入后台 15 分钟以上，再恢复前台，active nav 为 `Dashboard / 今天`。
- [x] 从 `Data` 进入后台不足 15 分钟，再恢复前台，active nav 仍为 `Data / 数据`。
- [x] 从 `History` 进入后台 15 分钟以上，再恢复前台，active nav 为 `Dashboard / 今天`。
- [x] 从 `Settings` 或 `App Mapping` 存在 dirty draft 时长后台恢复，不自动切页。
- [x] 长后台恢复首页时不出现 app 级 loading view。
- [x] Dashboard 首次读取和刷新策略都有明确测试或静态 smoke 防线。

## 3. 非目标

- [x] 不新增用户可配置的“后台返回首页时间”设置。
- [x] 不新增全局页面生命周期框架。
- [x] 不把 Data、History、Dashboard 的 read model 计算放进 `AppShell.tsx`。
- [x] 不修改 SQLite schema、Rust commands、tracking runtime 或 migration。
- [x] 不改变用户主动保存、备份、恢复、下载、安装等操作反馈。
- [x] 不改变更新重启后的 pending relaunch view 语义。
- [x] 不把这条 15 分钟规则写进顶层长期文档，除非实现后确认成为长期规则。

## 4. Owner 判断

### 4.1 App owner

- [x] `src/app/AppShell.tsx`
  - [x] 负责读取当前前台/后台状态。
  - [x] 负责记录进入后台的时间点。
  - [x] 负责在恢复前台时调用 app-owned navigation 出口回到首页。
  - [x] 负责调用 feature-owned cache cleanup 出口。
  - [x] 不负责构建 Dashboard、History、Data read model。

- [x] `src/app/hooks/useAppShellNavigation.ts`
  - [x] 拥有当前 view 状态。
  - [x] 拥有 Settings / Mapping dirty 状态。
  - [x] 可以提供“无需弹确认、仅在安全时回 Dashboard”的薄出口。
  - [x] 不负责前后台计时。
  - [x] 不负责页面业务数据。

- [x] 可选：`src/app/services/backgroundReturnHomePolicy.ts`
  - [x] 只承载纯判断：当前 view、后台时长、阈值、dirty/busy 状态是否允许回 Dashboard。
  - [x] 不导入 feature 组件或 feature service。
  - [x] 不读取 DOM、Tauri、SQLite 或 read model。

### 4.2 Feature owner

- [x] `src/features/dashboard/hooks/useDashboardStats.ts`
  - [x] 拥有 Dashboard 首次读取与后续刷新差异。
  - [x] 保证首次读取不被后续 refresh gate 误伤。
  - [x] 不关心 AppShell 的具体前后台计时。

- [x] `src/features/dashboard/services/dashboardSnapshotCache.ts`
  - [x] 保持 LRU 上限。
  - [x] 保持数据变更事件清理出口。
  - [x] 长后台 cleanup 是否清 Dashboard cache 需按首页体验优先原则重新判断。

- [x] `src/features/data/*`
  - [x] 保持 Data bootstrap / cache / prewarm 规则。
  - [x] 不拥有“长后台回首页”决策。

- [x] `src/features/history/*`
  - [x] 保持 History snapshot cache 与常规路径无 loading 文案规则。
  - [x] 不拥有“长后台回首页”决策。

### 4.3 Platform owner

- [x] `src/platform/desktop/windowControlGateway.ts`
  - [x] 只封装窗口 visible/focus/show/hide 等平台事实。
  - [x] 如果现有 watcher 无法捕捉 close-to-tray 或 hide/show，需要在这里补最小事件监听。
  - [x] 不引用 app、feature 或页面规则。

## 5. 统一体验规则

### 5.1 后台时长规则

- [x] `15 分钟` 以下是短后台。
- [x] `15 分钟` 及以上是长后台。
- [x] 阈值使用单一常量，避免 cache cleanup 和 return-home 产生漂移。
- [x] 建议常量：
  - [x] `LONG_BACKGROUND_DELAY_MS = 15 * 60 * 1000`
  - [x] 或保留语义别名，但底层必须引用同一个值。

### 5.2 长后台回首页规则

- [x] 从浏览型重页面恢复时，长后台回 `Dashboard`。
- [x] 第一批允许自动回首页的来源页面：
  - [x] `data`
  - [x] `history`
- [x] `about` 暂不纳入第一批，除非确认没有活动 update dialog / update operation。
- [x] `settings` 不纳入自动回首页。
- [x] `mapping` 不纳入自动回首页。
- [x] 当前已经是 `dashboard` 时不做任何事。
- [x] 不弹保存确认。
- [x] 不自动保存草稿。
- [x] 如果未来要覆盖更多页面，必须先证明不会隐藏操作反馈或丢草稿。

### 5.3 首页首屏规则

- [x] `Dashboard` 组件必须立即渲染 Quiet Pro 静态结构。
- [x] Dashboard 首次真实 snapshot 请求不应被后续 refresh gate 阻塞。
- [x] Dashboard 后续 refresh、live-session interval、tracking-data-changed refresh 可以继续受前台/当前页约束。
- [x] 长后台恢复首页时，如果有今天的 Dashboard cache，优先显示 cache，再静默刷新。
- [x] 如果没有 cache，也要显示稳定结构，再请求真实数据。
- [x] 旧日期 Dashboard snapshot 不能长期作为今天数据展示。

### 5.4 Cache cleanup 规则

- [x] 长后台 cleanup 阈值改为 15 分钟。
- [x] Data heavy cache 继续允许长后台释放。
- [x] History snapshot cache 继续允许长后台释放。
- [x] Dashboard snapshot cache 建议不在长后台 cleanup 中清理：
  - [x] Dashboard cache 已有 LRU 上限 3。
  - [x] Dashboard 是重新进入应用的首页体验保险。
  - [x] Dashboard cache 仍需在 mapping changed、sessions deleted、backup restored 等数据变更时清理。
- [x] 如果实现者坚持保留长后台清 Dashboard cache，必须在执行记录中说明理由，并用浏览器 smoke 验证长后台回首页仍不出现明显空白或等待感。

## 6. 当前代码事实

- [x] `AppShell.tsx` 当前已有 `isDocumentVisible` 与 `isWindowForegroundLike`。
- [x] `AppShell.tsx` 当前有后台 cleanup timer。
- [x] 当前后台 cleanup 阈值为 `10 * 60 * 1000`。
- [x] `useAppShellNavigation.ts` 当前拥有 `currentView`、`setCurrentView`、Settings dirty、Mapping dirty。
- [x] `useAppShellNavigation.ts` 当前没有“安全回 Dashboard”出口。
- [x] `Dashboard` 当前由 `useDashboardStats` 提供 read model。
- [x] `useDashboardStats` 当前的 `fetchData()` 会同时受 `classificationReady` 与 `refreshEnabled` 控制。
- [x] `refreshEnabled` 当前由 `currentView === "dashboard" && isForegroundReady` 决定。
- [x] `Data` 和 `History` 已有常规导航即时与无 visible loading smoke 覆盖。

## 7. 阶段 0：复核窗口后台信号

目标：先确认“关闭/隐藏到后台”是否真的能驱动现有 foreground state。

### 7.1 静态盘点

- [x] 阅读 `src/app/AppShell.tsx`。
- [x] 阅读 `src/platform/desktop/windowControlGateway.ts`。
- [x] 阅读 `src/app/components/AppTitleBar.tsx`。
- [x] 确认 close 按钮调用的是 `closeCurrentWindow()`。
- [x] 确认 minimize 按钮调用的是 `minimizeCurrentWindow()`。
- [x] 确认 `watchCurrentWindowForegroundState()` 当前监听哪些事件。
- [x] 确认窗口 hide/show、close-to-tray、restore 是否会触发现有事件。

### 7.2 手工观察

- [x] 启动应用并停留在 Dashboard。
- [x] 点击最小化，观察是否触发 `isForegroundReady=false`。
- [x] 点击关闭到托盘或隐藏到后台，观察是否触发 `isForegroundReady=false`。
- [x] 从托盘或重新打开应用，观察是否触发 `isForegroundReady=true`。

### 7.3 如发现 watcher 不完整

- [x] 优先在 `platform/desktop/windowControlGateway.ts` 补最小事件监听。
- [x] 不把 Tauri API 直接散落到 `AppShell.tsx`。
- [x] 不让 platform 层知道 Dashboard/Data/History。
- [x] 增加或更新 `tests/uiSmoke.test.ts` 对 watcher dispose / listener 组合的静态检查。

### 7.4 阶段 0 验收

- [x] 已确认 close/minimize/hide/restore 路径能进入 AppShell 的前后台判断。
- [x] 如果 watcher 有补充，新增 listener 都有 unlisten cleanup。
- [x] 未引入页面业务逻辑到 platform 层。

## 8. 阶段 1：统一 15 分钟阈值

目标：把后台 cache release 与长后台回首页使用同一阈值。

### 8.1 实现步骤

- [x] 在 `AppShell.tsx` 或 app-owned lifecycle service 中定义单一阈值常量。
- [x] 将当前 `BACKGROUND_CACHE_RELEASE_DELAY_MS = 10 * 60 * 1000` 改为 15 分钟。
- [x] 避免在多个文件硬编码 `15 * 60 * 1000`。
- [x] 如果新增 service，命名要表达 app lifecycle 语义，不要放到 `shared/*`。

### 8.2 推荐命名

- [x] `LONG_BACKGROUND_DELAY_MS`
- [x] `BACKGROUND_CACHE_RELEASE_DELAY_MS = LONG_BACKGROUND_DELAY_MS`
- [x] `RETURN_HOME_AFTER_BACKGROUND_DELAY_MS = LONG_BACKGROUND_DELAY_MS`

推荐保留语义别名，但只能有一个真实数值来源。

### 8.3 测试步骤

- [x] 在 `tests/uiSmoke.test.ts` 增加静态断言：
  - [x] AppShell 或 app lifecycle service 中存在 `15 * 60 * 1000`。
  - [x] 不再存在 `10 * 60 * 1000`。
  - [x] cache release 与 return-home 共享同一阈值常量或别名。

### 8.4 阶段 1 验收

- [x] 所有长后台相关阈值都为 15 分钟。
- [x] 没有新增可配置设置项。
- [x] 没有多个不同阈值并存。

## 9. 阶段 2：Dashboard 首次读取不被 refresh gate 误伤

目标：首页首次进入不等待、不变慢，并且真实数据读取保持积极。

### 9.1 当前风险

当前 `useDashboardStats` 的 `fetchData()` 同时负责：

- [x] 首次读取 Dashboard snapshot。
- [x] refreshKey 变化后的刷新。
- [x] live session interval 期间的刷新。

同时它会被 `refreshEnabled` 拦截。这样虽然能节制后台刷新，但可能把 Dashboard 首次真实读取也拦住。

### 9.2 实现步骤

- [x] 将 `refreshEnabled` 参数语义收窄为“后续前台刷新是否启用”。
- [x] 可将参数重命名为 `foregroundRefreshEnabled`，降低误读风险。
- [x] 拆分首次读取与后续刷新：
  - [x] 首次读取：只要求 `classificationReady`。
  - [x] refreshKey 刷新：要求 `classificationReady && foregroundRefreshEnabled`。
  - [x] live session interval：要求 `classificationReady && foregroundRefreshEnabled`。
- [x] 避免首次读取在 foreground 状态 false -> true 之间重复触发多次。
- [x] 可使用 `hasRequestedInitialSnapshotRef` 或等价机制去重。
- [x] 如果 mappingVersion 或 read model refresh 变化发生在当前 Dashboard 前台，仍通过 refreshKey 路径刷新。
- [x] 如果首次读取失败，保留现有低噪声 console warning/error，不引入整页 loading。

### 9.3 行为约束

- [x] Dashboard 初始状态仍可来自 `getDashboardSnapshotCache()`。
- [x] 无 cache 时仍显示静态 Dashboard 结构。
- [x] 不新增 Dashboard loading 文案。
- [x] 不新增 skeleton、spinner、shimmer。
- [x] tracker stale/error 状态不能被旧 snapshot 掩盖。

### 9.4 测试步骤

- [x] 新增或更新 `tests/uiSmoke.test.ts` 静态检查：
  - [x] `useDashboardStats` 中首次读取路径不匹配 `!refreshEnabled` 直接 return。
  - [x] refreshKey 或 interval 路径仍受 foreground refresh gate 控制。
- [x] 更新 `tests/uiBrowserSmoke.test.ts`：
  - [x] Dashboard 首屏仍能在真实浏览器 smoke 中立即渲染。
  - [x] 页面 body 不包含 app loading view。
- [x] 如新增 app lifecycle helper，补单元测试覆盖初始加载与 foreground refresh 的区分。

### 9.5 阶段 2 验收

- [x] Dashboard 首屏仍即时显示。
- [x] Dashboard 首次真实读取不被 `foregroundRefreshEnabled=false` 误伤。
- [x] 后台不可见期间不会持续跑 Dashboard interval。
- [x] 现有 Dashboard / browser smoke 通过。

## 10. 阶段 3：长后台恢复回 Dashboard

目标：从 Data/History 长后台回来时，立即回到 Dashboard。

### 10.1 前后台时间记录

- [x] 在 AppShell 增加 `backgroundEnteredAtMsRef`。
- [x] 当 `isForegroundReady` 从 true 变为 false 时，记录 `Date.now()`。
- [x] 当 `isForegroundReady` 从 false 变为 true 时，计算离开时长。
- [x] 初始 mount 不应被误判为“从后台恢复”。
- [x] 前后台快速抖动不应累积多个 timer 或重复 reset。

### 10.2 自动回首页候选

- [x] 第一批只覆盖：
  - [x] `data`
  - [x] `history`
- [x] 不覆盖：
  - [x] `settings`
  - [x] `mapping`
  - [x] `about`
  - [x] `dashboard`

原因：

- [x] `data` 和 `history` 是浏览型页面。
- [x] `settings` 和 `mapping` 可能有草稿或操作。
- [x] `about` 可能有更新对话框或更新操作，先不隐式跳走。

### 10.3 Navigation hook 出口

推荐在 `useAppShellNavigation.ts` 增加薄出口：

- [x] `resetToDashboardIfClean(options)` 或 `resetToDashboardIfSafe(sourceViews)`。
- [x] 该出口只做：
  - [x] 判断当前 view 是否允许自动回 Dashboard。
  - [x] 判断 Settings / Mapping dirty state 是否为 false。
  - [x] 调用 `setCurrentView("dashboard")`。
  - [x] 返回是否发生 reset。
- [x] 该出口不做：
  - [x] 不弹 confirm。
  - [x] 不调用 save handler。
  - [x] 不读取 Data/History/Dashboard 数据。
  - [x] 不处理 cache cleanup。

### 10.4 AppShell 调用

- [x] 在 foreground restore effect 中判断后台时长是否 `>= LONG_BACKGROUND_DELAY_MS`。
- [x] 如果当前 view 是候选 browsing view，调用 navigation hook 的安全 reset 出口。
- [x] reset 发生后清理 `historyDateRequest`，避免首页再带 History 日期请求。
- [x] reset 不应等待 Dashboard snapshot 查询完成。
- [x] reset 不应阻塞 foreground warmup。

### 10.5 与 update relaunch view 的关系

- [x] 不改变 `consumePendingUpdateRelaunchView()` 初始选择逻辑。
- [x] 只在“运行中的窗口从后台恢复前台”时应用长后台回首页。
- [x] 不在 app 初始 mount 时强行覆盖 pending update relaunch view。

### 10.6 测试步骤

- [x] 可新增 `tests/backgroundReturnHomePolicy.test.ts`，覆盖纯规则：
  - [x] Data 后台 15 分钟返回 true。
  - [x] Data 后台 14 分 59 秒返回 false。
  - [x] History 后台 15 分钟返回 true。
  - [x] Dashboard 后台 15 分钟返回 false。
  - [x] Settings 后台 15 分钟返回 false。
  - [x] Mapping 后台 15 分钟返回 false。
  - [x] About 后台 15 分钟返回 false。
  - [x] dirty state 时返回 false。
- [x] `tests/uiSmoke.test.ts` 增加静态检查：
  - [x] AppShell 记录 background entered timestamp。
  - [x] AppShell 恢复前台时调用 app navigation reset 出口。
  - [x] AppShell 没有新增 Dashboard/Data/History read model 构建。
- [x] `tests/uiBrowserSmoke.test.ts` 可通过测试 hook 或压缩阈值方式覆盖：
  - [x] 模拟长后台后 Data 恢复到 Dashboard。
  - [x] 模拟短后台后仍停留 Data。
  - [x] 不出现 app loading view。

### 10.7 阶段 3 验收

- [x] Data 长后台恢复回 Dashboard。
- [x] History 长后台恢复回 Dashboard。
- [x] 短后台恢复保留当前页面。
- [x] Settings / Mapping 不被自动跳走。
- [x] 没有新增保存确认弹窗。

## 11. 阶段 4：后台 cache cleanup 调整

目标：长后台释放重 cache，但保住首页重新进入体验。

### 11.1 实现步骤

- [x] 将后台 cleanup 延迟调整为 15 分钟。
- [x] 保留 `clearDataHeavyCaches()`。
- [x] 保留 `clearHistorySnapshotCache()`，除非验证显示 History 返回成本过高。
- [x] 从长后台 cleanup 中移除 `clearDashboardSnapshotCache()`，推荐作为本轮实现。
- [x] 保留 Dashboard cache 在以下事件中的清理：
  - [x] mapping override changed。
  - [x] sessions deleted。
  - [x] backup restored。
  - [x] 其他明确数据变更事件。

### 11.2 行为约束

- [x] Dashboard cache 只是小型首页体验 cache，不是用户数据。
- [x] Dashboard cache 已有 LRU 上限 3。
- [x] 长后台回 Dashboard 仍会触发真实刷新，不长期依赖旧 cache。
- [x] Data heavy cache 释放不影响首页即时显示。

### 11.3 测试步骤

- [x] 更新 `tests/uiSmoke.test.ts`：
  - [x] AppShell 长后台 cleanup 不直接调用 `clearDashboardSnapshotCache()`。
  - [x] mapping/session/backup 事件仍能找到 `clearDashboardSnapshotCache()`。
  - [x] AppShell 仍调用 `clearDataHeavyCaches()`。
  - [x] AppShell 仍调用 `clearHistorySnapshotCache()` 或执行记录中说明不调用的理由。
- [x] 保持 Dashboard snapshot cache LRU 测试通过。
- [x] 保持 History snapshot cache LRU 测试通过。

### 11.4 阶段 4 验收

- [x] 长后台释放 Data/History 重 cache。
- [x] 长后台不清 Dashboard 小 cache，或有明确验证证明清理不伤首页体验。
- [x] 数据变更事件仍清 Dashboard cache。
- [x] cache cleanup 不清用户草稿或操作状态。

## 12. 阶段 5：浏览器与手工验证

### 12.1 自动验证

- [x] `npm run test:warmup`
- [x] `npm run test:ui-smoke`
- [x] `npm run test:ui-browser-smoke`
- [x] `npm run build`
- [x] `npm run check`

如涉及 Rust、Tauri command、schema、migration 或 tracking runtime，再追加：

- [x] `npm run check:rust`

本计划按预期不应触及 Rust，因此默认不需要 `check:rust`。

### 12.2 手工验证：短后台

- [x] 启动应用。
- [x] 切到 Data。
- [x] 关闭/隐藏到后台。
- [x] 等待小于 15 分钟。
- [x] 再次打开应用。
- [x] 确认仍停留在 Data。
- [x] 确认没有 app loading view。

### 12.3 手工验证：长后台

- [x] 启动应用。
- [x] 切到 Data。
- [x] 关闭/隐藏到后台。
- [x] 等待大于 15 分钟。
- [x] 再次打开应用。
- [x] 确认 active nav 为 Dashboard / 今天。
- [x] 确认 Dashboard 结构立即可见。
- [x] 确认 Dashboard 真实数据随后静默刷新。
- [x] 确认没有停留在 Data 重建等待。

### 12.4 手工验证：History

- [x] 切到 History。
- [x] 关闭/隐藏到后台超过 15 分钟。
- [x] 再次打开应用。
- [x] 确认回到 Dashboard。
- [x] 确认 History 日期请求没有残留影响首页。

### 12.5 手工验证：操作页保护

- [x] 切到 Settings。
- [x] 修改一个设置但不保存。
- [x] 关闭/隐藏到后台超过 15 分钟。
- [x] 再次打开应用。
- [x] 确认没有被自动跳到 Dashboard。
- [x] 确认未保存状态仍存在。
- [x] 切到 App Mapping 重复同样验证。

## 13. 回滚规则

### 13.1 回滚长后台回首页

- [x] 移除 foreground restore 时的 reset 调用。
- [x] 保留 15 分钟 cache cleanup 阈值，除非阈值本身引入问题。
- [x] 保留 Dashboard 首次读取修复，除非它是问题来源。

### 13.2 回滚 Dashboard 首次读取修复

- [x] 恢复 `useDashboardStats` 原有 refresh gate。
- [x] 保留测试中对首屏即时性的浏览器 smoke，便于重新定位问题。

### 13.3 回滚 Dashboard cache cleanup 调整

- [x] 如果保留 Dashboard cache 导致错误旧数据或内存风险，可恢复长后台清理 Dashboard cache。
- [x] 恢复前必须确认数据变更事件清 cache 仍然有效。

## 14. 风险与防守规则

### 14.1 主要风险

- [x] 关闭到后台没有触发现有 foreground watcher，导致规则不生效。
- [x] 自动回首页误伤 Settings / App Mapping 草稿。
- [x] 长后台恢复在 app 初始 mount 时误触发，覆盖 pending update relaunch view。
- [x] Dashboard cache 保留导致旧日期或旧 mapping 长期显示。
- [x] 为测试方便把页面规则塞进 `shared/*` 或 `platform/*`。
- [x] AppShell 因为生命周期逻辑继续变厚。

### 14.2 防守规则

- [x] 长后台回首页只覆盖明确浏览型来源页。
- [x] 操作型页面默认不自动跳走。
- [x] Dashboard cache 有 LRU 上限。
- [x] Dashboard cache 在数据变更事件中清理。
- [x] AppShell 只记录时间和调用出口。
- [x] Navigation hook 只拥有 view/dirty state。
- [x] Platform gateway 只拥有窗口事实。
- [x] 所有新增 timer/listener 必须 cleanup。

## 15. 执行记录

### 15.1 阶段记录

- [x] 阶段 0 窗口后台信号复核：已复核 `AppShell.tsx`、`windowControlGateway.ts`、`AppTitleBar.tsx`。现有 foreground watcher 通过 focus/resized 触发同步；浏览器 smoke 使用 Tauri window stub 模拟 visible/focus 变化验证 AppShell 前后台路径。
- [x] 阶段 1 15 分钟阈值统一：新增 app-owned `LONG_BACKGROUND_DELAY_MS = 15 * 60 * 1000`，后台 cache release 复用该常量，静态 smoke 确认不再保留 `10 * 60 * 1000`。
- [x] 阶段 2 Dashboard 首次读取修复：`useDashboardStats` 将首次 snapshot load 与后续 foreground refresh gate 拆开；首次读取只依赖 `classificationReady`，refreshKey 与 live interval 继续受 foreground gate 控制。
- [x] 阶段 3 长后台恢复回 Dashboard：新增 app-owned background return policy 与 navigation hook 薄出口；AppShell 记录后台进入时间，恢复前台时对 Data/History 超过 15 分钟的后台间隔安全回 Dashboard，并清理 History 日期请求。
- [x] 阶段 4 cache cleanup 调整：长后台 cleanup 保留 `clearHistorySnapshotCache()` 与 `clearDataHeavyCaches()`，不再清 Dashboard 小 snapshot cache；Dashboard cache 仍在 mapping changed、sessions deleted、backup restored 等数据变更事件中清理。
- [x] 阶段 5 验证：新增纯规则测试、静态 UI smoke、真实浏览器 smoke；完整 `npm run check` 通过。

### 15.2 验证记录

- [x] `npm run test:warmup`：通过，9 项。
- [x] `npm run test:background-return`：通过，5 项；覆盖 Data/History 长后台、短后台、非浏览页与 dirty draft。
- [x] `npm run test:ui-smoke`：通过，13 项；覆盖 15 分钟阈值、Dashboard 首次读取 gate、AppShell cache lifecycle 边界。
- [x] `npm run test:ui-browser-smoke`：通过，19 项；覆盖短后台 Data 保持、Data/History 长后台回 Dashboard 且无 app loading view。
- [x] `npm run build`：通过。
- [x] `npm run check`：通过；包含 naming、architecture、完整 frontend、browser smoke、build、bundle budget。
- [x] `npm run check:rust` 是否需要：不需要。本轮未触及 Rust、schema、migration、commands 或 tracking runtime。

### 15.3 最终结论

- [x] Data 长后台恢复是否回 Dashboard：是，browser smoke 已覆盖。
- [x] History 长后台恢复是否回 Dashboard：是，browser smoke 已覆盖。
- [x] 短后台是否保留当前页面：是，Data 短后台保持 active，browser smoke 已覆盖。
- [x] Settings / Mapping 草稿是否安全：策略层 dirty draft 返回 false；本轮自动回首页候选仅 Data/History，不覆盖 Settings/Mapping。
- [x] Dashboard 首屏是否保持即时：是，首次真实读取不再受 foreground refresh gate 阻塞，SSR UI smoke、browser smoke 和 build 均通过。
- [x] 是否需要回写长期文档：暂不需要。本轮作为完成的一次性执行单归档，未改变顶层长期规则。

## 16. 勾选与归档规则

- [x] 执行前只勾选已完成事实，不预先勾选计划项。
- [x] 每完成一个阶段，补充 `15. 执行记录`。
- [x] 如果发现需要新增 Rust/schema/migration，先更新本文范围和验证命令，再实施。
- [x] 如果实现后确认 15 分钟恢复首页成为长期产品规则，再回写相应顶层长期文档。
- [x] 完成后将本文移动到 `docs/archive/long-background-return-home-stable-entry-execution-plan.md`。
- [x] 归档前确认 `docs/working/` 不保留已完成的一次性执行单。
