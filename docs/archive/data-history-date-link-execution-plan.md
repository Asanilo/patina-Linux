# Data 到 History 日期联动执行计划

## 1. 文档定位

本文是一份临时执行计划，用于实现：

> 在 `数据` 页双击活动热力图或活动趋势中的某一天后，跳转到这一天的 `历史` 页查看详情。

本文只作为本轮功能实施依据。功能完成并验证后，应根据实际结果决定是否归档到 `docs/archive/`，不应长期留在 top-level `docs/` 作为母规则。

## 2. 背景与目标

当前 `数据` 页提供长期趋势、活动热力图和应用趋势，适合发现“哪一天值得看”。`历史` 页提供单日时间线、应用分布和标题详情，适合查看“这一天具体发生了什么”。

两者目前缺少日期上下文联动，用户需要先在 `数据` 页发现某一天，再手动切到 `历史` 页并重新选择日期。这个流程会打断回看路径。

本次目标是让 `数据` 页成为 `历史` 页的上游入口：

- 用户在 `数据` 页发现某一天。
- 用户双击对应日期目标。
- 应用切换到 `历史` 页。
- `历史` 页自动选中该日期并加载详情。

## 3. 范围

### 3.1 本次包含

- [x] 支持在 `数据 > 活动热力图` 中打开某一天的 `历史`。
- [x] 支持在 `数据 > 活动趋势` 的日粒度范围中打开某一天的 `历史`。
- [x] 支持在 `数据 > 应用趋势` 的日粒度图表中打开某一天的 `历史`。
- [x] 保留现有 `History` 日期选择、前后一天切换、缓存和加载逻辑。
- [x] 复用当前未保存变更确认逻辑，避免从 `设置` 或 `应用` 脏状态跳转时绕过保护。
- [x] 不新增可见 UI 文案，保留现有日期与时长信息。
- [x] 增加测试覆盖读模型日期字段和浏览器交互主路径。

### 3.2 本次不包含

- [x] 不做 `History -> Data` 的反向跳转。
- [x] 不在 `History` 中新增“来源于 Data”的特殊视觉状态。
- [x] 不按应用过滤 `History`。
- [x] 不在 `应用趋势` 左侧应用列表上做双击跳转，因为应用列表没有具体日期上下文，单击仍用于切换选中应用。
- [x] 不改变 `History` 的 read model、SQLite 查询或 Rust 运行时逻辑。
- [x] 不为 `近一年`月粒度趋势点强行跳转到某一天。
- [x] 不引入 URL 路由、全局状态库或持久化导航状态。

## 4. Owner 判断

### 4.1 Frontend owner

- `features/data` 拥有“用户在 Data 页选择某个日期目标”的 UI 意图。
- `app` 拥有跨页面导航编排和未保存变更保护。
- `features/history` 拥有“展示某个日期的历史详情”的本地状态。
- `styles/quiet-pro.css` 拥有当前 Data/History 相关 Quiet Pro 样式。

### 4.2 不应修改的 owner

- 不修改 `platform/persistence/*`，因为本次不需要新增查询能力。
- 不修改 `src-tauri/*`，因为本次不涉及运行时写侧、IPC 或 Rust 数据边界。
- 不新增 `shared/*` 抽象，除非实现过程中证明某个能力已经稳定跨 feature 复用。

## 5. 推荐交互

### 5.1 活动热力图

- 默认状态保持当前视觉密度。
- 可跳转日期在 hover 时提供轻量反馈。
- 双击可跳转日期后打开对应 `历史` 日期。
- 未来日期、年外日期、加载占位状态不响应跳转。
- 保留现有 tooltip 的日期与时长信息，不用跳转说明替换时长信息。

### 5.2 活动趋势

- 仅 `近 7 天` 和 `近 30 天`支持按日跳转。
- `近一年`趋势按月聚合，暂不跳转。
- 双击图表中的日粒度点打开对应日期的 `历史`。
- 如果图表库事件无法稳定定位 active point，应优先封装一个小的事件解析 helper，并用测试覆盖。
- 不改变趋势图的单击行为。

### 5.3 应用趋势

- 左侧应用列表继续保持单击选择应用，不增加双击跳转。
- 右侧应用趋势图在 `近 7 天` 和 `近 30 天`范围内支持双击日粒度点。
- 双击应用趋势图中的某一天时，打开对应日期的 `历史`。
- 本次只传递日期，不传递应用过滤条件。
- `近一年`应用趋势按月聚合，暂不跳转。
- 如果未来要支持“打开这一天并定位到该应用”，应作为单独需求扩展 History 的过滤或高亮能力。

### 5.4 导航保护

- 如果当前页存在未保存的 `设置` 或 `应用` 变更，继续弹出现有保存确认。
- 用户取消保存时，不跳转，不改变 `History` 目标日期。
- 保存失败时，不跳转，不改变 `History` 目标日期。
- 保存成功后再切换到 `历史` 并应用日期。

## 6. 详细执行步骤

### 6.1 准备与基线确认

- [x] 确认工作区没有与本任务冲突的未提交改动。
- [x] 阅读并遵守：
  - [x] `docs/product-principles-and-scope.md`
  - [x] `docs/quiet-pro-component-guidelines.md`
  - [x] `docs/architecture.md`
  - [x] `docs/issue-fix-boundary-guardrails.md`
- [x] 确认本次属于“核心页面体验打磨”，不越过当前产品边界。
- [x] 确认 `docs/working/data-history-date-link-execution-plan.md` 是本次执行依据。

### 6.2 补齐 Data 趋势日期语义

目标文件：

- `src/features/data/services/dataReadModel.ts`
- `tests/dataReadModel.test.ts`

步骤：

- [x] 在 `DataTrendPoint` 中增加 `date: string | null`。
- [x] 确认 `DataAppTrendPoint` 已有 `date` 字段，可直接作为应用趋势图跳转日期来源。
- [x] 在 `DataTrendViewModel` 中增加 `granularity: "day" | "month"`。
- [x] 在 `DataAppTrendViewModel` 中增加 `granularity: "day" | "month"`，或通过 `range` 明确推导日/月粒度。
- [x] 在 `buildDataTrendViewModel` 中：
  - [x] `7` 天和 `30` 天范围生成 `date`，格式为 `YYYY-MM-DD`。
  - [x] `365` 天范围保留月聚合，`date` 可为该月 key 或 `null`，但必须通过 `granularity: "month"` 明确不可按日跳转。
  - [x] 不改变原有 `label`、`hours`、总时长和平均值计算。
- [x] 在 `tests/dataReadModel.test.ts` 中增加用例：
  - [x] `7` 天趋势点包含可跳转日期。
  - [x] `30` 天趋势点包含可跳转日期。
  - [x] `365` 天趋势声明为月粒度，不参与按日跳转。
  - [x] 应用趋势在日粒度范围内保留可跳转日期。
- [x] 运行 `npm run test:data`。

验收标准：

- [x] 现有 Data 趋势统计结果不变。
- [x] 日粒度趋势点具备稳定日期字段。
- [x] 应用趋势日粒度点具备稳定日期字段。
- [x] 月粒度趋势不会被实现层误判为日粒度。

### 6.3 调整 AppShell 导航编排

目标文件：

- `src/app/hooks/useAppShellNavigation.ts`
- `src/app/AppShell.tsx`

步骤：

- [x] 将 `handleNavigate(nextView)` 的内部异步流程调整为可返回导航结果。
- [x] 推荐返回值：

```ts
type NavigationResult = {
  navigated: boolean;
};
```

- [x] 保持现有调用方可用，避免侧边栏普通导航需要改复杂逻辑。
- [x] 在未保存变更场景中：
  - [x] 用户取消确认时返回 `{ navigated: false }`。
  - [x] 保存失败时返回 `{ navigated: false }`。
  - [x] 保存成功并切换视图后返回 `{ navigated: true }`。
- [x] 在无脏状态普通切换时返回 `{ navigated: true }`。
- [x] 在目标 view 等于当前 view 时返回 `{ navigated: true }`，方便 Data 已经在 History 场景之外的统一处理。
- [x] 在 `AppShell.tsx` 中新增状态：

```ts
type HistoryDateRequest = {
  dateKey: string;
  requestId: number;
};
```

- [x] 在 `AppShellContent` 中新增 `openHistoryForDate(dateKey: string)`：
  - [x] 校验 `dateKey` 是否为合法本地日期。
  - [x] 校验日期不晚于今天。
  - [x] 调用 `handleNavigate("history")`。
  - [x] 仅当 `navigated` 为 `true` 时更新 `historyDateRequest`。
- [x] 将 `historyDateRequest` 传给 `History`。
- [x] 将 `openHistoryForDate` 传给 `Data`。

验收标准：

- [x] 普通侧边栏导航行为不变。
- [x] 未保存变更确认仍然生效。
- [x] Data 发出的日期跳转只能通过 AppShell 完成跨页面切换。

### 6.4 让 History 接收日期请求

目标文件：

- `src/features/history/components/History.tsx`

步骤：

- [x] 给 `History` props 增加可选字段：

```ts
selectedDateRequest?: {
  dateKey: string;
  requestId: number;
};
```

- [x] 增加 `parseLocalDateKey(dateKey)` 局部 helper：
  - [x] 只接受 `YYYY-MM-DD`。
  - [x] 使用本地时间构造 `Date`，避免 UTC 日期偏移。
  - [x] 对非法日期返回 `null`。
- [x] 初始化 `selectedDate` 时优先使用合法 `selectedDateRequest`。
- [x] 增加 effect 监听 `selectedDateRequest?.requestId`：
  - [x] 合法日期才更新。
  - [x] 更新 `selectedDate`。
  - [x] 更新 `calendarMonth`。
  - [x] 关闭 `calendarOpen`。
  - [x] 关闭 `timelineDetailsPopover`。
  - [x] 清空 `timelineDetailsTriggerRef`。
- [x] 不改 `loadData` 的依赖关系，让现有 `selectedDate` 变化自动触发加载。
- [x] 不在 History 内部关心请求来自 Data 还是未来其他入口。

验收标准：

- [x] 进入 History 后页面标题日期正确。
- [x] 时间线、当日活动、应用分布都基于目标日期。
- [x] 用户仍可继续使用前后一天按钮和日期选择器。

### 6.5 接入 Data 热力图跳转

目标文件：

- `src/features/data/components/Data.tsx`
- `src/styles/quiet-pro.css`

步骤：

- [x] 给 `Data` props 增加：

```ts
onOpenHistoryDate?: (dateKey: string) => void;
```

- [x] 为 heatmap cell 计算 `canOpenHistoryDate`：
  - [x] 非未来日期。
  - [x] 非年外日期。
  - [x] 非加载 skeleton。
  - [x] `cell.date` 是合法日期 key。
- [x] 保持 heatmap cell 为非 Tab 停靠点，不把整张热力图改成按钮网格。
- [x] 双击可跳转 cell 时调用 `onOpenHistoryDate(cell.date)`。
- [x] 保留 heatmap tooltip 当前的日期与时长信息。
- [x] 可跳转性通过 cursor 和 hover 表达，不额外占用 tooltip 文案。
- [x] 为不可跳转 cell 保持当前不可交互语义。
- [x] 在 `quiet-pro.css` 中补充：
  - [x] 可双击 heatmap cell 的 default。
  - [x] hover 状态。
  - [x] active 状态。
  - [x] disabled / unavailable 状态。
- [x] 所有样式使用现有 token 或 `color-mix` 现有 token，不新增硬编码品牌色。

验收标准：

- [x] 热力图原有布局宽度、滚动和密度不明显变化。
- [x] 鼠标用户能发现可打开日期，但视觉不过度突出。
- [x] 未来日期和年外日期不会触发跳转。

### 6.6 接入 Data 活动趋势跳转

目标文件：

- `src/features/data/components/Data.tsx`
- 视需要新增 feature-local helper，例如 `src/features/data/services/dataChartInteraction.ts`
- 视需要新增对应测试文件

步骤：

- [x] 判断 `trendViewModel.granularity === "day"` 时才启用趋势跳转。
- [x] 给趋势图容器或 `AreaChart` 增加双击处理。
- [x] 从 Recharts 事件中解析 active payload：
  - [x] 找到当前 active datum。
  - [x] 读取 datum.date。
  - [x] 校验 dateKey。
  - [x] 调用 `onOpenHistoryDate(dateKey)`。
- [x] 如果 Recharts 事件对象类型不稳定：
  - [x] 建立 feature-local helper `resolveTrendDateFromChartEvent(event)`。
  - [x] 只在 helper 内部处理 `unknown` / 宽松类型。
  - [x] 组件只消费 `string | null`。
- [x] 给图表容器增加轻量 cursor 或 hover 反馈。
- [x] 如果图表 tooltip 已显示日期与时长，保留原有信息，不用跳转说明替换。
- [x] `365` 天范围不添加跳转提示。

验收标准：

- [x] 近 7 天 / 近 30 天趋势双击可打开对应 History 日期。
- [x] 近一年趋势不会打开错误日期。
- [x] 图表原有 tooltip、渲染和统计不受影响。

### 6.7 接入 Data 应用趋势跳转

目标文件：

- `src/features/data/components/Data.tsx`
- 视需要复用 `src/features/data/services/dataChartInteraction.ts`
- 视需要新增或扩展对应测试文件

步骤：

- [x] 保持左侧应用列表单击选择应用的现有行为。
- [x] 不给左侧应用列表增加双击跳转。
- [x] 判断 `selectedAppTrendRange !== 365` 时才启用应用趋势图日期跳转。
- [x] 给应用趋势图容器或 `AreaChart` 增加双击处理。
- [x] 从 Recharts 事件中解析 active payload：
  - [x] 找到当前 active datum。
  - [x] 读取 datum.date。
  - [x] 校验 dateKey。
  - [x] 调用 `onOpenHistoryDate(dateKey)`。
- [x] 如果活动趋势和应用趋势解析逻辑一致，复用同一个 feature-local helper。
- [x] 如果图表 tooltip 已显示日期与时长，保留原有信息。
- [x] 可跳转性优先通过 cursor 和 hover 表达，不承诺按应用过滤。
- [x] `365` 天应用趋势范围不添加跳转提示。

验收标准：

- [x] 近 7 天 / 近 30 天应用趋势双击可打开对应 History 日期。
- [x] 左侧应用列表仍只负责选择应用。
- [x] History 只按日期打开，不出现未实现的应用过滤假象。
- [x] 图表原有 tooltip、渲染和统计不受影响。

### 6.8 文案策略

步骤：

- [x] 不新增可见 UI 文案。
- [x] 不新增“打开这一天的历史” tooltip。
- [x] 热力图 tooltip 继续显示现有日期与时长。
- [x] 图表 tooltip 继续显示现有日期与时长。
- [x] 跳转能力只通过双击、cursor 和轻量 hover 反馈表达。

验收标准：

- [x] 用户看到的日期与时长信息不减少。
- [x] 页面上不出现额外说明性文字。

### 6.9 浏览器交互测试

目标文件：

- `tests/uiBrowserSmoke.test.ts`

步骤：

- [x] 扩展 Tauri SQL stub，使 Data 页热力图有可点击历史日期。
- [x] 在测试中导航到 `数据`。
- [x] 等待活动热力图渲染出可打开日期 cell。
- [x] 触发双击。
- [x] 等待侧边栏 `历史` nav item 激活。
- [x] 断言页面正文包含目标日期 label。
- [x] 断言没有 console error。

建议测试名称：

```ts
await runTest("data heatmap opens the selected day in history", async () => {
  // ...
});
```

可选追加：

- [x] 如果趋势图事件稳定，可增加趋势双击 smoke。
- [x] 如果应用趋势图事件稳定，可增加应用趋势双击 smoke。
- [x] 如果趋势图事件难以在 headless 浏览器中稳定模拟，使用 helper 单测覆盖事件解析，browser smoke 先覆盖 heatmap 主路径。

验收标准：

- [x] 测试证明 Data -> History 日期联动可端到端工作。
- [x] 不显著拉长 UI smoke 超时时间。

### 6.10 局部验证

按风险从小到大执行：

- [x] `npm run test:data`
- [x] `npm run test:ui-browser-smoke`
- [x] 如果新增 interaction helper，运行对应测试或 `npm run test:interaction`
- [x] `npm run build`

如果其中任一步失败：

- [x] 先确认是否是本次修改造成。
- [x] 如果是，修复后重跑失败项。
- [x] 如果不是，记录现有失败和影响范围，不隐藏风险。

### 6.11 最终验证

本次触及核心页面交互、read model 形状和 AppShell 导航编排，交付前应运行：

- [x] `npm run check`

如果实现过程中意外触及 Rust、SQLite schema、IPC 或发布链路，则追加：

- [x] `npm run check:full`

## 7. 文件修改清单

预计修改：

- [x] `src/features/data/services/dataReadModel.ts`
- [x] `src/features/data/components/Data.tsx`
- [x] `src/features/history/components/History.tsx`
- [x] `src/app/hooks/useAppShellNavigation.ts`
- [x] `src/app/AppShell.tsx`
- [x] `src/styles/quiet-pro.css`
- [x] `tests/dataReadModel.test.ts`
- [x] `tests/uiBrowserSmoke.test.ts`

视实现情况可能新增：

- [x] `src/features/data/services/dataChartInteraction.ts`
- [x] `tests/dataChartInteraction.test.ts`

不应修改：

- [x] `src-tauri/**`
- [x] `src/platform/persistence/**`
- [x] `docs/archive/**`
- [x] 顶层长期规则文档，除非实施过程中发现长期规则确实需要变化。

## 8. 风险与处理

### 8.1 Recharts 双击事件不稳定

- [x] 先尝试读取 active payload。
- [x] 若事件类型不清晰，将解析逻辑封装在 Data feature service 内。
- [x] 不把图表事件宽松类型扩散到 AppShell 或 shared 层。

### 8.2 日期时区偏移

- [x] 日期 key 使用 `YYYY-MM-DD`。
- [x] 解析时使用 `new Date(year, monthIndex, day)`。
- [x] 不用 `new Date("YYYY-MM-DD")` 作为本地日期来源。

### 8.3 未保存变更保护被绕过

- [x] 所有 Data -> History 跳转必须经过 `handleNavigate("history")`。
- [x] `historyDateRequest` 只能在导航成功后更新。

### 8.4 History 重复请求同一天不生效

- [x] 使用 `requestId` 保证同一天重复触发也可被 History effect 感知。
- [x] History effect 依赖 `selectedDateRequest.requestId`，而不是只依赖 `dateKey`。

### 8.5 UI 过度强调

- [x] 不添加大按钮、强色块或新装饰。
- [x] 只使用 hover/cursor 传达可操作性，不替换现有时长 tooltip，不新增 focus-visible outline。
- [x] 遵守 Quiet Pro：克制、清晰、长期可用。

## 9. 完成标准

- [x] 用户可从 Data 热力图打开某一天 History。
- [x] 用户可从 Data 日粒度活动趋势打开某一天 History。
- [x] 用户可从 Data 日粒度应用趋势图打开某一天 History。
- [x] 用户双击应用列表不会触发误跳转。
- [x] History 正确显示目标日期的标题、时间线、应用分布和当日活动。
- [x] 未保存变更保护仍然有效。
- [x] 不改变数据统计口径。
- [x] 不新增 Rust、SQLite 或 IPC 复杂度。
- [x] Quiet Pro 视觉一致。
- [x] `npm run check` 通过，或清楚记录无法通过的外部原因。

## 10. 推荐实施顺序

- [x] 第一步：补 `dataReadModel` 日期语义与单测。
- [x] 第二步：改 `useAppShellNavigation` 返回导航结果。
- [x] 第三步：在 `AppShell` 建立 `openHistoryForDate` 和 `historyDateRequest`。
- [x] 第四步：让 `History` 响应外部日期请求。
- [x] 第五步：接入 Data 热力图跳转。
- [x] 第六步：接入 Data 活动趋势跳转。
- [x] 第七步：接入 Data 应用趋势图跳转。
- [x] 第八步：确认文案策略并补样式状态。
- [x] 第九步：补浏览器 smoke。
- [x] 第十步：执行局部验证。
- [x] 第十一步：执行 `npm run check` 并收口结果。
