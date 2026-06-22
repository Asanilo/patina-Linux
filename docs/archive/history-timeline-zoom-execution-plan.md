# History 时间轴缩放执行方案

## 文档状态

- 状态：已完成并归档
- 关联反馈：GitHub issue [#6](https://github.com/Ceceliaee/patina/issues/6) 后续评论
- 目标版本：待定
- 真实 owner：`features/history`
- 文档归档规则：本方案完成后移入 `docs/archive/`，不要长期留在 top-level `docs/`

## 背景

#6 中原始反馈已覆盖统计范围、分类管理、按分类查看、时间轴与网页同步等能力。后续长期体验反馈新增一个明确诉求：

> 时间轴希望能加个缩放功能。

当前 `History` 页面已有横向日内时间轴，但实现以全天 24 小时为固定比例。一天中活动较碎时，短会话和高频切换会被压缩成很窄的线段，用户难以细看某个工作时段内的真实切换。

本方案只处理单日时间轴的离散缩放，不扩展 History 为跨日分析工作台。

## 产品判断

### 属于当前主线

- [x] 增强 `History` 的可回看性。
- [x] 服务个人、本地优先、桌面时间追踪场景。
- [x] 不改变 tracking、session 切分、分类统计或数据存储。
- [x] 不引入云、账号、团队、截图回放或重型分析能力。

### 优先级

按 `docs/roadmap-and-prioritization.md` 判断，本事项属于：

- 核心页面体验打磨。
- 提升长期使用时的时间回看效率。
- 优先级低于 tracking 正确性与数据安全，高于低频装饰性改进。

## 范围

### 本次目标

- [x] 在 History 的时间轴详情弹窗中加入离散缩放。
- [x] 缩放档位固定为 `24h / 12h / 8h / 4h / 1h`。
- [x] 支持在当前日期内左右平移时间窗口。
- [x] 缩放和平移只改变时间轴可视窗口，不改变当天摘要、当日分布、小时活动图、session 列表统计口径。
- [x] 保留主页面上的全天概览时间轴，默认仍展示 24 小时。
- [x] 保持 Quiet Pro 风格：克制、明确、稳定、低噪音。

### 非目标

- [x] 不做连续自由缩放。
- [x] 不做鼠标位置锚定缩放。
- [x] 不做拖拽平移。
- [x] 不做跨日、周、月、年时间轴。
- [x] 不改变 History 的单日定位。
- [x] 不改变 Data 页面自定义范围能力。
- [x] 不新增全局设置项。
- [x] 不改变 session 合并、最短时长过滤或 tracking 数据写入。
- [x] 不让页面组件直接访问 SQLite、Tauri API 或 platform gateway。
- [x] 不使用 issue-closing 关键词；若需要引用，只写 `Refs #6`。

## 交互规格

### 入口

- [x] 主页面时间轴保持现状：全天概览 + 分类/应用切换 + 打开详情按钮。
- [x] 点击现有展开按钮后，打开时间轴详情弹窗。
- [x] 时间轴详情弹窗新增可缩放时间轴区域。
- [x] 时间轴详情弹窗下方继续保留现有 session 列表。

### 缩放档位

固定档位：

- [x] `24h`：全天，默认档。
- [x] `12h`：半日视图。
- [x] `8h`：标准工作时段视图。
- [x] `4h`：半个工作段视图。
- [x] `1h`：细看短会话和碎片切换。

建议 UI：

- [x] 使用 `QuietSegmentedFilter` 承载档位。
- [x] 标签使用 `24h`、`12h`、`8h`、`4h`、`1h`，避免长文本挤压工具栏。
- [x] 增加 `aria-label` 或上层说明，使辅助技术能识别为时间轴缩放。

### 平移

- [x] 在缩放档位左侧或右侧加入上一窗口、下一窗口按钮。
- [x] 使用 lucide 图标，例如 `ChevronLeft` 与 `ChevronRight`。
- [x] 平移步长为当前窗口的一半：
  - `12h` 每次移动 `6h`
  - `8h` 每次移动 `4h`
  - `4h` 每次移动 `2h`
  - `1h` 每次移动 `30m`
- [x] `24h` 档位下禁用左右平移按钮。
- [x] 到达当天边界时禁用对应方向按钮。
- [x] 平移窗口不得越过 `dayStartMs` 与 `dayEndMs`。

### 初始窗口

打开详情弹窗时：

- [x] 默认缩放档位为 `24h`。
- [x] 默认窗口为当天 `00:00 - 24:00`。

切换到较小档位时：

- [x] 优先保持当前窗口中心点。
- [x] 如果没有当前中心点，使用当天最新可见活动时间。
- [x] 如果当天没有活动，使用 `dayStartMs`。
- [x] 今天的数据允许窗口位于未来时段，但不会显示未来活动；不额外制造“未来不可查看”状态。

日期变化时：

- [x] 关闭详情弹窗。
- [x] 重置窗口为 `24h` 全天。
- [x] 清空已展开的 title details popover。

### 坐标轴

坐标轴应随窗口变化：

- [x] `24h`：`00:00 / 06:00 / 12:00 / 18:00 / 24:00`
- [x] `12h`：每 `3h` 一个刻度
- [x] `8h`：每 `2h` 一个刻度
- [x] `4h`：每 `1h` 一个刻度
- [x] `1h`：每 `15m` 一个刻度

显示要求：

- [x] 轴标签必须与当前窗口对应。
- [x] 末端如果正好是当天结束，显示 `24:00`。
- [x] 其他时间使用现有 `formatTime` 口径。
- [x] 轴标签不能溢出或互相覆盖。

### tooltip 和可访问性

- [x] tooltip 继续显示应用/分类名、起止时间、持续时长。
- [x] tooltip 定位基于缩放后的窗口比例，而不是全天比例。
- [x] 键盘 focus 仍能触发 tooltip。
- [x] segment 的 `aria-label` 使用当前可见时间范围内的起止时间。
- [x] 缩放和平移按钮必须有明确 `aria-label`。
- [x] 禁用按钮使用 `disabled`，不要只靠视觉表达。

### 详情列表

第一版建议：

- [x] 列表继续展示当天经过最短时长过滤后的完整 session 列表。
- [x] 不按当前缩放窗口过滤列表。
- [x] 列表的最短时长控制继续保留现有逻辑。

后续可选增强：

- [x] 如果用户反馈希望列表跟随窗口，再单独实现“只看当前窗口”开关。

## 架构边界

### 允许修改

- [x] `src/features/history/components/History.tsx`
- [x] `src/features/history/components/HistoryHorizontalTimeline.tsx`
- [x] `src/features/history/services/historyTimelineViewModel.ts`
- [x] `src/features/history/services/historyLayoutPreferenceStorage.ts`
- [x] `src/styles/features/history.css`
- [x] `src/shared/copy/uiText.ts` 中已有 History accessibility 文案区域
- [x] History 相关测试文件

### 谨慎修改

- [x] `src/shared/components/QuietSegmentedFilter.tsx`
  - 只有在现有组件无法表达 `aria-label` 时才扩展。
  - 扩展必须保持向后兼容。
- [x] `src/styles/quiet-pro.css`
  - 只有新增可复用控件状态时才动。
  - 优先使用现有 `qp-control`、`qp-button-secondary`、`qp-segmented-filter`。

### 禁止修改

- [x] 不改 Rust tracking runtime。
- [x] 不改 SQLite schema。
- [x] 不改 session repository。
- [x] 不改 Dashboard/Data 读模型。
- [x] 不把缩放状态放进 `app/*`。
- [x] 不新增 `src/lib/*` 或 `src/types/*`。
- [x] 不新增跨 feature 的临时 shared helper。

## 数据与状态设计

### 新增类型

在 `historyTimelineViewModel.ts` 或 History-owned 相邻 service 中定义：

- [x] `HistoryTimelineZoomHours = 24 | 12 | 8 | 4 | 1`
- [x] `HistoryTimelineViewport`

建议形状：

```ts
export type HistoryTimelineZoomHours = 24 | 12 | 8 | 4 | 1;

export interface HistoryTimelineViewport {
  startMs: number;
  endMs: number;
  zoomHours: HistoryTimelineZoomHours;
}
```

### 常量

- [x] 定义 `HISTORY_TIMELINE_ZOOM_OPTIONS = [24, 12, 8, 4, 1] as const`
- [x] 定义 `HOUR_MS = 60 * 60 * 1000`
- [x] 复用现有 `DAY_MS`

### 状态归属

在 `History.tsx` 内部维护：

- [x] `timelineZoomHours`
- [x] `timelineViewportStartMs`

第一版不持久化 viewport offset。

是否持久化 zoom 档位：

- [x] 推荐第一版不持久化，避免用户下次打开详情时误以为全天数据消失。
- [x] 如果实现过程中决定持久化，只持久化 `zoomHours`，不持久化 `viewportStartMs`。
- [x] 若持久化，应放入 `historyLayoutPreferenceStorage.ts`，使用 best-effort localStorage，保持现有布局偏好风格。

## 实现步骤

### 第 0 阶段：准备与确认

- [x] 确认工作区状态，避免覆盖用户改动。
- [x] 确认 `docs/working/history-timeline-zoom-execution-plan.md` 是当前执行依据。
- [x] 打开 `History.tsx`，确认详情弹窗当前只渲染 toolbar + session/web list。
- [x] 打开 `HistoryHorizontalTimeline.tsx`，确认 segment 当前使用 `startRatio/endRatio/widthRatio`。
- [x] 打开 `historyTimelineViewModel.ts`，确认 ratio 当前基于全天 `DAY_MS`。
- [x] 打开 `tests/historyTimelineViewModel.test.ts`，确认可补 viewport 单测。
- [x] 打开 `tests/uiBrowserSmoke.test.ts`，确认已有 History 时间轴交互 smoke 可扩展。

### 第 1 阶段：抽出 viewport 计算规则

- [x] 在 History-owned service 中新增 `normalizeHistoryTimelineViewport`。
- [x] 输入参数包含：
  - [x] `selectedDate`
  - [x] `nowMs`
  - [x] `zoomHours`
  - [x] `requestedStartMs`
- [x] 输出 clamp 后的 `{ startMs, endMs, zoomHours }`。
- [x] `24h` 永远输出当天 `00:00 - 24:00`。
- [x] 非 `24h` 使用 `zoomHours * HOUR_MS` 作为窗口长度。
- [x] 当 `requestedStartMs` 小于当天开始时，clamp 到当天开始。
- [x] 当 `requestedStartMs + duration` 大于当天结束时，clamp 到当天结束减窗口长度。
- [x] 窗口长度不得超过一天。
- [x] 对非法输入使用安全 fallback，不抛给 UI。

单测：

- [x] `24h` 输出完整日。
- [x] `8h` 在日内正常输出。
- [x] `4h` 靠近日末时 clamp 到 `20:00 - 24:00`。
- [x] `1h` 靠近日初时 clamp 到 `00:00 - 01:00`。
- [x] 非法 start fallback 到日初。

### 第 2 阶段：让 view model 支持 viewport

修改 `buildHistoryTimelineViewModel` 参数：

- [x] 新增可选 `viewport?: HistoryTimelineViewport`。
- [x] 默认 viewport 为全天，保持现有调用不变。
- [x] 保留 `dayStartMs/dayEndMs` 作为当天边界。
- [x] 新增 `viewportStartMs/viewportEndMs/viewportDurationMs` 到返回值。
- [x] segment clipping 使用 viewport 边界：
  - [x] `clippedStart = max(sessionStart, dayStartMs, viewportStartMs)`
  - [x] `clippedEnd = min(sessionEnd, dayEndMs, visibleEndMs, viewportEndMs)`
- [x] ratio 使用 viewport 边界：
  - [x] `startRatio = (clippedStart - viewportStartMs) / viewportDurationMs`
  - [x] `endRatio = (clippedEnd - viewportStartMs) / viewportDurationMs`
- [x] `widthRatio` 继续由 `endRatio - startRatio` 得出。
- [x] title sample clipping 使用缩放后 segment 的可见范围。
- [x] `visibleEndMs` 继续表示今天的真实可见数据截止，不等同于 viewport end。
- [x] legend 第一版按当前可视 viewport 内 segment 统计。

单测：

- [x] 默认不传 viewport 时，现有测试全部保持通过。
- [x] `8h` viewport 内 segment ratio 正确。
- [x] 跨 viewport 起点的 session 被裁剪到 viewport start。
- [x] 跨 viewport 终点的 session 被裁剪到 viewport end。
- [x] viewport 外 session 不出现。
- [x] category mode 在 viewport 内仍按分类聚合。
- [x] title sample details 被裁剪到 viewport 可见范围。

### 第 3 阶段：动态坐标轴

- [x] 将 `buildAxisTicks()` 改为接收 viewport。
- [x] 为不同窗口选择 tick step：
  - [x] `24h`：`6h`
  - [x] `12h`：`3h`
  - [x] `8h`：`2h`
  - [x] `4h`：`1h`
  - [x] `1h`：`15m`
- [x] tick 生成范围从 `viewportStartMs` 到 `viewportEndMs`。
- [x] tick ratio 使用 viewport duration。
- [x] tick label 使用 `formatTimelineTime` 能理解的绝对时间。
- [x] 如果最后一个 tick 不等于 viewport end，补一个 viewport end tick。
- [x] 避免重复 tick label。

单测：

- [x] `24h` 仍输出 `00:00 / 06:00 / 12:00 / 18:00 / 24:00`。
- [x] `8h` 从 `09:00 - 17:00` 输出 `09:00 / 11:00 / 13:00 / 15:00 / 17:00`。
- [x] `1h` 从 `09:30 - 10:30` 输出 `09:30 / 09:45 / 10:00 / 10:15 / 10:30`。
- [x] 跨到当天结束时末端显示 `24:00`。

### 第 4 阶段：扩展 `HistoryHorizontalTimeline`

- [x] 增加 `variant="expanded"` 的真实渲染使用场景。
- [x] 确认 `expanded` 不走主页面宽屏 track height 自动放大逻辑，避免弹窗内过高。
- [x] tooltip left 使用 viewport 后 ratio，无需组件额外换算。
- [x] 在 root 节点增加调试/测试属性：
  - [x] `data-history-timeline-zoom-hours`
  - [x] `data-history-timeline-window-start`
  - [x] `data-history-timeline-window-end`
- [x] 确认空状态在小窗口下居中。
- [x] 确认 legend 过多时仍走现有 `+N` tooltip。

样式：

- [x] 在 `src/styles/features/history.css` 中为详情弹窗时间轴增加布局。
- [x] 不新增强阴影、渐变、玻璃、霓虹。
- [x] 控件高度与现有 toolbar 对齐。
- [x] 轨道高度固定，避免切换档位时弹窗跳动。

### 第 5 阶段：在详情弹窗接入缩放时间轴

在 `History.tsx` 中：

- [x] 新增 `timelineZoomHours` state。
- [x] 新增 `timelineViewportStartMs` state。
- [x] 新增 `timelineViewport` memo。
- [x] 新增 `timelineDialogTimelineView` memo，调用 `buildHistoryTimelineViewModel` 并传入 viewport。
- [x] 详情弹窗打开时初始化为 `24h`。
- [x] 日期变化时重置为 `24h`。
- [x] 切换 `timelineDialogMode` 时不影响 zoom。
- [x] web tab 下第一版不显示缩放时间轴，继续显示 web list。
- [x] app tab 下显示缩放时间轴 + app session list。
- [x] 如果以后 web 也需要视觉时间轴，另开小方案。

toolbar 调整：

- [x] 左侧保留 app/web tab 和 session count。
- [x] 右侧组合：
  - [x] 上一窗口按钮
  - [x] 缩放档位 segmented filter
  - [x] 下一窗口按钮
  - [x] 最短时长控制
- [x] 小宽度下 toolbar 可换行，不产生横向溢出。

行为函数：

- [x] `handleTimelineZoomChange(nextZoomHours)`
- [x] `shiftTimelineViewport(direction)`
- [x] `resetTimelineViewportForDate()`
- [x] `canShiftTimelineViewportBackward`
- [x] `canShiftTimelineViewportForward`

### 第 6 阶段：补齐文案与可访问性

在 `src/shared/copy/uiText.ts` 或 History 局部 copy 中补：

- [x] 中文：`时间轴缩放`
- [x] 英文：`Timeline zoom`
- [x] 中文：`上一时间窗口`
- [x] 英文：`Previous time window`
- [x] 中文：`下一时间窗口`
- [x] 英文：`Next time window`
- [x] 中文：`当前窗口 {start} - {end}`
- [x] 英文：`Current window {start} - {end}`

检查：

- [x] 中英文 copy key 结构一致。
- [x] 不把长说明直接放在 UI 上。
- [x] 不在应用内解释快捷键或实现细节。

### 第 7 阶段：测试

单元测试：

- [x] `tests/historyTimelineViewModel.test.ts`
  - [x] viewport clamp
  - [x] viewport ratio
  - [x] viewport clipping
  - [x] dynamic axis ticks
  - [x] default full-day compatibility
- [x] 如新增 storage：
  - [x] zoom preference accepts only `24/12/8/4/1`
  - [x] invalid value fallback to `24`

源码 smoke：

- [x] `tests/uiSmoke.test.ts`
  - [x] History dialog 包含 zoom 控件。
  - [x] 主页面时间轴仍没有复杂缩放控件。
  - [x] web activity gating 断言不受影响。

浏览器 smoke：

- [x] `tests/uiBrowserSmoke.test.ts`
  - [x] 打开 History。
  - [x] 打开时间轴详情弹窗。
  - [x] 确认默认 zoom 为 `24h`。
  - [x] 点击 `8h`。
  - [x] 确认 `data-history-timeline-zoom-hours="8"`。
  - [x] 确认轴标签出现 8 小时窗口。
  - [x] 点击下一窗口。
  - [x] 确认 window start/end 变化。
  - [x] 点击上一窗口回退。
  - [x] 确认无横向溢出。
  - [x] 在 390px mobile viewport 下打开弹窗，确认 toolbar 换行且不溢出。

回归测试：

- [x] `history hourly chart toggles category layers` 仍通过。
- [x] `history timeline opens list dialog from timeline axis` 更新为新详情布局后仍通过。
- [x] title details popover 仍可打开、关闭、随滚动消失。

### 第 8 阶段：人工验收

桌面默认宽度：

- [x] History 首屏仍保持 Quiet Pro 密度。
- [x] 主页面时间轴仍是全天概览。
- [x] 详情弹窗打开后不会压迫页面。
- [x] `24h / 12h / 8h / 4h / 1h` 控件可扫读。
- [x] `1h` 下短片段明显更容易识别。
- [x] 左右平移按钮状态正确。
- [x] tooltip 不越界。
- [x] 弹窗关闭后回到 History 无状态泄漏。

窄宽度：

- [x] toolbar 换行后仍清晰。
- [x] 按钮文本不溢出。
- [x] 轨道和轴标签不发生横向页面溢出。

今天：

- [x] 活动到当前时间为止。
- [x] 缩放到未来窗口时不会显示错误数据。
- [x] live session 时长仍随现有刷新逻辑更新。

历史日期：

- [x] 全天窗口显示完整记录。
- [x] 缩放窗口按日期内边界 clamp。
- [x] 跨午夜 session 只显示选中日期内部分。

## 验证命令

局部验证：

- [x] `npm run test:history-timeline`
- [x] `npm run test:ui-smoke`
- [x] `npm run test:ui-browser-smoke`

默认交付验证：

- [x] `npm run check`

如果沙箱内 Vite/esbuild 出现 `spawn EPERM`：

- [x] 记录失败原因。
- [x] 在允许的沙箱外重跑 `npm run test:ui-browser-smoke`。
- [x] 在允许的沙箱外重跑 `npm run build`。
- [x] 继续执行 `npm run check:bundle`。

## 验收标准

- [x] 用户可以在 History 时间轴详情中切换 `24h / 12h / 8h / 4h / 1h`。
- [x] 用户可以在非 `24h` 档位左右平移窗口。
- [x] 所有窗口都限制在单日内。
- [x] 主页面仍保持全天概览，不被缩放控件占用。
- [x] 统计数字不因缩放变化而改变。
- [x] 缩放窗口内 segment、tooltip、axis tick 与实际时间一致。
- [x] app/category timeline mode 均可缩放。
- [x] web tab 行为不被破坏。
- [x] UI 符合 Quiet Pro，不出现高噪音控件或装饰。
- [x] `npm run check` 或等价补跑验证通过。

## 风险与处理

### 风险：缩放被误解为过滤统计

- [x] 不改变摘要和分布。
- [x] 不改变 session count 文案，除非明确标注为可视窗口。
- [x] 第一版列表不跟随窗口过滤。

### 风险：toolbar 过密

- [x] 优先图标按钮 + 短标签档位。
- [x] 小屏允许换行。
- [x] 不添加解释性长文案。

### 风险：view model 复杂度回流到组件

- [x] clipping、ratio、axis tick 全部留在 `historyTimelineViewModel.ts` 或 History-owned service。
- [x] 组件只渲染 view model。
- [x] `History.tsx` 只做状态编排。

### 风险：后续想加入自由缩放

- [x] 本次只实现离散 zoom model。
- [x] 不预留复杂 pointer math。
- [x] 如果后续要做自由缩放，另开执行方案。

## 执行备注

- 已完成代码、测试与变更记录更新，并按要求归档。
- 缩放状态第一版未持久化；historyLayoutPreferenceStorage.ts 与 uiText.ts 等可选修改点已审查但无需改动，文案采用 History 局部 copy。
- normalizeHistoryTimelineViewport 未接收 nowMs：今天的真实可见截止仍由 buildHistoryTimelineViewModel 的 visibleEndMs 负责，窗口归一化只负责日内边界 clamp。
- Web tab 第一版保持列表视图；如需网页视觉时间轴，后续另开小方案。
- 沙箱内 Vite/esbuild 触发 spawn EPERM，已按规则提升权限重跑浏览器 smoke 与完整前端检查。
- 未对 GitHub issue 执行评论、关闭或标签变更；仅在 changelog 中按规则使用 Refs #6。
- 后续按产品判断调整：原“时间线”弹窗恢复为应用/网页列表弹窗；时间轴缩放拆成主页面时间轴标题旁的独立“时间轴缩放”弹窗，避免网页列表与应用时间轴混在同一个弹窗模型里。
- 后续交互补充：从 `24h` 首次切换到更小窗口时，焦点按所选日期中与当前时钟相同的时段吸附到最近半小时；滚轮平移后继续切换缩放，则按平移后的窗口中心吸附到最近半小时。
- 后续交互补充：独立缩放弹窗首装默认 `24h`，之后只持久化用户选择的缩放倍率，不持久化窗口位置；滚轮平移步长从当前窗口的 `1/12` 调整为 `1/6`。
- 后续交互补充：缩放窗口内没有可见记录时，空态文案使用“当前时间段暂无记录”，不再沿用整天无记录文案。

## 完成后的收尾

- [x] 更新关联 issue 评论时只引用 `Refs #6`，不关闭 issue，除非用户明确要求。
- [x] 若该能力进入发布范围，在 `CHANGELOG.md` 的 `Unreleased` 中加入用户可感知条目。
- [x] 完成开发并确认方案不再作为 active basis 后，将本文件移入 `docs/archive/`。
