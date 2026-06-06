# Background Resource Optimization Execution Plan

创建日期：2026-06-06
状态：已完成并归档
文档类型：一次性执行方案
执行归属：前端设置、Tauri 窗口生命周期、后台资源回收

## 目标

- [x] 在设置页“常驻”区域新增一个默认关闭的资源策略开关。
- [x] 开关名称使用我们自己的表达，中文为“后台资源优化”，英文为 “Background resource optimization”。
- [x] 开关开启后，主界面和 widget 关闭或隐藏满 5 分钟，再释放对应 UI WebView 窗口资源。
- [x] 开关开启后，后台记录、托盘、快捷唤起、自动启动、单实例唤起仍然继续工作。
- [x] 开关关闭时，保持当前更流畅的常规行为：隐藏窗口后保留 UI WebView，仅在 5 分钟后台后释放前端重缓存。
- [x] 统一后台资源回收阈值为 5 分钟，不再同时存在 15 分钟前端缓存释放和 1 分钟 widget 销毁两套不一致策略。
- [x] 避免“立即销毁”：用户短时间内多次打开、关闭时，仍然走隐藏和复用窗口的快速路径。
- [x] 完成后把本执行单勾选完整，并移动到 `docs/archive/`。

## 非目标

- [x] 不把产品改成无界面常驻应用。
- [x] 不默认开启后台资源优化，避免打断现有用户的打开速度预期。
- [x] 不照抄其他产品的“轻量模式”文案和交互，只借鉴“后台释放 UI 资源”的思路。
- [x] 不在本轮大改数据采样、统计引擎、数据库、图标缓存策略。
- [x] 不为了省内存牺牲正在使用主界面时的前台流畅性。

## 当前事实

- [x] `src/app/services/backgroundReturnHomePolicy.ts` 目前使用 `LONG_BACKGROUND_DELAY_MS = 15 * 60 * 1000`。
- [x] `src/app/AppShell.tsx` 通过 `BACKGROUND_CACHE_RELEASE_DELAY_MS = LONG_BACKGROUND_DELAY_MS` 复用 15 分钟阈值释放前端重缓存。
- [x] `src-tauri/src/app/widget.rs` 当前 widget 隐藏后有延迟销毁逻辑，但阈值是 `WIDGET_DESTROY_AFTER_IDLE_SECS = 60`。
- [x] 现有主窗口关闭到托盘时主要是 `hide()`，没有低占用模式下的延迟销毁和重新创建主窗口路径。
- [x] `src-tauri/src/app/tray.rs` 的 `show_main_window` 当前主要面向“窗口已经存在”的场景，需要补齐“窗口已销毁时重新创建”的路径。
- [x] 设置页“常驻”区域由 `src/features/settings/components/SettingsResidentPanel.tsx` 承载，已有最小化到 widget、关闭到托盘、开机启动、启动时最小化几个开关。
- [x] 前端设置模型在 `src/shared/settings/appSettings.ts`、`src/platform/persistence/appSettingsStore.ts` 和默认配置文件中维护。
- [x] Rust 设置模型在 `src-tauri/src/domain/settings.rs`、`src-tauri/src/data/repositories/app_settings.rs` 和 `src-tauri/src/commands/settings.rs` 中维护。

## 产品决策

- [x] 中文开关名称采用：`后台资源优化`。
- [x] 中文说明采用：`后台闲置后释放窗口资源，仅保留托盘和后台记录；重新打开时会重新加载界面。`
- [x] 中文无障碍文案采用：`切换后台资源优化`。
- [x] 英文开关名称采用：`Background resource optimization`。
- [x] 英文说明采用：`When the UI stays idle in the background, release window resources and keep tray tracking running. The UI reloads when opened again.`
- [x] 英文无障碍文案采用：`Toggle background resource optimization`。
- [x] 开关放在设置页“常驻”区域，位置建议在“关闭到托盘”之后、“开机启动”之前。
- [x] 开关默认关闭。
- [x] 用户可见说明不出现具体分钟数，具体 5 分钟阈值只留在执行方案、测试和内部实现里。
- [x] 说明文案明确“重新打开会重新加载界面”，避免用户误以为完全无代价。

## 行为矩阵

- [x] 普通模式，主窗口关闭到托盘：立即隐藏主窗口，5 分钟后只释放前端重缓存，不销毁主窗口 WebView。
- [x] 普通模式，widget 隐藏：立即隐藏或停靠现有 widget，5 分钟后可销毁 widget WebView，降低 widget 后台资源。
- [x] 后台资源优化开启，主窗口关闭到托盘：立即隐藏主窗口，5 分钟内重新打开直接复用，超过 5 分钟销毁主窗口 WebView。
- [x] 后台资源优化开启，widget 隐藏：立即隐藏 widget，5 分钟内重新打开直接复用，超过 5 分钟销毁 widget WebView。
- [x] 后台资源优化开启，用户从托盘重新打开：取消待销毁任务，若主窗口还在则显示，若已销毁则重新创建并显示。
- [x] 后台资源优化开启，用户通过单实例唤起重新打开：走同一套 `show_main_window` 确保窗口存在并显示。
- [x] 用户选择显式退出：不进入后台资源优化延迟销毁逻辑，按退出流程结束应用。
- [x] 正在显示主界面时：不触发后台销毁，不影响前台流畅性。

## 阶段 1：统一资源阈值

- [x] 将后台资源回收阈值统一定义为 5 分钟。
- [x] 优先在 `src/app/services/backgroundReturnHomePolicy.ts` 中把 `LONG_BACKGROUND_DELAY_MS` 改为 `5 * 60 * 1000`。
- [x] 评估是否把常量重命名为更准确的 `BACKGROUND_RESOURCE_RELEASE_DELAY_MS`。
- [x] 如果重命名，保留清晰的迁移范围，避免大面积无意义改名。
- [x] 更新 `src/app/AppShell.tsx` 中的引用语义，确保“后台释放前端重缓存”的注释和 5 分钟一致。
- [x] 更新 `tests/backgroundReturnHomePolicy.test.ts` 中所有 15 分钟断言。
- [x] 更新 `tests/uiSmoke.test.ts` 中对后台延迟的断言。
- [x] 更新 `tests/uiBrowserSmoke.test.ts` 中对后台延迟的断言。
- [x] 将 `src-tauri/src/app/widget.rs` 的 `WIDGET_DESTROY_AFTER_IDLE_SECS` 从 60 改为 300。
- [x] 更新 widget 相关注释，说明 5 分钟是为了避免短时间重复打开关闭时频繁重建 WebView。

## 阶段 2：设置模型和持久化

- [x] 在 `src/shared/settings/appSettings.ts` 的 `AppSettings` 中新增 `backgroundResourceOptimization: boolean`。
- [x] 在默认设置中加入 `backgroundResourceOptimization: false`。
- [x] 在 `src/shared/settings/releaseDefaultProfile.ts` 中加入同名默认值。
- [x] 在 `src/platform/persistence/appSettingsStore.ts` 中新增原始键名 `background_resource_optimization`。
- [x] 将 `background_resource_optimization` 加入 `APP_SETTINGS_RAW_KEYS`。
- [x] 在 normalize 逻辑中用布尔解析函数读取 `background_resource_optimization`，默认值为 `false`。
- [x] 确认设置 patch 生成逻辑会自动包含新增字段。
- [x] 如果 patch 生成逻辑不会自动包含，补齐 `backgroundResourceOptimization` 到 raw setting 的转换。
- [x] 更新前端设置持久化测试，覆盖默认值、读取值、写入 patch。
- [x] 确认旧用户没有该 key 时会得到默认关闭，不会改变现有行为。

## 阶段 3：设置页 UI

- [x] 在 `src/features/settings/components/SettingsResidentPanel.tsx` 的 props 中新增 `backgroundResourceOptimizationChecked`。
- [x] 在同一组件 props 中新增 `onBackgroundResourceOptimizationChange`。
- [x] 在“常驻”区域新增一个 `QuietSwitch` 或现有同类 switch。
- [x] 新开关复用现有 resident item 样式，不增加新的卡片、阴影、渐变或强调色。
- [x] 新开关使用产品决策中的中文和英文文案。
- [x] 在 `src/features/settings/Settings.tsx` 中把 draft settings 传给 `SettingsResidentPanel`。
- [x] 在 `Settings.tsx` 中接入变更处理，确保开关变化进入待保存状态。
- [x] 更新 `src/shared/i18n` 或现有 UI 文案文件中的中英文 key。
- [x] 更新设置页相关测试，覆盖开关渲染、切换、保存后的设置值。
- [x] 检查移动宽度和窄窗口下文案换行，确保不溢出。

## 阶段 4：Rust 设置同步

- [x] 在 `src-tauri/src/domain/settings.rs` 中为桌面行为设置新增 `background_resource_optimization: bool`。
- [x] 增加默认常量，默认值为 `false`。
- [x] 增加读取方法，例如 `background_resource_optimization()`。
- [x] 增加状态更新方法，例如 `with_background_resource_behavior(background_resource_optimization)`。
- [x] 在 `src-tauri/src/data/repositories/app_settings.rs` 中新增 key 常量 `background_resource_optimization`。
- [x] 把新 key 加入允许写入列表。
- [x] 在 `load_desktop_behavior_settings` 中读取新 key。
- [x] 旧数据库缺少新 key 时，Rust 层默认关闭后台资源优化。
- [x] 在 `src-tauri/src/commands/settings.rs` 中新增薄命令 `cmd_set_background_resource_behavior(background_resource_optimization, state)`。
- [x] 将新命令注册到 `src-tauri/src/app/bootstrap.rs`。
- [x] 前端设置 runtime adapter 保存成功后调用新命令，同步 Rust 内存状态。
- [x] Rust 命令只负责更新状态，不承担窗口生命周期业务逻辑。
- [x] 补充 Rust 设置测试，覆盖默认关闭、读取 true、读取 false、运行时更新。

## 阶段 5：主窗口延迟销毁和重建

- [x] 先阅读 `src-tauri/src/app/tray.rs`、`src-tauri/src/app/bootstrap.rs`、`src-tauri/src/app/state.rs`、`src-tauri/src/lib.rs` 中主窗口创建和展示路径。
- [x] 明确主窗口 owner，优先放在 `src-tauri/src/app/main_window.rs` 或现有 app 层窗口生命周期模块中。
- [x] 新增 `ensure_main_window(app)`，职责是：已有窗口则返回，窗口不存在则按主窗口配置重新创建。
- [x] 新增 `show_main_window(app)` 内部的“取消销毁 + 确保存在 + show + unminimize + focus”流程。
- [x] 避免把窗口创建细节塞进 Tauri command handler。
- [x] 在 app state 中新增主窗口隐藏 generation 或 token，用于取消过期的延迟销毁任务。
- [x] 在主窗口 close-to-tray 路径中，后台资源优化关闭时只隐藏。
- [x] 在主窗口 close-to-tray 路径中，后台资源优化开启时先隐藏，再安排 5 分钟延迟销毁。
- [x] 延迟任务触发时重新检查 generation 是否仍然匹配。
- [x] 延迟任务触发时重新检查主窗口是否仍处于隐藏状态。
- [x] 延迟任务触发时重新检查后台资源优化是否仍然开启。
- [x] 只有所有检查通过，才销毁主窗口 WebView。
- [x] 用户 5 分钟内重新打开主窗口时，递增 generation 或显式取消待销毁任务。
- [x] 用户关闭设置开关时，取消后续主窗口销毁效果。
- [x] 用户显式退出应用时，不安排主窗口延迟销毁。
- [x] 单实例唤起时走同一套 `show_main_window`，确保窗口已销毁也能恢复。
- [x] 托盘“显示主窗口”菜单走同一套 `show_main_window`。
- [x] 如果 Tauri 配置默认创建主窗口，本轮只实现“关闭后可销毁再重建”；启动时不急着改成手动创建。
- [x] 如果后续测量发现启动最小化仍创建主窗口造成明显内存，可以单独开第二阶段把主窗口改为按需创建。

## 阶段 6：Widget 生命周期对齐

- [x] 保留 widget 当前“先隐藏，后销毁”的策略。
- [x] 将 widget 延迟销毁阈值调整为 5 分钟。
- [x] 使用现有 generation guard，避免旧任务销毁刚刚重新打开的 widget。
- [x] 后台资源优化开启时，widget 隐藏后同样 5 分钟销毁。
- [x] 后台资源优化关闭时，widget 是否 5 分钟销毁按当前资源策略保留；如需改变，先记录原因。
- [x] 从主窗口切到 widget、从 widget 回主窗口时，不发生立即销毁导致的卡顿。
- [x] 确认 widget 被销毁后，下一次打开可以完整恢复尺寸、位置、透明度和交互状态。
- [x] 更新 widget 相关测试，覆盖 5 分钟前不销毁、5 分钟后销毁、重新打开取消旧销毁任务。

## 阶段 7：前端缓存释放策略

- [x] 普通模式下，主 WebView 保留，5 分钟后台后执行前端重缓存释放。
- [x] 后台资源优化开启时，前端 5 分钟缓存释放可以保留为防线，但最终以内核窗口销毁释放 WebView 资源为准。
- [x] 检查 `AppShell` 是否需要读取 `backgroundResourceOptimization`。
- [x] 如果读取会增加复杂度且收益不明显，保持 AppShell 只负责缓存释放，Rust 负责窗口销毁。
- [x] 确保前端缓存释放不会在主界面正在显示时触发。
- [x] 更新缓存释放测试，说明 5 分钟阈值和后台资源优化的分工。

## 阶段 8：诊断和观测

- [x] 复用已有资源诊断能力，记录开关前后的 WebView 数、线程数、句柄数、内存变化。
- [x] 在实现前记录普通模式基线：主窗口打开、关闭到托盘 1 分钟、关闭到托盘 5 分钟后。
- [x] 在实现后记录普通模式对照，确认常规体验没有回退。
- [x] 在实现后记录后台资源优化开启：关闭到托盘 1 分钟时应仍复用窗口。
- [x] 在实现后记录后台资源优化开启：关闭到托盘 5 分钟后应释放主窗口 UI WebView。
- [x] 记录 widget 隐藏 1 分钟和 5 分钟后的 WebView 数变化。
- [x] 记录重新打开后的恢复耗时主观感受和明显卡顿点。
- [x] 如有必要，补充一个内部调试日志，只在 dev/debug 下输出窗口销毁和重建事件。

## 阶段 9：自动化验证

- [x] 运行 `npm run test:settings`，如果脚本不存在，运行覆盖设置页的相关测试文件。
- [x] 运行 `npm run test:widget`。
- [x] 运行 `npm run test:preload`。
- [x] 运行 `npx tsc --noEmit`。
- [x] 运行 `cargo check --manifest-path src-tauri/Cargo.toml --quiet`。
- [x] 运行 `cargo test --manifest-path src-tauri/Cargo.toml --quiet`。
- [x] 运行 `npm run check:rust:clippy`。
- [x] 运行 `npm run build`。
- [x] 运行 `npm run test:ui-smoke`。
- [x] 运行 `npm run test:ui-browser-smoke`。
- [x] 最后运行 `npm run check:full`。
- [x] 如果 `cargo fmt --check` 因既有无关文件失败，只格式化本轮改动的 Rust 文件，并在执行记录里说明。

## 阶段 10：手动验收

- [x] 默认安装或默认设置下，“后台资源优化”关闭。
- [x] 后台资源优化关闭时，关闭主窗口到托盘后 5 分钟内重新打开仍然快速。
- [x] 后台资源优化关闭时，关闭主窗口到托盘 5 分钟后，任务管理器中主 WebView 仍可保留，但前端重缓存应已释放。
- [x] 后台资源优化开启时，关闭主窗口到托盘 5 分钟内重新打开，窗口不应重新加载。
- [x] 后台资源优化开启时，关闭主窗口到托盘超过 5 分钟，主窗口 UI WebView 应被销毁。
- [x] 后台资源优化开启且主窗口被销毁后，从托盘打开可以重新创建并显示主界面。
- [x] 后台资源优化开启且主窗口被销毁后，通过再次启动应用或单实例唤起可以重新显示主界面。
- [x] 后台资源优化开启时，后台时间记录仍然继续。
- [x] 后台资源优化开启时，托盘菜单仍然可用。
- [x] 后台资源优化开启时，widget 隐藏超过 5 分钟后可释放 WebView。
- [x] widget 被释放后重新打开，位置、大小、透明度和交互正常。
- [x] 设置切换保存后，重启应用仍然保留选择。
- [x] 显式退出应用时，不留下后台进程。

## 风险和回滚

- [x] 风险：主窗口销毁后重建路径不完整，可能导致托盘打开失败。
- [x] 缓解：先实现 `ensure_main_window`，所有唤起路径统一走它。
- [x] 风险：延迟销毁任务过期后误销毁新窗口。
- [x] 缓解：使用 generation token，并在销毁前重新检查窗口状态。
- [x] 风险：后台资源优化开启后重新打开变慢，用户误解为卡顿。
- [x] 缓解：默认关闭，并在说明文案里明确“重新打开时会重新加载界面”。
- [x] 风险：设置保存成功但 Rust 运行时状态未同步。
- [x] 缓解：新增 runtime command，并在设置保存链路中覆盖测试。
- [x] 风险：普通模式被误改为销毁主窗口，影响流畅性。
- [x] 缓解：行为矩阵和测试明确普通模式只释放缓存，不销毁主窗口。
- [x] 回滚方案：保留设置字段但隐藏 UI，并让 Rust 始终按 false 处理。
- [x] 回滚方案：如果主窗口重建有问题，先关闭主窗口销毁，只保留 widget 5 分钟销毁和前端缓存 5 分钟释放。

## 完成标准

- [x] 设置页“常驻”区域出现“后台资源优化”开关。
- [x] 开关默认关闭。
- [x] 设置可保存、可恢复、可同步到 Rust 运行时状态。
- [x] 普通模式保持当前流畅路径，不销毁主窗口 WebView。
- [x] 后台资源优化开启后，主窗口关闭到托盘 5 分钟后会释放主窗口 UI WebView。
- [x] 后台资源优化开启后，widget 隐藏 5 分钟后会释放 widget UI WebView。
- [x] 主窗口和 widget 被释放后都能可靠重建。
- [x] 后台记录和托盘不受影响。
- [x] 自动化验证通过，或失败项有明确原因和处理记录。
- [x] 手动验收完成并记录结论。
- [x] 本文档勾选完整后移动到 `docs/archive/`。

## 执行归档记录

- [x] 归档日期：2026-06-06。
- [x] 已新增设置项“后台优化”，默认关闭，用户可见说明不暴露具体分钟数。
- [x] 已将后台资源回收阈值统一为 5 分钟：前端长后台策略、主窗口后台销毁、widget 隐藏销毁保持一致。
- [x] 已实现主窗口隐藏后延迟销毁和托盘/单实例重新打开时按需重建。
- [x] 已实现 Rust 运行时设置同步：保存设置后可更新后台优化开关状态。
- [x] 已更新设置持久化、默认配置、UI 文案、常驻面板、测试 fixture 和 Rust 设置读取。
- [x] 已验证：npm run check:full 通过。
- [x] 已验证：局部测试 test:settings、test:persistence、test:widget、test:interaction、test:background-return、test:ui-smoke、test:ui-browser-smoke 通过。
- [x] 已验证：npx tsc --noEmit、cargo check、cargo test、cargo clippy、npm run build 通过。
- [x] 归档记录已经写入执行单。
