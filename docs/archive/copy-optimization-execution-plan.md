# 文案优化执行记录

## 1. 目标

- [x] 找出并收口所有用户可见文案。
- [x] 将中文文案改得更适配个人桌面时间追踪软件。
- [x] 保持文案简洁、清楚、可扫读。
- [x] 为后续英文翻译预留结构和 UI 空间。
- [x] 不破坏 Quiet Pro 的克制、专业、稳定界面气质。

## 2. 执行原则

- [x] 文案优先服务理解，不制造“设计感”。
- [x] 标题短，说明句短，按钮更短。
- [x] 同一概念只用一个词。
- [x] 高风险操作必须说清后果。
- [x] 不用营销语、情绪化表达或游戏化表达。
- [x] 不为了英文翻译拉长中文。
- [x] 不把文案硬编码继续散落到组件中。
- [x] 英文翻译前先解决语义键、参数和布局余量。

## 3. 文案范围清单

### 3.1 主文案入口

- [x] 审查 `src/shared/copy/uiText.ts`。
- [x] 补齐 `common` 分组：确认、取消、继续、保存、关闭、默认、处理中、加载中。
- [x] 补齐 `accessibility` 分组：窗口控制、图表切换、范围切换、颜色控件、挂件操作。
- [x] 补齐 `toast` 分组：保存、清理、备份、恢复、链接打开失败。
- [x] 补齐 `backup` 分组：备份预览、兼容提示、恢复确认。
- [x] 补齐 `update` 分组：更新状态、更新弹窗、失败兜底、按钮动作。
- [x] 补齐 `categories` 分组：长分类名、短分类名、自定义分类兜底。

### 3.2 散落文案收口

- [x] 收口 `src/features/update/services/updateViewModel.ts` 中的更新状态、按钮、错误说明。
- [x] 收口 `src/features/update/components/UpdateStatusPanel.tsx` 中的更新入口文案。
- [x] 收口 `src/features/update/components/UpdateConfirmDialog.tsx` 中的更新弹窗文案。
- [x] 收口 `src/features/about/components/AboutPanel.tsx` 中的关于页标题和说明。
- [x] 收口 `src/features/about/components/About.tsx` 中的 toast。
- [x] 收口 `src/features/settings/components/SettingsTrackingPanel.tsx` 中的追踪设置文案。
- [x] 收口 `src/features/settings/components/SettingsResidentPanel.tsx` 中的常驻设置文案。
- [x] 收口 `src/features/settings/components/SettingsDataSafetyPanel.tsx` 中的数据安全文案。
- [x] 收口 `src/features/settings/components/SettingsAppearancePanel.tsx` 中的外观设置文案。
- [x] 收口 `src/features/settings/services/settingsPageActions.ts` 中的清理、备份、恢复 toast。
- [x] 收口 `src/features/settings/services/settingsRuntimeAdapterService.ts` 中的备份预览摘要。
- [x] 收口 `src/features/classification/components/AppMapping.tsx` 中的筛选、弹窗、按钮文案。
- [x] 收口 `src/features/classification/components/AppMappingCandidateCard.tsx` 中的应用卡片操作文案。
- [x] 收口 `src/features/classification/components/CategoryColorControls.tsx` 中的颜色与分类操作文案。
- [x] 收口 `src/features/classification/hooks/useAppMappingState.ts` 中的“自动识别”等选项文案。
- [x] 收口 `src/features/data/services/dataReadModel.ts` 中的范围、指标、热力图提示文案。
- [x] 收口 `src/features/data/components/Data.tsx` 中的 aria-label 和图表标签。
- [x] 收口 `src/features/history/components/History.tsx` 中的日历、时间筛选和 aria-label。
- [x] 收口 `src/app/widget/WidgetShell.tsx` 中的挂件标题、按钮、aria-label。
- [x] 收口 `src/app/components/AppTitleBar.tsx` 中的窗口控制 aria-label。
- [x] 收口 `src/app/components/AppSidebar.tsx` 中的“更新”入口文案。
- [x] 收口 `src/shared/components/QuietColorField.tsx`、`QuietConfirmDialog.tsx`、`QuietResetAction.tsx` 中的共享控件文案。
- [x] 审查 `src-tauri/src/domain/backup.rs` 返回给前端的中文提示。

### 3.3 保留内容

- [x] 保留真实应用名映射，如微信、飞书、钉钉、哔哩哔哩。
- [x] 保留窗口标题清洗规则中的真实中文后缀，如“未跟踪”“已修改”。
- [x] 保留 tracking 规则和测试 fixture 中用于识别真实进程、安装窗口的中文。
- [x] 保留 `CHANGELOG.md` 的发布记录中文。
- [x] 保留 Rust 备份中文 `compatibility_message` 作为现有 UI 兼容字段，同时新增 `compatibility_message_key` 和 `compatibility_message_args` 供后续本地化使用。

## 4. 术语执行结果

- [x] `追踪`：系统自动记录当前应用活动。
- [x] `记录`：已经写入的数据结果。
- [x] `统计`：是否计入报表和汇总。
- [x] `时间线`：历史页按时间排列的活动流。
- [x] `无操作`：用户没有输入活动，替代“挂机”。
- [x] `持续计入`：音频、会议、视频等低交互场景继续计时。
- [x] `合并间隔`：短暂切换或返回后仍合并为同一段活动。
- [x] `应用`：应用分类、名称、颜色、统计和标题记录控制。
- [x] `数据安全`：备份、恢复、清理等本地数据控制。
- [x] `常驻`：最小化、托盘、开机启动、挂件相关行为。

## 5. 中文优化结果

### 5.1 全局与导航

- [x] “系统设置”改为“设置”。
- [x] “应用美化与隐私”改为“应用”。
- [x] “历史概览”改为“历史”。
- [x] “数据”保持不变，副标题简化为“查看趋势与长期变化”。
- [x] “关于”保持不变。

### 5.2 今天页

- [x] “正在追踪: {app}”改为“正在追踪：{app}”。
- [x] “静候活动中”改为“暂无活动”。
- [x] “处于挂机状态”改为“无操作”。
- [x] “今日能量脉冲”改为“今日活动”。
- [x] “先去专注一会儿吧...”改为“暂无今日记录”。
- [x] “前 {count} 位”改为“前 {count} 项”。

### 5.3 历史页

- [x] “专注时间流”改为“时间线”。
- [x] “当日能量脉冲”改为“当日活动”。
- [x] “当天暂无记录”改为“这一天暂无记录”。
- [x] “{count} 次活动”改为“{count} 段活动”。
- [x] 日历按钮 aria-label 保持明确：上个月、下个月。

### 5.4 数据页

- [x] “活跃趋势”改为“活动趋势”。
- [x] “活跃热力图”改为“活动热力图”。
- [x] “每日活跃强度”改为“每日活动强度”。
- [x] “应用趋势”保持，说明改为“查看各应用的每日时长”。
- [x] “峰值日”保留。
- [x] “少 / 多”保留为热力图图例。

### 5.5 设置页

- [x] “定制你的追踪偏好与数据行为”改为“调整追踪、常驻、数据和外观设置”。
- [x] “追踪策略”改为“追踪”。
- [x] “连续关注窗口”改为“合并间隔”。
- [x] “持续参与窗口”改为“持续计入时间”。
- [x] “专注时间流最少时长”改为“时间线最短时长”。
- [x] “性能与同步”改为“性能”。
- [x] “UI 刷新频率”改为“刷新间隔”。
- [x] “数据管理”保留为清理分组，备份区域使用“数据安全”。
- [x] “未分类应用微调”改为“未分类应用”。

### 5.6 应用页

- [x] “分类控制”改为“管理分类”。
- [x] “在这里新建分类并调整分类主色”改为“新建分类，调整分类颜色”。
- [x] “+ 新建分类”改为“新建分类”。
- [x] “不记标题”改为“不记录标题”。
- [x] “统计中”改为“计入统计”。
- [x] “不统计”改为“不计入统计”。
- [x] “恢复该应用进入统计”改为“重新计入统计”。
- [x] “恢复该应用默认识别”改为“恢复默认识别”。
- [x] “删除应用记录”保留，并继续使用危险操作确认弹窗。

### 5.7 更新与关于

- [x] “尚未检查更新”改为“未检查更新”。
- [x] “发现新版本：{version}”保留。
- [x] “新版本已就绪，确认后将先下载更新包。”改为“新版本可用。确认后开始下载。”
- [x] “更新包已准备完成，确认后将重启并安装。”改为“更新包已下载。确认后重启安装。”
- [x] “处理中...”统一进入 `UI_TEXT.common.processing` / `UI_TEXT.update.processing`。
- [x] “改为手动下载”改为“手动下载”。
- [x] “当前版本：v{version}”保留。
- [x] “查看最新发布说明，或提交使用反馈。”改为“查看发布说明，或提交反馈。”

### 5.8 数据安全与备份

- [x] “生成当前数据快照”改为“导出当前数据快照”。
- [x] “从备份文件回滚数据”改为“从备份恢复数据”。
- [x] “恢复会覆盖当前统计、应用映射和缓存图标。”保留，保持风险明确。
- [x] “备份恢复失败，已自动回滚，不会破坏当前数据。”保留，属于信任文案。
- [x] “备份不兼容：未知原因”改为“备份不兼容：无法确认兼容性”。

### 5.9 挂件与窗口控制

- [x] “打开主窗口”保留，全项目继续使用“主窗口”。
- [x] “收起悬浮窗 / 展开悬浮窗”改为“收起挂件 / 展开挂件”。
- [x] “当前应用：{app}”保留。
- [x] 标题栏 aria-label 保持“最小化窗口 / 最大化窗口 / 还原窗口 / 关闭窗口”。

## 6. 英文翻译准备结果

- [x] 增加 `common / accessibility / toast / update / backup / categories` 等语义分组。
- [x] 将更新状态拆为 `title / detail / actionLabel` 所需的独立键。
- [x] 将 toast 单独建键，不复用弹窗正文。
- [x] 将 aria-label 单独建键，不复用可见按钮文案。
- [x] 将分类长名、短名和自定义兜底收口到 `UI_TEXT.categories`。
- [x] 将日期短星期、热力图星期、月份标签收口到 `UI_TEXT.date`。
- [x] 将 Rust 备份兼容结果增加 `compatibility_message_key` 和 `compatibility_message_args`。
- [x] 暂未引入 `COPY.zhCN / COPY.enUS` 和 `getUiText(locale)`，因为当前任务是中文文案优化与翻译准备，不引入语言切换行为。

## 7. UI 不破坏检查

- [x] 通过 `npm run test:ui-smoke` 确认主导航和 Dashboard SSR 可渲染。
- [x] 通过 `npm run build` 确认所有页面、懒加载 chunk 和类型检查通过。
- [x] 通过 `npm run check:bundle` 确认文案收口没有破坏 bundle 预算。
- [x] 按 Quiet Pro 约束保持按钮短文案，未新增颜色、半径、阴影或装饰样式。
- [x] 应用卡片、设置页、更新弹窗、挂件只替换文案和文案入口，不改布局行为。

## 8. 验证命令

- [x] 运行 `rg "[\\p{Han}]" src src-tauri/src`，确认除 `UI_TEXT` 外只剩真实应用名、标题清洗规则、tracking 规则 fixture、Rust 备份兼容中文 fallback。
- [x] 运行 `npm test`。
- [x] 运行 `npm run test:replay`。
- [x] 运行 `npm run build`。
- [x] 运行 `npm run check`。
- [x] 涉及 Rust 备份提示改造，已追加 `npm run check:rust`。

## 9. 最终改动说明

- [x] 新增并扩展 `src/shared/copy/uiText.ts`，把全局操作、可访问标签、toast、更新、备份、设置、应用、分类和日期文案统一收口。
- [x] 将今天、历史、数据、设置、应用、关于、更新弹窗、挂件、标题栏和共享控件中的散落中文迁入 `UI_TEXT`。
- [x] 将高频页面文案改为更安静、直接、桌面工具化的表达，例如“应用”“时间线”“今日活动”“无操作”“合并间隔”“持续计入时间”。
- [x] 将更新流程文案拆成可复用的语义键，避免后续英文翻译时受中文语序和按钮长度限制。
- [x] 将备份预览摘要迁入前端文案表，并为 Rust 备份兼容结果补充 message key 与参数，保留原中文 message 以兼容现有 UI。
- [x] 更新 `tests/uiSmoke.test.ts` 中的导航断言，匹配新的导航文案。
- [x] 未更改页面布局、样式 token、交互流程、数据统计逻辑或 tracking 逻辑。

## 10. 完成标准

- [x] 所有用户可见文案有明确归属，少数保留项已在“保留内容”说明。
- [x] 高频页面文案统一、简洁、可扫读。
- [x] 高风险操作说明清楚，不含模糊后果。
- [x] 中文文案没有 mojibake。
- [x] 后续新增英文文案不需要先拆组件结构。
- [x] 英文长度风险已通过短按钮、独立 aria-label、独立状态文案降低。
- [x] 本文档保留在 `docs/working/` 作为本轮执行记录；任务完全结束后可移入 `docs/archive/`。
