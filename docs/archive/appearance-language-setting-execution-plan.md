# 外观语言设置执行计划

## 1. 目标

- [x] 在设置页「外观」区域新增「语言」设置。
- [x] 支持用户在中文与英文之间选择界面语言。
- [x] 默认语言保持中文，不破坏现有中文体验。
- [x] 语言切换后不破坏 Quiet Pro 布局、密度和控件节奏。
- [x] 为后续继续扩展语言留下稳定结构。

## 2. 产品边界

- [x] 只做本地界面语言，不引入账号、云同步或在线翻译。
- [x] 语言设置作为本地应用设置保存。
- [x] 不改变日期、数字、时长统计逻辑。
- [x] 不改变追踪、备份、恢复、更新安装等行为。
- [x] 不做系统语言自动检测，默认仍为中文。

## 3. UI 执行

- [x] 保持「外观」面板位置和 Quiet Pro 风格不变。
- [x] 语言设置放在外观区域最下面，顺序为：主题模式 -> 配色方案 -> 语言。
- [x] 语言行使用与主题模式一致的双栏布局。
- [x] 左侧为标题与说明，右侧为分段控件。
- [x] 中文文案：`语言` / `选择界面显示语言。`
- [x] 英文文案：`Language` / `Choose the interface language.`
- [x] 选项使用自称：`中文` / `English`。
- [x] 复用现有 `QuietSegmentedFilter`，未新增局部视觉样式。

## 4. 数据结构与持久化

- [x] 在 `AppSettings` 增加 `language` 字段。
- [x] 类型为 `"zh-CN" | "en-US"`。
- [x] 默认值为 `"zh-CN"`。
- [x] 在 release default profile 中加入默认语言。
- [x] 在 settings persistence 中加入 raw key：`language`。
- [x] 增加语言归一化：合法英文保存为 `en-US`，缺失或非法值回退 `zh-CN`。
- [x] 设置 patch、保存、读取和事件通知均支持该字段。

## 5. 文案系统执行

- [x] 将 `UI_TEXT` 改造为双语 copy 结构。
- [x] 增加 `COPY["zh-CN"]` 和 `COPY["en-US"]`。
- [x] 增加 `getUiText(language)` 与 `setUiTextLanguage(language)`。
- [x] 保留 `UI_TEXT` 兼容导出，降低本次改动范围。
- [x] 补齐英文界面文案，保持中英文 key 结构一致。
- [x] 将挂件文案纳入 `UI_TEXT.widget`，避免硬编码中文。
- [x] 处理侧边栏、日历星期、热力图星期、分类标签等模块级文案，避免语言切换后被旧文案锁住。

## 6. 运行时接入

- [x] 主窗口根据 `appSettings.language` 设置当前 UI 文案。
- [x] 挂件根据 `appSettings.language` 设置当前 UI 文案。
- [x] 点击语言选项后立即预览主窗口文案。
- [x] 取消编辑或离开设置页时恢复已保存语言。
- [x] 保存设置后通过既有 settings changed 流程固化语言，并同步主窗口与挂件。

## 7. 翻译范围

- [x] 导航、Dashboard、History、Data、应用、Settings、About 已接入双语文案结构。
- [x] Update 面板、弹窗、toast、确认框文案已纳入英文 copy。
- [x] 共享控件 aria-label 已纳入英文 copy。
- [x] 挂件可见文案和 aria-label 已纳入英文 copy。
- [x] 真实应用名、用户自定义分类名、窗口标题规则、追踪识别规则不自动翻译。
- [x] CHANGELOG 和发布记录不随界面语言自动翻译。

## 8. 测试与验证

- [x] 更新 `tests/settingsPageState.test.ts`，覆盖语言默认值、合法值、非法值回退和 patch。
- [x] 更新 `tests/persistenceTransaction.test.ts`，覆盖 `language` 持久化 key。
- [x] 更新 `tests/widgetViewModel.test.ts` 和相关 AppSettings fixture。
- [x] 更新 `tests/uiSmoke.test.ts`，增加中英文 copy key 对齐测试。
- [x] 运行 `npm test`。
- [x] 运行 `npm run test:replay`。
- [x] 运行 `npm run test:settings`。
- [x] 运行 `npm run test:widget`。
- [x] 运行 `npm run test:persistence`。
- [x] 运行 `npm run test:interaction`。
- [x] 运行 `npm run test:ui-smoke`。
- [x] 运行 `npm run build`。
- [x] 运行 `npm run check`。
- [x] 本次未改 Rust 设置仓储，无需运行 `npm run check:rust`。

## 9. 最终改动说明

- [x] `src/shared/settings/appSettings.ts`：新增 `AppLanguage` 与 `AppSettings.language`。
- [x] `src/shared/settings/releaseDefaultProfile.ts`：默认语言设为 `zh-CN`。
- [x] `src/platform/persistence/appSettingsStore.ts`：新增 `language` raw key、归一化和保存支持。
- [x] `src/shared/copy/uiText.ts`：建立中英文 copy 包、运行时读取入口和兼容 `UI_TEXT` 导出。
- [x] `src/features/settings/components/SettingsAppearancePanel.tsx`：在外观区最下面新增语言分段控件。
- [x] `src/features/settings/components/Settings.tsx`：把语言设置接入 draft、预览和保存流程。
- [x] `src/app/AppShell.tsx`、`src/app/widget/WidgetShell.tsx`：按当前设置或设置页预览状态同步 UI 文案语言。
- [x] `src/app/components/AppSidebar.tsx`、`src/features/history/components/History.tsx`、`src/features/data/components/Data.tsx`、`src/shared/classification/categoryTokens.ts`：移除会锁定旧语言的模块级文案读取。
- [x] `src/app/widget/widgetViewModel.ts`：挂件状态文案改为从 `UI_TEXT.widget` 读取。
- [x] 相关测试 fixture 已补齐 `language` 字段。

## 10. 验证结果

- [x] `npm run build` 通过。
- [x] `npm run check` 通过。
- [x] `npm run test:ui-smoke` 首次在沙箱内因 esbuild 子进程 `EPERM` 失败；已按规则在沙箱外重跑并通过。
- [x] 中英文 copy key 结构已由 UI smoke 测试校验一致。
- [x] 外观区域语言控件位置已按反馈调整到最下面。
- [x] 语言选项已支持未保存预览，点击 `English` 可立即看到英文界面。
