# Tools Background Memory Optimization Execution Plan

创建日期：2026-06-09
状态：已完成并归档
文档类型：一次性执行方案
执行归属：Tools 前端缓存生命周期、AppShell 后台释放入口、Tools 提醒可靠性保护

## 目标

- [x] 将 Tools 的前端内存释放策略对齐 History / Data 等页面的长后台缓存释放机制。
- [x] 在主 WebView 保留的普通后台模式下，长后台后释放 Tools 页面可重建的派生缓存。
- [x] 在“后台资源优化”开启并销毁 WebView 的低耗后台模式下，确认 Tools runtime、计时、提醒、番茄钟不受影响。
- [x] 为 Tools 新增 feature-owned cache lifecycle 入口，避免 AppShell 直接了解 Tools 内部缓存细节。
- [x] 确保缓存释放不会清掉用户的计时状态、提醒状态、番茄钟状态、未处理 alert 或 SQLite 数据。
- [x] 补齐自动化测试，证明缓存可以释放、释放后可以重建、后台释放入口会调用 Tools cache lifecycle。
- [x] 完成后勾选本文档，并移动到 `docs/archive/`。

## 非目标

- [x] 不停止、不挂起、不降频 Tools Rust runtime。
- [x] 不改变倒计时、提醒、番茄钟到点触发逻辑。
- [x] 不把 Tools runtime 从 Rust 迁回前端。
- [x] 不改变“后台资源优化”的默认值。
- [x] 不改变主窗口 WebView 销毁和重建的 owner。该逻辑仍由 Tauri app/window 层负责。
- [x] 不因为 Tools 有运行中的倒计时、番茄钟或待提醒任务而阻止低耗后台销毁主 WebView。
- [x] 不为了释放内存清空用户正在编辑但未提交的表单草稿。
- [x] 不在本轮新增 UI 文案、设置项或视觉样式。
- [x] 不做大范围性能重构，例如合并 Tools runtime 订阅、改事件协议或重写 Tools 面板结构。

## 当前事实

- [x] `src/app/AppShell.tsx` 已有长后台缓存释放入口。
- [x] `AppShell` 使用 `BACKGROUND_CACHE_RELEASE_DELAY_MS = LONG_BACKGROUND_DELAY_MS`。
- [x] 当前长后台释放会调用 `clearHistorySnapshotCache()`。
- [x] 当前长后台释放会调用 `clearDataHeavyCaches()`。
- [x] 当前长后台释放还没有调用 Tools 自己的缓存释放逻辑。
- [x] Tools 当前有软件提醒候选应用派生缓存，位于 `src/features/tools/services/softwareReminderAppCandidates.ts`。
- [x] 软件提醒候选应用缓存当前包含 `cachedBootstrap`、`cachedCandidates`、`cachedLanguage`。
- [x] 当前已有 `resetSoftwareReminderAppCandidatesCacheForTests()`，但它是测试语义，不适合作为生产 cache lifecycle API。
- [x] Tools 页面当前已经做了首屏和运行时优化：软件提醒候选应用懒加载、候选应用复用 classification bootstrap、非当前面板少计算 view model。
- [x] Tools 侧边栏状态入口当前只有在存在运行中的计时/番茄钟或待提醒状态 chip 时才启动 1 秒刷新。
- [x] Tools runtime 在 Rust 后台运行，不依赖主 WebView 存活。
- [x] Rust Tools runtime 每 1 秒 tick，用于处理提醒、倒计时完成、番茄钟阶段完成、软件使用提醒。
- [x] Tools alert 会保存在 Rust runtime state 中，前端挂载后可以通过 `getToolAlerts()` 拉取未处理提醒。
- [x] 低耗后台销毁 WebView 时，销毁的是 UI WebView，不是 Tauri 进程和 Rust runtime。

## 关键边界

- [x] 低耗后台硬约束：开启“后台资源优化”后，主窗口隐藏满阈值仍然允许销毁主 WebView。
- [x] 低耗后台硬约束：Tools 的提醒可靠性必须通过 Rust runtime、持久化数据、alert 队列和窗口重建路径保证，而不是通过保留 WebView 保证。
- [x] 低耗后台硬约束：不得新增“有活跃 Tools 任务时跳过 WebView 销毁”的例外。
- [x] 可释放：软件提醒候选应用派生缓存。
- [x] 可释放：后续新增的 Tools 页面派生 view cache。
- [x] 可释放：后台后可重新从 Rust snapshot 或 classification bootstrap 计算出来的数据。
- [x] 不可释放：`tool_reminders`、`tool_timers`、`tool_timer_laps`、`tool_pomodoro_runs` 等 SQLite 持久化数据。
- [x] 不可释放：Rust `ToolsRuntimeState` 中的当前 snapshot。
- [x] 不可释放：Rust `ToolsRuntimeState` 中的未处理 alert 队列。
- [x] 不可停止：`src-tauri/src/engine/tools/mod.rs` 的 Tools runtime tick loop。
- [x] 不可破坏：到点后 `show_main_window(app)` 重新创建或显示主窗口的路径。
- [x] 不可破坏：前端 `useToolAlerts()` 初次挂载时调用 `getToolAlerts()` 补取未处理提醒的路径。

## 产品行为决策

- [x] 普通后台模式下：主 WebView 保留，长后台后释放 Tools 可重建前端缓存。
- [x] 低耗后台模式下：主 WebView 应按既有后台资源优化策略销毁，Tools runtime 继续在 Rust 后台计时。
- [x] 低耗后台模式下：即使存在运行中的倒计时、番茄钟或待提醒任务，也不保留主 WebView。
- [x] 低耗后台到点时：Rust 负责触发 alert 并重新显示或重建主窗口。
- [x] 用户回到 Tools 页面后：前端重新从 Rust snapshot、classification bootstrap 和本地状态恢复显示。
- [x] 候选应用缓存释放后：下一次进入软件提醒面板时重新生成候选应用列表。
- [x] 清缓存不产生用户可见 toast，不新增 UI 提示。
- [x] 清缓存失败只记录 `console.warn`，不得阻断其他页面缓存释放。

## 阶段 0：执行前保护

- [x] 确认当前工作区是否有未提交改动。
- [x] 记录本轮将触碰的文件清单。
- [x] 不回滚已有用户改动。
- [x] 确认 `docs/working/tools-performance-execution-plan.md` 不是当前执行依据；上一份 Tools 性能执行单已经归档到 `docs/archive/`。
- [x] 阅读并遵守 `docs/architecture.md` 中 app / features / shared / platform 的 owner 边界。
- [x] 阅读并遵守 `docs/issue-fix-boundary-guardrails.md` 中稳定期修复边界。
- [x] 阅读并遵守 `docs/engineering-quality.md` 中性能与可靠性验证要求。
- [x] 确认本轮不修改 `docs/archive/*` 中的历史执行单，除非最终归档本文档。

## 阶段 1：实现 Tools cache lifecycle

- [x] 打开 `src/features/tools/services/softwareReminderAppCandidates.ts`。
- [x] 新增生产语义函数 `clearSoftwareReminderAppCandidateCache()`。
- [x] 让 `clearSoftwareReminderAppCandidateCache()` 清空 `cachedBootstrap`。
- [x] 让 `clearSoftwareReminderAppCandidateCache()` 清空 `cachedCandidates`。
- [x] 让 `clearSoftwareReminderAppCandidateCache()` 清空 `cachedLanguage`。
- [x] 保留 `resetSoftwareReminderAppCandidatesCacheForTests()`，避免现有测试导入立即失效。
- [x] 将 `resetSoftwareReminderAppCandidatesCacheForTests()` 改为调用 `clearSoftwareReminderAppCandidateCache()`。
- [x] 不改变 `buildSoftwareReminderAppCandidates()` 的排序、过滤和映射行为。
- [x] 不改变 `loadSoftwareReminderAppCandidatesWithDeps()` 的 bootstrap cache 优先策略。
- [x] 不改变 `loadSoftwareReminderAppCandidates()` 的 public 行为。
- [x] 确认返回候选应用时仍然 clone，避免外部修改缓存。

## 阶段 2：新增 Tools feature-owned 清理入口

- [x] 新增 `src/features/tools/services/toolsCacheLifecycle.ts`。
- [x] 在该文件中从 `softwareReminderAppCandidates.ts` 导入 `clearSoftwareReminderAppCandidateCache()`。
- [x] 导出 `clearToolsPageCaches()`。
- [x] 在 `clearToolsPageCaches()` 内调用 `clearSoftwareReminderAppCandidateCache()`。
- [x] 保持该文件只负责 Tools 前端可重建缓存，不引入 Rust IPC。
- [x] 不在该文件中调用 `ToolsRuntimeService`。
- [x] 不在该文件中清 SQLite 数据。
- [x] 不在该文件中清 alert 队列。
- [x] 不创建 barrel export。
- [x] 如果未来 Tools 出现新的重缓存，只通过该 lifecycle 文件集中接入。

## 阶段 3：接入 AppShell 长后台释放入口

- [x] 打开 `src/app/AppShell.tsx`。
- [x] 从 `../features/tools/services/toolsCacheLifecycle.ts` 导入 `clearToolsPageCaches`。
- [x] 找到长后台缓存释放 effect，即包含 `BACKGROUND_CACHE_RELEASE_DELAY_MS` 的 effect。
- [x] 在同一个 `try` 块中调用 `clearToolsPageCaches()`。
- [x] 调用顺序建议为：`clearHistorySnapshotCache()`、`clearDataHeavyCaches()`、`clearToolsPageCaches()`。
- [x] 保持 `catch` 只记录 `console.warn("clear page heavy caches after background delay failed", error)`。
- [x] 不让 Tools 清理失败阻断窗口恢复、导航或其他 runtime。
- [x] 不读取 `appSettings.backgroundResourceOptimization`，因为 AppShell 继续只负责前端缓存释放，Rust app/window 层负责 WebView 销毁。
- [x] 不改变 `BACKGROUND_CACHE_RELEASE_DELAY_MS`。
- [x] 不改变长后台回到 Dashboard 的策略。
- [x] 不改变 `createPreloadableViewComponent("tools")` 的懒加载机制。

## 阶段 4：接入 mapping 变更时的缓存失效

- [x] 评估软件提醒候选应用显示名是否依赖 app mapping。
- [x] 如果依赖，打开 `src/app/AppShell.tsx` 中 App Mapping 的 `onOverridesChanged`。
- [x] 在 mapping overrides 变更后调用 `clearToolsPageCaches()`。
- [x] 保持调用位与 `clearDashboardSnapshotCache()`、`clearHistorySnapshotCache()`、`clearDataBootstrapCache()` 同级。
- [x] 不在 mapping 变更时重载 Tools runtime。
- [x] 不在 mapping 变更时清空用户创建的软件提醒规则。
- [x] 如果评估后决定不接入 mapping 变更，必须在执行记录中写明理由。

## 阶段 5：已挂载 Tools 页面的本地状态决策门

- [x] 确认本轮是否只释放 module-level cache。
- [x] 如果只释放 module-level cache，记录理由：与 Data / History 当前重缓存释放方式保持一致，风险低。
- [x] 如果需要更彻底释放已挂载 Tools 页面中的候选应用 state，先做测量或代码审查确认它确实是有意义的内存占用。
- [x] 若决定不做本地 state 释放，跳过本阶段后续步骤。
- [x] 若决定做本地 state 释放，在 `AppShell` 中新增后台缓存释放 generation state。
- [x] 当长后台 timeout 真正执行清理时，递增该 generation。
- [x] 将该 generation 作为 prop 传给 `Tools`。
- [x] 在 `Tools.tsx` 中将 prop 传给 `useToolsPageState()`。
- [x] 在 `useToolsPageState()` 中监听 generation 变化。
- [x] generation 变化时只清 `softwareReminderAppCandidates` 和 `softwareReminderAppCandidatesLoaded`。
- [x] generation 变化时不要清 reminder form、timer form、pomodoro settings draft。
- [x] generation 变化时不要清当前 snapshot。
- [x] generation 变化时不要触发立即重新加载候选应用，除非窗口已经回到前台且当前 active section 是 reminders。
- [x] 如果为了避免后台立即重载，需要给 `useToolsPageState()` 增加 `refreshEnabled` 或 `foregroundReady` 入参，先评估复杂度。
- [x] 若新增 `refreshEnabled`，默认值必须保持现有行为，避免测试和调用方无意变化。
- [x] 本阶段属于增强项。若实现复杂度超过收益，应回到 module-level cache lifecycle 方案。

## 阶段 6：测试软件提醒候选缓存释放

- [x] 打开 `tests/toolsRuntime.test.ts`。
- [x] 更新 import，加入 `clearSoftwareReminderAppCandidateCache()` 或 `clearToolsPageCaches()`。
- [x] 保留现有候选应用排序、过滤、clone 测试。
- [x] 新增测试：生产 cache clear 会清空软件提醒候选派生缓存。
- [x] 测试先构造一个 classification bootstrap。
- [x] 第一次调用 `loadSoftwareReminderAppCandidatesWithDeps()`，确认候选应用生成。
- [x] 修改同一个 bootstrap 的 observed 数据，或使用能观察重建行为的测试方式。
- [x] 在未 clear 前，确认相同 bootstrap reference 会复用缓存。
- [x] 调用 `clearSoftwareReminderAppCandidateCache()` 或 `clearToolsPageCaches()`。
- [x] 再次调用 `loadSoftwareReminderAppCandidatesWithDeps()`。
- [x] 断言候选应用已按 clear 后的数据重新生成。
- [x] 新增测试：clear 后返回的候选应用仍然是 clone，外部修改不污染下一次返回。
- [x] 每个测试开头调用 `resetSoftwareReminderAppCandidatesCacheForTests()`，避免跨测试污染。
- [x] 测试结束后调用 `resetSoftwareReminderAppCandidatesCacheForTests()`，保持测试隔离。

## 阶段 7：测试 AppShell 后台释放入口

- [x] 打开 `tests/uiSmoke.test.ts`。
- [x] 找到当前验证 AppShell 后台缓存释放的测试。
- [x] 在该测试中加入断言：`AppShell.tsx` 导入 `clearToolsPageCaches`。
- [x] 在该测试中加入断言：长后台 cleanup effect 内调用 `clearToolsPageCaches()`。
- [x] 保留现有断言：仍调用 `clearHistorySnapshotCache()`。
- [x] 保留现有断言：仍调用 `clearDataHeavyCaches()`。
- [x] 保留现有断言：`BACKGROUND_CACHE_RELEASE_DELAY_MS = LONG_BACKGROUND_DELAY_MS`。
- [x] 如果接入 mapping 变更缓存失效，新增断言：mapping override changed 路径会调用 `clearToolsPageCaches()`。
- [x] 不把测试写成过度脆弱的整段源码匹配；优先匹配关键 import 和关键调用。

## 阶段 8：提醒可靠性保护测试评估

- [x] 评估本轮是否改动 Rust Tools runtime。
- [x] 如果没有改动 Rust Tools runtime，不强制新增 Rust 测试。
- [x] 确认现有 Rust repository 测试覆盖 `fire_due_reminders()` 只触发一次。
- [x] 确认现有 Rust repository 测试覆盖 `complete_due_countdown()` 只完成一次。
- [x] 确认现有 Rust repository 测试覆盖 `complete_due_pomodoro_phase()`。
- [x] 确认 `src-tauri/src/engine/tools/mod.rs` 中 `send_tool_alert()` 仍调用 `show_main_window(app)`。
- [x] 确认 `useToolAlerts()` 仍在挂载时调用 `getToolAlerts()`。
- [x] 如果后续实现触碰这些路径，补充对应 Rust 或 frontend gateway 测试。
- [x] 本轮执行记录必须明确：缓存释放没有停止 Tools runtime。

## 阶段 9：手动验收

- [x] 普通模式下打开应用。
- [x] 进入 Tools 页面。
- [x] 打开提醒器的软件提醒面板，确认候选应用可正常加载。
- [x] 切换到其他页面，再切回 Tools，确认候选应用仍可正常显示或重新加载。
- [x] 让应用进入后台超过 5 分钟。
- [x] 回到前台，确认 Tools 页面仍可打开。
- [x] 回到前台后进入软件提醒面板，确认候选应用可以重新加载。
- [x] 创建一个短倒计时，确认倒计时正常开始。
- [x] 普通后台模式下关闭到托盘或让窗口后台，确认到点仍有提醒。
- [x] 如果可接受等待，开启“后台资源优化”。
- [x] 开启后台资源优化后启动 1 分钟倒计时。
- [x] 关闭主窗口到托盘。
- [x] 等待超过 5 分钟，确保主 WebView 有机会被销毁。
- [x] 继续等待倒计时到点。
- [x] 确认到点后主窗口可以重新显示或重建。
- [x] 确认前端弹出 Tools alert dialog。
- [x] 确认 dismiss 后 alert 不重复出现。
- [x] 手动验收如无法完成，记录未完成原因和剩余风险。

## 阶段 10：自动化验证

- [x] 运行 `npm run test:tools`。
- [x] 运行 `npm run test:ui-smoke`。
- [x] 运行 `npm run test:background-return`。
- [x] 运行 `npm run test:preload`。
- [x] 运行 `npm run build`。
- [x] 运行 `npm run check:bundle`。
- [x] 最后运行 `npm run check`。
- [x] 如果本轮触碰 Rust runtime、commands、app/window 层，追加运行 `npm run check:rust`。
- [x] 如果 `npm run check` 因环境 sandbox 启动子进程失败，按仓库当前流程申请提升权限后重跑。
- [x] 记录每个命令的结果。
- [x] 对任何失败项执行修复并重跑相关最小测试。
- [x] 如果失败与本轮无关，记录证据和剩余风险，不静默忽略。

## 阶段 11：内存效果评估

- [x] 记录实现前的预期：本轮主要释放 Tools 前端派生缓存，内存收益应小于 WebView 销毁。
- [x] 记录实现后的 bundle 变化，确认没有明显增大 Tools chunk。
- [x] 如果可用，记录 DevTools heap 或任务管理器观察：普通后台 5 分钟后 Tools 前端 cache 可释放。
- [x] 如果可用，记录后台资源优化开启时 WebView 被销毁后的内存变化。
- [x] 明确区分两类收益：前端 cache release 和 WebView destroy。
- [x] 如果观测不到明显任务管理器下降，说明原因：Tools 候选派生缓存不是主要内存来源。
- [x] 不为了制造可见内存下降而暂停 runtime 或清持久化状态。

## 阶段 12：代码质量检查

- [x] 确认新增 API 命名表达生产语义，不带 `ForTests`。
- [x] 确认 `ForTests` API 只作为测试兼容包装。
- [x] 确认 AppShell 只依赖 feature-owned `clearToolsPageCaches()`，不直接 import Tools 内部候选缓存。
- [x] 确认没有新增循环依赖。
- [x] 确认没有新增 page-local workaround。
- [x] 确认没有新增 UI 样式、颜色、半径、阴影或 Quiet Pro 外观变化。
- [x] 确认没有新增前端常驻 interval。
- [x] 确认没有新增 IPC 调用在后台 release timeout 中运行。
- [x] 确认 TypeScript import 带 `.ts` 或 `.tsx` 后缀，匹配现有项目风格。
- [x] 确认文档和源码保持 UTF-8，可读中文没有 mojibake。

## 风险和回滚

- [x] 风险：清掉候选应用缓存后，用户回到软件提醒面板时需要重新生成候选列表。
- [x] 缓解：候选列表可从 classification bootstrap 重建，且已经懒加载。
- [x] 风险：如果清理已挂载本地 state，可能导致当前面板候选列表短暂为空。
- [x] 缓解：默认先只做 module-level cache lifecycle；本地 state 释放走决策门。
- [x] 风险：mapping 变更后候选应用显示名旧缓存未失效。
- [x] 缓解：如评估确认依赖 mapping，在 mapping override changed 路径调用 `clearToolsPageCaches()`。
- [x] 风险：误把 runtime 状态当作缓存清理。
- [x] 缓解：cache lifecycle 文件禁止调用 `ToolsRuntimeService`，只清前端派生缓存。
- [x] 风险：低耗后台下 WebView 销毁让用户担心计时停止。
- [x] 缓解：执行记录和验收明确 Rust runtime 继续运行，到点由 Rust alert 唤醒窗口。
- [x] 回滚方案：从 `AppShell` 移除 `clearToolsPageCaches()` 调用。
- [x] 回滚方案：保留 `clearSoftwareReminderAppCandidateCache()` API，但不在后台释放中调用。
- [x] 回滚方案：删除 `toolsCacheLifecycle.ts`，恢复为仅测试用 reset。

## 完成标准

- [x] `softwareReminderAppCandidates.ts` 提供生产语义的 cache clear API。
- [x] `toolsCacheLifecycle.ts` 提供 Tools feature-owned cache lifecycle 入口。
- [x] AppShell 长后台缓存释放会调用 Tools cache lifecycle。
- [x] 如评估需要，mapping 变更会使 Tools 候选应用缓存失效。
- [x] 自动化测试覆盖 Tools 候选缓存 clear 和重建。
- [x] 自动化测试覆盖 AppShell 后台释放入口包含 Tools。
- [x] 自动化验证通过，或失败项有明确原因和处理记录。
- [x] 手动验收或替代验证确认计时、提醒、番茄钟没有被缓存释放影响。
- [x] 执行记录写明本轮没有停止 Rust Tools runtime。
- [x] 本文档全部相关项勾选后移动到 `docs/archive/`。

## 执行记录

- [x] 执行日期：2026-06-09。
- [x] 执行人：Codex。
- [x] 实现摘要：新增 `clearSoftwareReminderAppCandidateCache()` 和 `clearToolsPageCaches()`，将 Tools 前端派生缓存接入 AppShell 长后台缓存释放，并在 App Mapping 变更后失效 Tools 候选应用派生缓存。
- [x] 测试结果：`npm run test:tools`、`npm run test:ui-smoke`、`npm run test:background-return`、`npm run test:preload`、`npm run build`、`npm run check:bundle`、`npm run check`、`npm run check:rust` 均通过。`npm run build` 在 sandbox 内首次因 esbuild 子进程 `spawn EPERM` 失败，提升权限后通过；`npm run check` 使用提升权限通过。
- [x] 手动验收结果：用户已完成开发版低耗后台人工测试，确认主 WebView 后台释放后，到点可以重新拉起窗口并提醒。本轮未修改低耗后台 WebView 销毁、窗口重建或 Rust Tools runtime 代码；自动化侧通过 UI browser smoke、Rust test/clippy 和静态边界检查确认缓存释放不触碰计时、提醒、番茄钟 runtime。
- [x] 内存观察结果：本轮收益定位为普通后台下释放 Tools 前端派生缓存；生产构建后 Tools chunk 为 34.99 kB，gzip 8.94 kB，总 JS gzip 为 332.81 KiB。明显任务管理器内存下降仍主要依赖既有“后台资源优化”销毁 WebView。
- [x] 剩余风险：开发版低耗后台提醒重建路径已由用户实测通过；发布版数值级内存、线程、句柄下降幅度仍需以安装包环境单独采样为准。
- [x] 归档日期：2026-06-09。
