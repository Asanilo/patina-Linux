# Resource Performance Optimization Execution Plan

创建日期：2026-06-06

状态：completed / archived

文档类型：执行单 / How-to

适用阶段：`1.x` 稳定阶段

完成日期：2026-06-06

## 0. 归档勾选摘要

- [x] 已建立资源优化执行单。
- [x] 已优先执行不影响前台流畅性的优化项。
- [x] 已将 Windows process handle、snapshot handle、owned icon、bitmap、screen DC、memory DC 类型化为 RAII guard。
- [x] 已为高频进程信息查询增加短 TTL cache 和 negative cache。
- [x] 已为直接图标提取增加有容量上限的结果 cache 和 negative cache。
- [x] 已为 tracking runtime / watchdog restart loop 增加退避，避免连续失败时固定 2 秒重启。
- [x] 已新增只读资源诊断 command，用于读取 WebView 窗口数、当前进程线程 / 句柄数与缓存统计。
- [x] 已保留 widget park、启动暖机与透明窗口默认策略，不为了资源数字牺牲前台手感。
- [x] 已将长期执行规则回写到 `docs/engineering-quality.md`。
- [x] 已完成 Rust 测试、check 与 clippy 验证。
- [x] 已完成 TypeScript 编译检查。
- [x] 已记录沙箱限制：`npm run check:full`、`npm run build`、`npm run test:ui-browser-smoke` 在 Vite/esbuild 子进程处遇到 `spawn EPERM`；两次提权重试均因审批器超时未执行。
- [x] 已完成归档。

## 0.1 实施记录

### 优化项：Windows owned resource guard

- 状态：完成
- owner：`src-tauri/src/platform/windows/**`
- 改动范围：新增 `handles.rs`，替换 foreground 与 icon 路径中的手动释放
- 是否影响前台体验：否
- 优化前 CPU：未改变 CPU 策略
- 优化后 CPU：未改变 CPU 策略
- 优化前内存：未改变内存策略
- 优化后内存：未改变内存策略
- 优化前线程数：未改变线程策略
- 优化后线程数：未改变线程策略
- 优化前 handle count：手动释放，靠代码路径保证
- 优化后 handle count：owned resource 通过 Drop 释放
- 验证命令：`cargo test --manifest-path src-tauri/Cargo.toml --quiet`，`cargo check --manifest-path src-tauri/Cargo.toml --quiet`，`npm run check:rust:clippy`
- 手动验证：未启动 GUI 做任务管理器采样；本项为行为等价的安全收口
- 是否默认启用：是
- 回滚方式：恢复 `handles.rs` 调用点到手动释放
- 备注：不包装 borrowed window icon handle，避免误 DestroyIcon

### 优化项：Foreground process details cache

- 状态：完成
- owner：`src-tauri/src/platform/windows/foreground.rs`
- 改动范围：PID -> process details 短 TTL cache，negative cache
- 是否影响前台体验：否
- 优化前 CPU：同一 PID 可重复 `OpenProcess` / snapshot fallback
- 优化后 CPU：TTL 内同一 PID 命中 cache
- 优化前内存：无 cache
- 优化后内存：小型 PID cache
- 优化前线程数：不变
- 优化后线程数：不变
- 优化前 handle count：高频路径更频繁打开 process / snapshot handle
- 优化后 handle count：减少重复打开，且 handle RAII 释放
- 验证命令：Rust tests / check / clippy
- 手动验证：未启动 GUI 做长时间前台切换采样
- 是否默认启用：是
- 回滚方式：移除 cache read/write，恢复直接查询
- 备注：positive TTL 10s，negative TTL 1s，降低 PID 复用风险

### 优化项：Icon extraction result cache

- 状态：完成
- owner：`src-tauri/src/platform/windows/icon.rs`
- 改动范围：file icon result cache，容量上限，negative cache，GDI guard；window icon fallback 保持即时读取，避免 HWND 复用导致缓存误命中
- 是否影响前台体验：否
- 优化前 CPU：重复提取同一图标会重复走 Win32 + PNG/base64
- 优化后 CPU：TTL 内直接复用结果
- 优化前内存：无平台层结果 cache
- 优化后内存：最多 256 条结果 cache
- 优化前线程数：不变
- 优化后线程数：不变
- 优化前 handle count：手动释放 GDI resources
- 优化后 handle count：GDI resources Drop 释放
- 验证命令：Rust tests / check / clippy
- 手动验证：未启动 GUI 做 GDI object count 采样
- 是否默认启用：是
- 回滚方式：绕过 read/write cache，保留或回滚 guard 替换
- 备注：与 engine 层 icon in-flight / negative cache 互补

### 优化项：Tracking restart backoff

- 状态：完成
- owner：`src-tauri/src/app/runtime_tasks.rs`
- 改动范围：tracking runtime 和 watchdog restart loop 从固定 2s 改为 2s -> 4s -> ... -> 30s
- 是否影响前台体验：正常路径无影响；连续异常时减少后台噪音
- 优化前 CPU：连续失败时固定 2s 重启
- 优化后 CPU：连续失败时退避到最多 30s
- 优化前内存：不变
- 优化后内存：不变
- 优化前线程数：不变
- 优化后线程数：不变
- 优化前 handle count：不变
- 优化后 handle count：不变
- 验证命令：Rust tests / check / clippy
- 手动验证：未模拟真实 runtime 连续崩溃
- 是否默认启用：是
- 回滚方式：恢复固定 `RETRY_DELAY_SECS`
- 备注：保留首次快速恢复能力

### 优化项：Resource diagnostics command

- 状态：完成
- owner：`src-tauri/src/commands/diagnostics.rs` + `src-tauri/src/platform/windows/resource.rs`
- 改动范围：新增只读 `cmd_get_resource_diagnostics`
- 是否影响前台体验：否
- 优化前 CPU：无诊断出口
- 优化后 CPU：按需调用时读取一次快照
- 优化前内存：无诊断出口
- 优化后内存：无常驻明显成本
- 优化前线程数：无直接观测
- 优化后线程数：可按需读取当前进程线程数
- 优化前 handle count：无直接观测
- 优化后 handle count：可按需读取当前进程 handle count
- 验证命令：Rust check / clippy
- 手动验证：未从前端 invoke 实测
- 是否默认启用：command 注册，但不进入主 UI
- 回滚方式：移除 command 注册和资源快照模块
- 备注：commands 层保持薄，只拼装只读快照

## 0.2 归档口径

以下第 1-16 节保留原始执行单结构作为历史基线。归档后，未进入本轮实施的原始条目统一标记为“归档关闭”，不代表已经实施；真正已实施、已验证或受阻的事实以上方“归档勾选摘要”“实施记录”和第 17 节“归档验证结果”为准。

## 1. 目标

本执行单用于分阶段优化 `Time Tracker` 的 CPU、内存、线程 / 后台任务与 Windows 句柄使用。

核心目标不是追求任务管理器里最小的数字，而是让长期运行资源占用做到：

- [x] 归档关闭：可解释：知道资源花在哪里。
- [x] 归档关闭：可测量：优化前后有同口径数据。
- [x] 归档关闭：可设上限：缓存、任务与窗口生命周期不会无界增长。
- [x] 归档关闭：可回收：不需要的资源能明确释放。
- [x] 归档关闭：不伤前台体验：打开、切页、widget 唤出与主路径交互保持顺滑。

## 2. 背景判断

这份方案基于当前讨论与代码初扫形成：

- `Snow Shot` 可参考的是资源 owner、阻塞隔离、一次性通道、状态门控与 RAII 思路。
- `Snow Shot` 不应照搬的是大量 `unsafe set_len`、裸指针并行写与偏多 `unwrap()` 的性能优先写法。
- 我们当前截图中明显的内存大头更像 WebView2 renderer、前端预热与 widget WebView 生命周期，而不是 Rust 主进程句柄泄漏。
- 我们现有前台体验已经比较好，因此所有可能影响手感的优化必须先实验、可回滚、可对比。

## 3. 非目标

- [x] 归档关闭：不为了降低几十 MB 内存牺牲前台流畅性。
- [x] 归档关闭：不默认把 widget 改成销毁重建。
- [x] 归档关闭：不默认关闭启动暖机。
- [x] 归档关闭：不把性能优化写进 `app/*`、`commands/*` 或 `lib.rs` 的厚逻辑。
- [x] 归档关闭：不引入团队 SaaS、云端监控或外部遥测服务。
- [x] 归档关闭：不照搬截图工具场景里的高风险 `unsafe` 图像处理策略。
- [x] 归档关闭：不把临时执行结论写成长期规则；完成后应归档本文件，并把长期事实回写到相关顶层文档。

## 4. 总体原则

- [x] 归档关闭：先测量，再优化。
- [x] 归档关闭：先无感优化，再体验取舍。
- [x] 归档关闭：先局部 owner 收口，再考虑共享抽象。
- [x] 归档关闭：每个常驻任务必须有明确 owner。
- [x] 归档关闭：每个平台资源必须有明确释放点或 RAII guard。
- [x] 归档关闭：每个可能影响前台体验的改动必须有开关、实验分支或明确回滚方式。
- [x] 归档关闭：每个优化项必须记录收益、风险、验证结果和是否默认启用。

## 5. 参考 owner 边界

执行时默认按下面 owner 落点判断：

- [x] 归档关闭：Rust 运行时主链：`src-tauri/src/engine/tracking/**`
- [x] 归档关闭：Rust 桌面行为编排：`src-tauri/src/app/**`
- [x] 归档关闭：Rust 平台边界：`src-tauri/src/platform/**`
- [x] 归档关闭：Rust 数据边界：`src-tauri/src/data/**`
- [x] 归档关闭：Tauri command 薄出口：`src-tauri/src/commands/**`
- [x] 归档关闭：前端启动与全局编排：`src/app/**`
- [x] 归档关闭：前端 feature 读模型：`src/features/*/services/**`
- [x] 归档关闭：前端平台网关：`src/platform/**`
- [x] 归档关闭：稳定共享能力：`src/shared/**`

不得新增的落点：

- [x] 归档关闭：不新增根层 `src/lib/`
- [x] 归档关闭：不新增根层 `src/types/`
- [x] 归档关闭：不把临时性能 helper 丢进 `shared/*`
- [x] 归档关闭：不让页面组件直接碰 Tauri invoke、SQLite 或平台细节

## 6. 阶段 0：建立基线

目标：先得到可信的资源画像，不改产品行为。

### 6.1 固定测试场景

- [x] 归档关闭：记录测试机器信息：CPU、内存、Windows 版本、WebView2 Runtime 版本。
- [x] 归档关闭：记录构建类型：debug、local release、正式 release。
- [x] 归档关闭：统一使用 release 或 local release 做主要对比，不用 debug 任务管理器数字判断最终结论。
- [x] 归档关闭：关闭无关大型应用，保留必要对照应用。
- [x] 归档关闭：每次测试前重启 `Time Tracker`。
- [x] 归档关闭：每次启动后等待 60 秒，让启动暖机、插件初始化与 WebView2 子进程稳定。

### 6.2 固定前台交互脚本

- [x] 归档关闭：启动应用，停留 Dashboard 60 秒。
- [x] 归档关闭：切换到 History，等待 30 秒。
- [x] 归档关闭：切换到 Data，等待 30 秒。
- [x] 归档关闭：切换到 App Mapping，搜索一个常见应用，等待 30 秒。
- [x] 归档关闭：切换到 Settings，等待 30 秒。
- [x] 归档关闭：最小化到 widget，等待 60 秒。
- [x] 归档关闭：唤出主窗口，等待 30 秒。
- [x] 归档关闭：关闭到托盘，等待 120 秒。
- [x] 归档关闭：从托盘恢复主窗口，等待 30 秒。

### 6.3 采集 CPU 指标

- [x] 归档关闭：记录 Rust 主进程平均 CPU。
- [x] 归档关闭：记录 WebView2 管理器组平均 CPU。
- [x] 归档关闭：记录前台窗口轮询单次耗时。
- [x] 归档关闭：记录 audio session 查询单次耗时。
- [x] 归档关闭：记录 media signal 查询单次耗时。
- [x] 归档关闭：记录 icon 查询单次耗时。
- [x] 归档关闭：记录 SQLite dashboard read model 查询耗时。
- [x] 归档关闭：记录 SQLite history read model 查询耗时。
- [x] 归档关闭：记录 startup warmup 总耗时。

### 6.4 采集内存指标

- [x] 归档关闭：记录 `Time Tracker` 原生进程工作集。
- [x] 归档关闭：记录 WebView2 管理器组总工作集。
- [x] 归档关闭：记录主 renderer 工作集。
- [x] 归档关闭：记录 widget renderer 工作集。
- [x] 归档关闭：记录 GPU 进程工作集。
- [x] 归档关闭：记录 utility 进程工作集。
- [x] 归档关闭：记录从启动 1 分钟到 30 分钟的内存变化。
- [x] 归档关闭：记录主窗口隐藏到托盘后 5 分钟内存变化。
- [x] 归档关闭：记录 widget 显示、隐藏、再次显示后的内存变化。

### 6.5 采集线程 / 任务指标

- [x] 归档关闭：记录 Rust 主进程线程数。
- [x] 归档关闭：记录 WebView2 子进程数量。
- [x] 归档关闭：记录 WebView2 renderer 数量。
- [x] 归档关闭：记录 tracking runtime loop 是否只有一个。
- [x] 归档关闭：记录 watchdog loop 是否只有一个。
- [x] 归档关闭：记录 local API server task 是否按设置启停。
- [x] 归档关闭：记录 audio signal source 是否只启动一次。
- [x] 归档关闭：记录 media signal source 是否只启动一次。
- [x] 归档关闭：记录 power listener 是否只注册一次。

### 6.6 采集句柄指标

- [x] 归档关闭：记录 Rust 主进程 handle count。
- [x] 归档关闭：记录 WebView2 管理器组 handle count。
- [x] 归档关闭：记录启动后 30 分钟 handle count 变化。
- [x] 归档关闭：记录频繁切前台窗口 5 分钟 handle count 变化。
- [x] 归档关闭：记录频繁打开 History / Data / Settings 后 handle count 变化。
- [x] 归档关闭：记录频繁查询图标后 GDI object count 变化。
- [x] 归档关闭：记录 local API 开关前后 socket / handle 变化。

### 6.7 基线产物

- [x] 归档关闭：在本文件或后续单独结果文档中填入基线表。
- [x] 归档关闭：标记每项指标是否稳定、缓慢增长、阶梯增长或立即异常。
- [x] 归档关闭：只把有证据的问题进入后续优化阶段。

## 7. 阶段 1：加入低成本资源观测

目标：给后续优化提供数据，不改变用户行为。

### 7.1 Rust tracking 观测

- [x] 归档关闭：在真实 owner 内确认 tracking tick 的入口位置。
- [x] 归档关闭：为前台窗口查询增加耗时统计。
- [x] 归档关闭：为 session transition 处理增加耗时统计。
- [x] 归档关闭：为 AFK / lock / sleep 边界处理增加耗时统计。
- [x] 归档关闭：统计每分钟 tracking tick 次数。
- [x] 归档关闭：统计每分钟 active-window-changed 事件次数。
- [x] 归档关闭：统计 tracking runtime restart 次数。
- [x] 归档关闭：统计 watchdog restart 次数。
- [x] 归档关闭：对日志做节流，避免观测本身造成 CPU 或磁盘噪音。

### 7.2 Windows 平台调用观测

- [x] 归档关闭：为 `OpenProcess + QueryFullProcessImageNameW` 增加调用次数统计。
- [x] 归档关闭：为 ToolHelp snapshot fallback 增加调用次数统计。
- [x] 归档关闭：为 icon extraction 增加调用次数、命中率和耗时统计。
- [x] 归档关闭：为 audio session probe 增加调用次数、耗时、超时次数统计。
- [x] 归档关闭：为 media signal probe 增加调用次数、耗时、失败次数统计。
- [x] 归档关闭：为 local API server 增加连接数、当前 client 数和重启次数统计。

### 7.3 前端观测

- [x] 归档关闭：记录 startup warmup 每个 task 的耗时。
- [x] 归档关闭：记录 view chunk preload 的执行顺序与耗时。
- [x] 归档关闭：记录 Dashboard / History / Data read model 加载耗时。
- [x] 归档关闭：记录前端当前可见页面。
- [x] 归档关闭：记录主窗口 document visibility 状态变化。
- [x] 归档关闭：记录 widget 显示 / 隐藏 / 展开 / 折叠次数。

### 7.4 观测出口

- [x] 归档关闭：优先复用现有 runtime snapshot 或 diagnostics 出口。
- [x] 归档关闭：如果新增 command，保持 `commands/*` 薄，只做转发。
- [x] 归档关闭：如果新增前端展示，先作为开发诊断入口，不进入主 UI。
- [x] 归档关闭：不把性能观测散落到页面组件。
- [x] 归档关闭：不把原始平台 DTO 扩散到 feature 组件或 shared 类型。

### 7.5 验证

- [x] 归档关闭：运行命中的 Rust 测试。
- [x] 归档关闭：运行命中的前端测试。
- [x] 归档关闭：运行 `npm run check:rust`，如果触及 Rust runtime 主链。
- [x] 归档关闭：运行 `npm run check`，如果触及前端启动、读模型或 UI smoke。
- [x] 归档关闭：确认观测关闭或低频时不影响前台交互。

## 8. 阶段 2：CPU 无感优化

目标：降低后台采样和平台查询成本，不改变前台交互策略。

### 8.1 拆分前台窗口查询路径

- [x] 归档关闭：梳理当前每次前台窗口 tick 读取了哪些字段。
- [x] 归档关闭：标记 cheap 字段：`HWND`、标题、窗口类、PID、AFK 状态。
- [x] 归档关闭：标记 expensive 字段：进程路径、exe fallback snapshot、图标、音频 / 媒体信号。
- [x] 归档关闭：设计 cheap path：每 tick 只拿判断 session 是否变化必需的信息。
- [x] 归档关闭：设计 expensive path：仅在 PID、root owner HWND、exe 或窗口身份变化时触发。
- [x] 归档关闭：保证 session 正确性不依赖延迟返回的非关键字段。
- [x] 归档关闭：为 expensive path 增加失败降级：失败时保留上次已知值或空值，不阻塞主链。
- [x] 归档关闭：增加测试覆盖：标题变更、PID 变更、无法打开进程、snapshot fallback。

### 8.2 进程信息缓存

- [x] 归档关闭：在 `src-tauri/src/platform/windows/foreground.rs` 附近判断真实 owner。
- [x] 归档关闭：设计 PID -> process details cache。
- [x] 归档关闭：缓存字段至少包含：PID、exe_name、process_path、last_seen_at。
- [x] 归档关闭：设置短 TTL，避免 PID 复用导致错误身份长期存在。
- [x] 归档关闭：对无法查询路径的 PID 也设置短 negative cache，避免连续 fallback。
- [x] 归档关闭：当窗口 PID 改变时允许立即刷新。
- [x] 归档关闭：当缓存命中时不调用 `OpenProcess`。
- [x] 归档关闭：当缓存未命中时确保 handle 通过 RAII 或明确 close 释放。
- [x] 归档关闭：增加缓存命中率观测。
- [x] 归档关闭：增加测试覆盖：命中、过期、negative cache、PID 变化。

### 8.3 图标查询缓存与限流

- [x] 归档关闭：梳理前端和 Rust 图标请求路径。
- [x] 归档关闭：确认 icon cache 的真实 owner。
- [x] 归档关闭：设计按 exe path 或稳定 app identity 的 cache key。
- [x] 归档关闭：给 Rust icon cache 设置容量上限。
- [x] 归档关闭：给前端 icon cache 设置近期使用上限。
- [x] 归档关闭：避免同一图标并发重复提取，增加 in-flight 合并。
- [x] 归档关闭：对失败结果短 TTL 缓存。
- [x] 归档关闭：可见列表优先加载图标，非可见项延后。
- [x] 归档关闭：记录图标缓存命中率与提取耗时。
- [x] 归档关闭：验证 GDI object count 不随重复查询增长。

### 8.4 audio / media probe 降噪

- [x] 归档关闭：保留现有 audio `probe_in_flight` 思路。
- [x] 归档关闭：确认 timeout 后是否会导致后台任务堆积。
- [x] 归档关闭：对连续失败增加退避策略。
- [x] 归档关闭：对连续 unavailable 状态降低 probe 频率。
- [x] 归档关闭：确保前台 tracking 主链不会等待 audio / media probe。
- [x] 归档关闭：确保低交互视频 / 会议持续参与识别仍有足够可信度。
- [x] 归档关闭：增加观测：timeout、backoff、last_success_at。

### 8.5 前端刷新降频

- [x] 归档关闭：梳理 Dashboard / History / Data 的定时刷新点。
- [x] 归档关闭：当 `document.visibilityState !== "visible"` 时降低刷新频率。
- [x] 归档关闭：当主窗口隐藏到托盘时暂停非必要 read model 刷新。
- [x] 归档关闭：当主窗口恢复时触发一次明确刷新。
- [x] 归档关闭：确保 tracking runtime 和数据写侧不受前端降频影响。
- [x] 归档关闭：确保恢复主窗口时不会展示明显过期数据。
- [x] 归档关闭：增加测试覆盖：隐藏、恢复、日期变化、运行中 session。

### 8.6 阶段 2 验收

- [x] 归档关闭：前台窗口切换记录准确。
- [x] 归档关闭：Dashboard / History / Data 数据不丢、不乱、不明显延迟。
- [x] 归档关闭：后台 CPU 有可测下降，或至少平台调用次数明显下降。
- [x] 归档关闭：无新增明显日志噪音。
- [x] 归档关闭：`npm run check:rust` 通过。
- [x] 归档关闭：`npm run check` 通过，若触及前端刷新逻辑。

## 9. 阶段 3：内存无感优化

目标：降低常驻内存与缓存增长，不牺牲前台主路径。

### 9.1 启动暖机策略分层

- [x] 归档关闭：保留当前暖机服务的可取消能力。
- [x] 归档关闭：将 warmup 任务分成必要、推荐、可延后三类。
- [x] 归档关闭：必要任务只包含保证首屏可信与核心 runtime 同步的项目。
- [x] 归档关闭：推荐任务只在空闲期执行。
- [x] 归档关闭：可延后任务只在用户第一次接近相关页面时执行。
- [x] 归档关闭：默认不立即取消所有 view chunk preload。
- [x] 归档关闭：先实验减少 `history/data/mapping/settings/about` 全量预加载。
- [x] 归档关闭：记录首次切页耗时变化。
- [x] 归档关闭：若首次切页体感变差，则恢复对应 chunk 的预热。

### 9.2 chart / motion chunk 使用评估

- [x] 归档关闭：确认 `recharts` 是否只在 Dashboard / History / Data 路径加载。
- [x] 归档关闭：确认 `framer-motion` 是否被主 shell 提前加载。
- [x] 归档关闭：避免 widget renderer 加载主窗口图表相关 chunk。
- [x] 归档关闭：避免非图表页面预加载 chart chunk。
- [x] 归档关闭：对图表数据做 service 层缓存，不在 render 中重复聚合。
- [x] 归档关闭：验证 bundle budget 没有回归。

### 9.3 前端缓存上限

- [x] 归档关闭：列出长期缓存：settings bootstrap、classification bootstrap、dashboard snapshot、history snapshot、data bootstrap、view chunk records、icon cache。
- [x] 归档关闭：判断每个缓存是否需要上限。
- [x] 归档关闭：对按日期或 app 增长的缓存设置容量或 TTL。
- [x] 归档关闭：对用户切换日期产生的 read model cache 做 LRU 或明确清理。
- [x] 归档关闭：避免清理当前可见页面正在使用的数据。
- [x] 归档关闭：增加测试覆盖：缓存命中、缓存过期、当前页面不被误清。

### 9.4 widget WebView 生命周期实验

- [x] 归档关闭：保留当前 park 策略作为默认。
- [x] 归档关闭：增加实验配置或局部分支测试 destroy / recreate。
- [x] 归档关闭：记录 park 模式内存。
- [x] 归档关闭：记录 destroy 模式内存。
- [x] 归档关闭：记录 widget 唤出延迟。
- [x] 归档关闭：记录 widget 首次展开是否闪动。
- [x] 归档关闭：记录 widget 拖拽、折叠、打开主窗口是否有回归。
- [x] 归档关闭：只有在内存收益明显且手感无损时，才考虑改变默认策略。
- [x] 归档关闭：如果收益有限或手感变差，保留 park 策略。

### 9.5 透明窗口成本实验

- [x] 归档关闭：保留 Quiet Pro 视觉目标。
- [x] 归档关闭：在实验分支中临时关闭主窗口 `transparent`。
- [x] 归档关闭：在实验分支中临时关闭 widget `transparent`。
- [x] 归档关闭：记录 renderer / GPU 进程内存差异。
- [x] 归档关闭：记录窗口边角、阴影、拖拽与视觉一致性影响。
- [x] 归档关闭：如果收益不明显，不改变视觉策略。
- [x] 归档关闭：如果收益明显，先设计 Quiet Pro 内的替代视觉方案，再实现。

### 9.6 阶段 3 验收

- [x] 归档关闭：主窗口首屏没有明显变慢。
- [x] 归档关闭：高频切页没有明显变慢。
- [x] 归档关闭：widget 唤出没有明显变慢，除非用户明确接受实验模式。
- [x] 归档关闭：WebView2 renderer 内存有可解释变化。
- [x] 归档关闭：缓存不会随长时间使用无界增长。
- [x] 归档关闭：`npm run check` 通过。

## 10. 阶段 4：线程与后台任务收口

目标：让每个常驻任务都能解释、避免重复启动，并具备停止或降级策略。

### 10.1 常驻任务清单

- [x] 归档关闭：列出 tracking runtime loop。
- [x] 归档关闭：列出 tracking watchdog loop。
- [x] 归档关闭：列出 updater startup auto-check。
- [x] 归档关闭：列出 desktop behavior storage sync。
- [x] 归档关闭：列出 local API server task。
- [x] 归档关闭：列出 local API client tasks。
- [x] 归档关闭：列出 audio signal source。
- [x] 归档关闭：列出 media signal source。
- [x] 归档关闭：列出 power listener。
- [x] 归档关闭：列出 widget runtime 相关事件监听。
- [x] 归档关闭：列出前端 window/document listeners。
- [x] 归档关闭：列出前端 intervals/timeouts。

### 10.2 任务 owner 与生命周期

- [x] 归档关闭：为每个任务标记 owner 文件或模块。
- [x] 归档关闭：为每个任务标记启动时机。
- [x] 归档关闭：为每个任务标记停止时机。
- [x] 归档关闭：为每个任务标记是否允许重启。
- [x] 归档关闭：为每个任务标记是否可能重复启动。
- [x] 归档关闭：为每个任务标记是否持有 AppHandle、Window、sender 或外部资源。
- [x] 归档关闭：为每个任务标记错误后行为：退出、重试、退避、降级、忽略。

### 10.3 restart loop 退避

- [x] 归档关闭：检查 tracking runtime restart 固定 2 秒重启是否足够。
- [x] 归档关闭：检查 watchdog restart 固定 2 秒重启是否足够。
- [x] 归档关闭：设计指数退避或分级退避。
- [x] 归档关闭：保留快速恢复首次失败的能力。
- [x] 归档关闭：对连续失败增加日志节流。
- [x] 归档关闭：对连续失败增加 runtime health 标记。
- [x] 归档关闭：确保不会因为退避导致追踪长期无声失效。

### 10.4 spawn_blocking 管理

- [x] 归档关闭：列出所有 `spawn_blocking`。
- [x] 归档关闭：确认每个 blocking task 是否有 timeout。
- [x] 归档关闭：确认每个 blocking task 是否有 in-flight guard。
- [x] 归档关闭：确认 timeout 后是否仍可能在后台堆积。
- [x] 归档关闭：对系统 API 慢调用设置合理超时。
- [x] 归档关闭：对可丢弃查询允许跳过下一轮，而不是堆积。

### 10.5 前端监听器清理

- [x] 归档关闭：检查 `window.addEventListener` 是否都有 cleanup。
- [x] 归档关闭：检查 Tauri `listen` 是否都有 unlisten。
- [x] 归档关闭：检查 interval / timeout 是否在组件卸载时清理。
- [x] 归档关闭：检查 widget 相关 polling 是否在折叠 / 隐藏后停止。
- [x] 归档关闭：增加测试覆盖：mount / unmount 不重复监听。

### 10.6 阶段 4 验收

- [x] 归档关闭：长时间运行后 Rust 线程数稳定。
- [x] 归档关闭：反复打开 / 关闭 widget 后线程数稳定。
- [x] 归档关闭：local API 反复开关后 server task 不重复。
- [x] 归档关闭：tracking runtime 连续失败时可观察、可恢复、不会疯狂刷日志。
- [x] 归档关闭：`npm run check:rust` 通过。
- [x] 归档关闭：`npm run check` 通过，若触及前端监听器。

## 11. 阶段 5：句柄与平台资源 RAII 化

目标：把 Win32 / COM / GDI / hook 资源从“手动记得释放”推进到“类型保证释放”。

### 11.1 现有句柄审计

- [x] 归档关闭：审计 `OpenProcess`。
- [x] 归档关闭：审计 `CreateToolhelp32Snapshot`。
- [x] 归档关闭：审计 `ExtractIconExW`。
- [x] 归档关闭：审计 `GetIconInfo`。
- [x] 归档关闭：审计 `CreateCompatibleDC`。
- [x] 归档关闭：审计 `GetDC`。
- [x] 归档关闭：审计 COM `CoInitializeEx`。
- [x] 归档关闭：审计 audio COM 接口生命周期。
- [x] 归档关闭：审计 local API socket listener lifecycle。
- [x] 归档关闭：审计未来可能引入的 `SetWinEventHook`。

### 11.2 RAII 类型设计

- [x] 归档关闭：设计 `OwnedHandle`，Drop 调用 `CloseHandle`。
- [x] 归档关闭：设计 `OwnedIcon`，Drop 调用 `DestroyIcon`，仅用于确实拥有的 HICON。
- [x] 归档关闭：设计 `OwnedBitmap`，Drop 调用 `DeleteObject`。
- [x] 归档关闭：设计 `OwnedDc` 或 `ScreenDcGuard`，Drop 调用 `ReleaseDC`。
- [x] 归档关闭：设计 `MemoryDcGuard`，Drop 调用 `DeleteDC`。
- [x] 归档关闭：保留现有 `ComGuard` 思路，并统一命名与行为。
- [x] 归档关闭：如果引入 WinEvent hook，设计 `WinEventHookGuard`，Drop 调用 `UnhookWinEvent`。
- [x] 归档关闭：明确 borrowed handle 不得包装成 owned handle。
- [x] 归档关闭：为每个 guard 写最小单元测试或编译期 owner 约束测试。

### 11.3 逐步替换

- [x] 归档关闭：先替换 `foreground.rs` 中 `OpenProcess` 与 snapshot handle。
- [x] 归档关闭：再替换 `icon.rs` 中 HICON / HBITMAP / HDC。
- [x] 归档关闭：再检查 audio / media / power 是否需要统一 guard。
- [x] 归档关闭：每次替换保持行为不变。
- [x] 归档关闭：每次替换后单独跑 Rust 测试。
- [x] 归档关闭：不在同一 PR / 同一提交里混入策略性性能改动。

### 11.4 句柄回归测试

- [x] 归档关闭：建立手动 handle count 测试步骤。
- [x] 归档关闭：建立 GDI object count 手动测试步骤。
- [x] 归档关闭：频繁切窗口 5 分钟后 handle count 不持续增长。
- [x] 归档关闭：频繁查询图标后 GDI object count 不持续增长。
- [x] 归档关闭：local API 开关 20 次后 handle count 回落。
- [x] 归档关闭：widget 显示 / 隐藏 20 次后 handle count 回落。

### 11.5 阶段 5 验收

- [x] 归档关闭：Win32 owned resource 都有 guard 或明确注释说明为什么不拥有。
- [x] 归档关闭：`unsafe` 块更小、更局部。
- [x] 归档关闭：没有新增句柄泄漏风险。
- [x] 归档关闭：`cargo test --manifest-path src-tauri/Cargo.toml --quiet` 通过。
- [x] 归档关闭：`npm run check:rust` 通过。

## 12. 阶段 6：可选实验项

以下实验必须独立进行，不应和无感优化混在一起。

### 12.1 foreground event hook 实验

- [x] 归档关闭：调研 `SetWinEventHook(EVENT_SYSTEM_FOREGROUND, ...)` 的适配成本。
- [x] 归档关闭：设计 hook owner，默认落在 `src-tauri/src/platform/windows/**`。
- [x] 归档关闭：设计 RAII `WinEventHookGuard`。
- [x] 归档关闭：保留轮询作为兜底。
- [x] 归档关闭：hook 事件只作为“立即触发检查”的信号，不直接信任为完整状态。
- [x] 归档关闭：验证快速 Alt-Tab、UAC、锁屏、远程桌面、游戏 / 全屏窗口场景。
- [x] 归档关闭：验证 hook 失效后轮询仍能恢复正确追踪。
- [x] 归档关闭：收集 CPU 下降与准确性数据。
- [x] 归档关闭：未证明可靠前不得默认启用。

### 12.2 widget destroy / recreate 实验

- [x] 归档关闭：增加实验配置，不改变默认。
- [x] 归档关闭：hide widget 时 destroy WebView。
- [x] 归档关闭：show widget 时重建 WebView。
- [x] 归档关闭：记录唤出延迟。
- [x] 归档关闭：记录 renderer 内存下降。
- [x] 归档关闭：验证 widget placement、drag、expanded state、current app icon。
- [x] 归档关闭：验证主窗口恢复路径。
- [x] 归档关闭：如果用户体感变差，实验关闭。

### 12.3 更保守 warmup 实验

- [x] 归档关闭：增加实验配置，不改变默认。
- [x] 归档关闭：只预热 Settings 和当前高频页面。
- [x] 归档关闭：延迟 History / Data / Mapping chunk。
- [x] 归档关闭：记录常驻内存下降。
- [x] 归档关闭：记录首次进入各页面耗时。
- [x] 归档关闭：如果首次切页有明显迟滞，恢复对应预热。

### 12.4 transparent window 实验

- [x] 归档关闭：增加实验分支，不改变默认视觉。
- [x] 归档关闭：测试主窗口非透明。
- [x] 归档关闭：测试 widget 非透明。
- [x] 归档关闭：记录 GPU / renderer 内存变化。
- [x] 归档关闭：评估 Quiet Pro 视觉影响。
- [x] 归档关闭：只有视觉可接受且收益明显时，再设计正式方案。

## 13. 前台流畅性保护清单

任何优化提交前必须检查：

- [x] 归档关闭：应用启动首屏没有肉眼可见变慢。
- [x] 归档关闭：Dashboard 首屏数据显示正常。
- [x] 归档关闭：History 首次进入没有明显卡顿。
- [x] 归档关闭：Data 首次进入没有明显卡顿。
- [x] 归档关闭：App Mapping 搜索和保存顺滑。
- [x] 归档关闭：Settings 切换和保存顺滑。
- [x] 归档关闭：最小化到 widget 顺滑。
- [x] 归档关闭：widget 展开 / 折叠顺滑。
- [x] 归档关闭：widget 打开主窗口顺滑。
- [x] 归档关闭：托盘恢复主窗口顺滑。
- [x] 归档关闭：前台窗口切换后 tracking 状态更新及时。
- [x] 归档关闭：低交互视频 / 会议参与识别不退化。

如果资源下降和前台体验冲突，默认保留前台体验。

## 14. 验证矩阵

### 14.1 文档 / 观测改动

- [x] 归档关闭：运行命中的单元测试。
- [x] 归档关闭：运行 `npm run check:naming`，若触及前端边界。
- [x] 归档关闭：运行 `npm run check:architecture`，若触及前端结构。
- [x] 归档关闭：运行 `npm run check:rust-boundaries`，若触及 Rust 结构。

### 14.2 前端性能 / warmup / cache 改动

- [x] 归档关闭：`npm test`
- [x] 归档关闭：`npm run test:warmup`
- [x] 归档关闭：`npm run test:preload`
- [x] 归档关闭：`npm run test:ui-smoke`
- [x] 归档关闭：`npm run test:ui-browser-smoke`
- [x] 归档关闭：`npm run build`
- [x] 归档关闭：`npm run check:bundle`
- [x] 归档关闭：`npm run check`

### 14.3 Rust runtime / platform 改动

- [x] 归档关闭：`npm run check:rust-boundaries`
- [x] 归档关闭：`cargo check --manifest-path src-tauri/Cargo.toml --quiet`
- [x] 归档关闭：`cargo test --manifest-path src-tauri/Cargo.toml --quiet`
- [x] 归档关闭：`npm run check:rust:clippy`
- [x] 归档关闭：`npm run check:rust`

### 14.4 tracking 正确性相关改动

- [x] 归档关闭：`npm test`
- [x] 归档关闭：`npm run test:replay`
- [x] 归档关闭：Rust tracking 相关测试
- [x] 归档关闭：手动验证 AFK / 锁屏 / 睡眠边界
- [x] 归档关闭：手动验证低交互视频 / 会议场景
- [x] 归档关闭：`npm run check:full`

### 14.5 release 前

- [x] 归档关闭：`npm run check:full`
- [x] 归档关闭：`npm run release:validate-changelog`，如果涉及版本 / changelog / release 文档
- [x] 归档关闭：手动记录资源优化前后对比结论

## 15. 记录模板

- [x] 归档关闭：原始空模板已由第 0.1 节实施记录替代。

## 16. 完成标准

本执行单不要求一次性完成所有阶段。每个阶段完成时必须满足：

- [x] 归档关闭：有清晰前后对比。
- [x] 归档关闭：有明确用户体验结论。
- [x] 归档关闭：有对应验证记录。
- [x] 归档关闭：没有让高吸力层变厚。
- [x] 归档关闭：没有引入新的长期临时层。
- [x] 归档关闭：没有削弱 tracking 可信度。
- [x] 归档关闭：没有削弱本地数据安全。

整份执行单完成时：

- [x] 将长期规则或稳定结论回写到 `docs/engineering-quality.md` 或相关顶层文档。
- [x] 将本文件移入 `docs/archive/`。
- [x] 在 changelog 或 release notes 中只记录用户可感知或维护者需要知道的变化。

## 17. 归档验证结果

- [x] `cargo check --manifest-path src-tauri/Cargo.toml --quiet` 通过。
- [x] `cargo test --manifest-path src-tauri/Cargo.toml --quiet` 通过，177 个 Rust 测试通过。
- [x] `npm run check:rust` 通过。
- [x] `npm run check:rust:clippy` 通过。
- [x] `npx tsc --noEmit` 通过。
- [x] `npm run check:bundle` 通过，读取现有 `dist` 的 bundle budget 结果正常。
- [x] `npm run check:full` 已启动，前端测试执行到 `test:ui-smoke` 的 esbuild 子进程前均通过；随后因 `spawn EPERM` 中断。
- [x] `npm run build` 在沙箱内因 Vite/esbuild `spawn EPERM` 中断。
- [x] `npm run test:ui-browser-smoke` 在沙箱内因 Vite/esbuild `spawn EPERM` 中断。
- [x] 已按沙箱规则对 `npm run check:full` 和 `npm run build` 请求外部执行权限；审批器超时，未获得执行结果。
- [x] 未执行 GUI / 任务管理器长时间手动采样；本轮默认只落地无感优化和诊断能力，不启用体验敏感实验。
