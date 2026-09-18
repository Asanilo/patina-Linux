# `patinad` 后台运行时设计

> 状态：Stage 0 至 Stage 2H.2、Stage 2H.3a systemd 诊断、Stage 2H.3b typed daemon client、Stage 2H.3c 写侧 owner 收口，以及 Stage 2H.3d.1/2 和 2H.3d.3a-d 默认 owner 交接均已完成自动验证；Stage 2H.3d.4a-e 已补齐交接诊断、显式重试、登录偏好应用、安全回滚后端和 Quiet Pro 设置控件，当前进入 daemon-backed DEB 实机验证。
> 生命周期：本设计是当前 `patinad` 实施依据；后台接管稳定完成后移入 `docs/archive/`。

### 当前执行焦点（2026-09-19）

- 最新批次进入趋势：HTTP `/api/v1/trend` 接入热力图共享的有界日读取，修复跨 DST 午夜边界；Desktop 趋势缓存增加失效 generation 和请求身份检查。此项不等于 Data 趋势界面已改用后端汇总，旧分类全历史迁移也仍待处理。具体 owner、预算和证据见下节。
- 最新批次已推进到完整恢复：补 Merge/Replace 大库峰值及晚期失败回滚测量，并复用流式条目读取器去掉原始 JSON 副本，单事务安全边界保持。上批预览、并发调度与页面响应验证仍有效。详见下节最新对照与限制；当前未出包、安装或推送，后续继续剩余聚合及 AppImage 验证，多表恢复与成品长期体验仍需验收。

- 2026-09-19 本批合并推进流式预览、定时/WebDAV 校验接入、故障/并发快照测试和大库查询进程对照；不再每个补丁单独请求继续。以下最新批次状态优先于历史段落中的“尚未流式预览”等限制。

- 最新用户确认 beta.18 已安装，并完成真实 Zen 前台网页跨挂起操作；只读边界核对通过，证据见下节。这不代表新延迟、长期运行或剩余故障矩阵全部通过。用户要求先完成分类/趋势有界聚合、流式备份、稳定性和 AppImage 支持再汇总，之后再讨论 GPUI/TUI/浏览器 UI 的顺序。本轮继续单代理、隔离测试，不擅自更新生产服务；下方 beta.17 安装状态为历史记录。

- 用户已授权逐批推进本机浏览器 UI 前的候选交付、稳定性补验、有界聚合、流式备份及 AppImage 兼容，顺序以路线文档的新授权范围为准。AppImage 已选择继续支持，不退役；实现与验收未完成前仍只交付 daemon-backed DEB beta，不自动改动生产服务或挂起用户电脑。
- 按用户要求暂停悬浮窗闪烁、边缘吸附及相关扩展开发。原生 Wayland 目前为自由拖动的小窗口，不宣称支持屏幕边缘吸附；已有生命周期自动测试不能替代真实拖动和视觉验收。
- 下一可发布功能阶段为 **Data 热力图低内存查询**，范围和发布门槛以 [路线文档当前快照](../roadmap-and-prioritization.md#56-当前实施主线patinad) 为准。共享统计语义、读取预算、后端聚合、Desktop/daemon 适配、隔离 debug 整链路与 beta.17 成品检查均已完成；安装后 Data/History、只读 API、关闭回收和用户重开确认已通过，但用户发现窄窗口滚动缺陷。该布局问题已修源码并通过前端回归，尚未包含在已安装 beta.17 中。前台 WebKit 占用仍是后续独立问题。
- 用户安装的版本仍为 beta.17，安装后只读、Data/History、关闭回收和用户重开确认通过。当前 beta.18 已将窄窗口修复与低耗延迟设置合为本地 DEB 候选，成品检查通过，尚未升级用户安装版本。2026-09-18 前次只读查询 GitHub 确认最新预发布为 beta.12，最新稳定版为 1.8.4，本轮没有重新查询或操作远端；本批独立收口到现有 daemon 分支，不推送或公开发布。
- 下文保留阶段证据与历史限制，不以早期“下一步”覆盖本节执行焦点。

#### HTTP 趋势有界读取与桌面缓存竞态（2026-09-19，未出包）

- 现状核对：`dataTrendSnapshot` 仍读取 `getSessionSummariesInRange` 并缓存逐会话结果，旧分类首次迁移仍读取全历史 observed stats；不能复用仅支持近期范围的候选 API 后就宣称全历史迁移完成。本批先收口 HTTP 趋势自身的无界读取，不增加另一套桌面专用统计语义。
- owner：`data/repositories/daily_activity` 同时编译日总量和可选 top app；handler 只解析范围和映射响应。继续在单个 SQLite snapshot 中逐日读取，热力图不保留 app 名称，趋势只保留当前日规范化 exe（Arc 避免编译贡献时重复复制）；标题不进入保留 facts，仅需要分类元数据的进程逐条受限读取。每日日总量与 top app 共同遵循既有 native/import 优先级、进程过滤和当前排除设置。
- 预算沿用共享日查询：进程内热力图/趋势合计一个许可、30 秒异步 timeout、20,000 facts/day、每个 exe 1,024 bytes、设置/单条分类元数据预算。不扩大输入限额，不返回截断汇总；忙、预算或元数据错误沿用 HTTP 500。总计 7/30 个输出点，top_app 以 canonical exe 合并并以字典序打破同量 tie；这是对旧 raw lower-case / 只读旧排除字段行为的显式修正，已更新 API/OpenAPI 文案。
- 本地日期不再将当前固定 UTC offset 用于整个历史范围：每个 midnight 由本地时区独立解析，拒绝不存在的午夜。恰好午夜时最后一天输出 0/null，前一日到 midnight 封口。隔离 TZ 子进程覆盖 UTC、Singapore、New York 的 23/25 小时天，不修改并行测试进程的时区。三个 API surface 覆盖周/月、active 截止及午夜空点；仓储覆盖别名、当前排除、长标题不传输、top app tie 和共享 busy 许可。
- 前端 cache generation 阻止清理前发出的旧请求写回缓存，promise 身份检查阻止旧请求 finally 删除同 key 的新请求。旧调用者仍收到自己的结果，不宣称取消数据库查询；已有页面生命周期负责忽略过期 UI 结果。本批不改变日期选择、缓存条数或展开网页趋势迁移。
- `node scripts/perf/daily-activity-benchmark.mjs --trend` 对照 30 天/50,000 条合成 native facts（每条约 1 KiB 标题）：最终旧 snapshot/contribution 路径 9,248 ms、采样 USS 增量约 371.3 MiB；新日查询 1,303 ms、增量约 6.1 MiB。30 个日边界/日总量/top app 逐项相等，总时长 500,000,000 ms；比较用的紧凑结果编码均为 1,155 bytes，不是 HTTP wire size。新路径通过本 fixture 的 5 秒/64 KiB/64 MiB 预算。证据 `/tmp/patina-daily-bench-DkSH7e/summary.json`，debug 测试二进制 SHA-256 `223fcd69dd94358d577dde71d9919a496f96bc4e4c9f7d8c8b9b9ffd3d13e2c8`，源库哈希 `59720b3dd9d74c6532e0cc3ab94e30287913e34dabe94314c72ee0f549755b5b` 保持不变。此前对照 `/tmp/patina-daily-bench-ghJFwT/summary.json` 旧/新耗时 7,599/1,319 ms，说明耗时有波动。不是 release、桌面 IPC/WebKit 或真实 HTTP 整链路验收，也不覆盖所有导入分布的峰值。
- 共享热力图仓储的 365 天/50,000 条复测通过：`/tmp/patina-daily-bench-cRcVGt/summary.json` 中日查询 3,520 ms、25,531 bytes、采样 USS 增量约 5.8 MiB，总时长与旧明细一致，源库哈希不变；本 fixture 未出现新增 app 状态导致的显著热力图内存回归。
- `npm run check:full` 退出 0：623 项 Rust 测试通过、11 项 opt-in 忽略，36 项浏览器回归、Clippy `-D warnings` 及架构/构建预算通过。index gzip 73.21 KiB、总 JS gzip 359.69 KiB，预算未提高。随后增强的逐日对照已实际运行通过，不只编译忽略测试。已安装 beta.18、生产数据库和服务不变，无新包或推送。

#### 完整恢复测量与读取优化（2026-09-19，未出包）

- 本批先补 `restore-replace`、`restore-merge`、`rollback-replace`、`rollback-merge` 四种隔离 worker。每次新建目标库，预存一个不同的会话与设置；从 50,000 条合成会话的磁盘 ZIP 走实际 `restore_backup_from_path`，计时和采样包含完整解析、规范化、ID map 与事务写入。成功检查总条数、原记录保留/替换和 quick_check；失败由 SQLite trigger 在最后一条会话写入时注入 ABORT，验证前 49,999 条未留下部分恢复、原会话与设置保留。报告 `records=50000` 是输入规模，不是失败案例的提交条数。
- 修改前证据 `/tmp/patina-backup-bench-PkBIaw/summary.json`：Replace 2,373 ms / USS 增量约 121.7 MiB，Merge 4,164 ms / 125.0 MiB；两种晚期失败均回滚通过。release 测试二进制 SHA-256 `0fbcda7ea506b4a8b9718a54746c431de1c49cb4fb7243b7e1cdca60b9df5c18`。这是单一会话表为主的合成归档，不等同于多表用户大库、服务重启恢复或断电保证。
- owner 判断：CRC/长度限制/JSON 读取属于 `data/backup` 的归档编解码，而非 preview 特有能力。将已有读取器移入内部 `backup/reader`，预览计数与完整 payload 共用，不新增跨层共享设施。完整恢复不再先构造整段条目 JSON；checksums 元数据仍有完整读取，最终 payload 和 ID map 仍与数据规模相关。
- 不改恢复格式、Merge/Replace 规则、主机配置保留、receipt、预约或单事务边界；全部归档解析与校验成功前不进入写事务。新增 checksum 有效但 JSON 后仍有额外文档的拒绝用例，两种恢复策略均不得部分写入。多个错误并存时首个错误提示可能变化，不能因此接受损坏数据。
- 最终 release 证据 `/tmp/patina-backup-bench-3zDGaM/summary.json`，二进制 SHA-256 `4f3290a3a65f0cd3b1fc866866fca7f11b6a5d1b912e0fca81c798a9436a45fb`，fixture SHA-256 `81c676c12fa6730b7f23adc0e6b6c95492c7e4c7713767584f5df7f8041b7191`，源库哈希保持。Replace 2,777 ms / USS 增量 68.0 MiB；Merge 4,479 ms / 67.6 MiB。成功恢复 50,000 条输入记录，总时长均为 1,500,000,000 ms；Merge 另保留原会话。两种末条失败耗时 2,665/3,526 ms，USS 增量均约 65.8 MiB，原记录/设置和 quick_check 通过。
- 中间对照 `/tmp/patina-backup-bench-OelbUb/summary.json` 测得 Replace 4,016 ms、Merge 6,006 ms，USS 增量同为约 68 MiB；不隐去耗时波动。两次使用相同生成规则而非逐字节相同归档，不能把单轮时间差精确归因于解析器。流式解析降低内存，但比 `from_str` 更耗 CPU；这批不承诺恢复加速。报告的 5 秒/64 MiB 预算仅应用于导出/预览，恢复是观测项目，不因 `passed=true` 宣称其已通过相同预算。
- `legacy-preview` 模式代表“完整 payload 再取预览”的对照，现在也复用新读取器，不是冻结的旧版二进制；改前后比较使用上列两份独立证据。最终预览约 529 ms，含 SHA-256 约 605 ms；异步连续预览时隔离追踪 65 次 tick、最大间隔 34 ms，数据库完整性通过。剩余完整 payload/ID map、多表峰值、磁盘故障、崩溃 receipt 与真实 systemd 恢复体验仍需独立验证，不宣称恢复链路恒定内存或稳定版门槛全部完成。
- 初轮 Rust 检查在受限环境因回环监听 `PermissionDenied` 失败，授权本机监听后通过。读取优化后 `npm run check:full` 退出 0：621 项 Rust 测试通过、11 项 opt-in 忽略，36 项浏览器回归、Clippy `-D warnings`、边界和构建预算通过；浏览器临时 profile 仍有既有 `ENOTEMPTY` 清理警告。晚期损坏用例新增尾随 JSON，两种策略均拒绝。release worker 的恢复总时长断言另行执行，不以默认忽略测试的编译代替运行。不修改用户已安装 beta.18、生产数据库或 systemd 服务，不构建新 DEB。

#### 备份预览调度与 release 验证（2026-09-19，未出包）

- `data/backup/inspection` 是本次调度 owner：公共异步预览及归档检查共用进程内单 worker 信号量，取得许可后才启动 blocking worker。等待方取消不启动工作；已启动工作在完成前继续持有许可，不因调用方取消提前放行。这里只限制重工作并发数，不限制等待队列长度，不跨进程串行化；同步维护/retention 与独立导出许可不在此门内。
- Settings 合并同一运行时 adapter 的进行中选择文件/预览请求，完成、取消选择或失败后释放，不缓存预览结果；再次预览同一路径仍重新验证。恢复确认、策略及写入事务未改变。
- `node scripts/perf/backup-benchmark.mjs --release` 用独立进程测 50,000 条合成记录，新增纯预览、纯 SHA-256 和组合检查阶段。证据：`/tmp/patina-backup-bench-nvb6gv/summary.json`，release 测试二进制 SHA-256 `93eaeb4df04b3635b2204c0e5ebb42573f66cc4a777240c9f1f15447703c35dc`；fixture SHA-256 `6c9a66a4df24f26224341c9099e948d7069e261c6706e1a1ec902fd5ad3b913e`，基准前后不变。

| 路径 | 耗时 | 采样 USS 峰值增量 |
| --- | ---: | ---: |
| 旧全量导出 | 1,121 ms | 297.7 MiB |
| 流式导出 | 387 ms | 6.3 MiB |
| 旧全量预览 | 214 ms | 124.7 MiB |
| 流式预览（不含 SHA-256） | 416 ms | 2.7 MiB |
| 单独 SHA-256 | 52 ms | 0.5 MiB |
| 流式预览 + SHA-256 | 484 ms | 2.9 MiB |

- 新路径通过本 fixture 的 5 秒/64 MiB 增量预算；单阶段和组合计时来自不同进程，不要求严格相加。这是 release 测试 worker，不是 DEB、真实用户大库、Desktop/WebKit 整链路或完整恢复事务峰值，也不承诺所有大小的归档都在半秒内完成。
- 额外隔离库在连续三次公共异步预览期间，通过真实 `TrackingRuntimeDataStore` 写入 heartbeat/title：总耗时 1,294 ms，56 次 tick，最大间隔 29 ms，57 条标题样本，会话成功封口、SQLite quick_check=ok。没有运行真实 GNOME provider、systemd daemon 或生产追踪，不代替实机长时间验收。
- 自动验证覆盖取消后 worker 仍持许可、排队取消、错误后重试及前端重复请求。新增浏览器慢预览用例验证定时器继续推进、按钮禁用、只发一次预览、取消策略框不执行恢复；其后端是受控 mock，不是原生文件对话框验收。
- `npm run check:full` 退出 0：621 项 Rust 测试通过、11 项 opt-in 忽略，36 项浏览器回归通过，Clippy `-D warnings`、架构与构建预算通过。index gzip 73.19 KiB、总 JS gzip 359.68 KiB；浏览器临时 profile 的既有 `ENOTEMPTY` 清理警告仍存在。完整恢复 payload/ID 映射仍在内存中，下一步先做隔离 release 恢复峰值与事务失败验证，再决定是否需要逐记录恢复，不能以本表宣称全链路恒定内存。

#### 分类候选有界聚合：功能接线（2026-09-18，未出包）

- 查询级新旧对照已通过：`node scripts/perf/daily-activity-benchmark.mjs --observed-apps` 使用 50,000 条合成 native facts，结果总时长同为 3,000,000,000 ms，数据库哈希不变。旧分类 SQL/JSON 参考路径 1,558 ms / 8,338,895 bytes / USS 增量约 110.4 MiB，新查询 695 ms / 122 bytes / USS 增量约 23.2 MiB；通过本 fixture 的 5 秒、64 KiB、64 MiB 增量预算。证据 `/tmp/patina-daily-bench-SZ8vCe/summary.json`。这是 debug 查询进程而非 Desktop/WebKit/IPC 整链路，也不代表多应用、大量导入或所有合法输入的峰值。
- owner：`data/repositories/observed_apps` 负责单事务紧凑 facts 读取，复用领域 native/import 优先级编译器；`domain/observed_apps` 定义范围与返回契约；HTTP handler、typed daemon client 和 Tauri command 只做适配。Desktop 内置与 daemon 不新增平行统计实现。
- 新增只读 `/api/v1/classification/observed-apps`，三个 surface 均可用，OpenAPI 与 API docs 同步。日常分类候选只接收 raw exe/app name/总时长/最近片段起点，不传 session 明细或窗口标题。前端仍负责别名合并、进程过滤和最终 120 项限制；被排除应用必须保留，不能误用热力图排除逻辑。
- 明确预算：最多 366 天、50,000 facts、每个名称 1,024 UTF-8 bytes、累计 metadata 8 MiB、4,096 raw exe、编码响应 1 MiB；进程内一个查询，仓储 timeout 15 秒。超限报错，不返回局部统计、不回退 SQL 明细。仅该 typed endpoint 放宽原有 64 KiB 响应上限到 1 MiB，其他接口不变。SQL 扫描/排序与共享编译器的实际峰值仍需测量，预算不等于已证明 RSS/PSS 上限。
- `last_seen_ms` 保持旧分类页“最后 resolved start”的含义；桶只表示 scoped start，不伪称准确网页/应用使用位置。raw 大小写保留给前端既有别名流程；native 零时长证据保留。源类型/id 显式排序使原来未定义的 SQL 同时刻 tie-break 稳定，跨语言共享 fixture 验证时长、裁剪、名称、原生遮盖导入和部分小时桶。
- 分类页加载失败现在显示错误与显式重试，不再因 draft 缺失永久转圈；保留缓存中的草稿。相同 bootstrap 进行中请求合并，避免 React StrictMode 重复请求撞上 daemon 单查询预算，失败后释放以便重试。新查询 adapter 按需加载，分类文案留在 feature，未提高 bundle 预算。
- 旧版全历史自动分类迁移仍使用原 reader，其他趋势、网页候选和流式备份尚未迁移。本轮不变更安装版本、不出包或推送；新接口不在既有 beta.18 DEB 中。下一步是同数据的新旧查询耗时/峰值与真实 IPC/daemon 验收，再推进其余趋势及备份；不能以小 fixture 或响应缩小代替内存验收。
- 验证收口：最终 `npm run check` 与 `npm run check:rust` 均退出 0，覆盖完整检查链；611 项 Rust 测试通过、10 项 opt-in 忽略，35 项浏览器回归通过，Clippy `-D warnings` 通过。共享 fixture、50,000 facts 汇总及第 50,001 条拒绝、metadata/响应/应用数预算、三个 API surface、鉴权和旧 daemon 错误、bootstrap 并发合并均有覆盖。初轮失败分别为 fixture 指纹未满足 schema、故障注入日志未从预期错误中区分，以及端点计数断言未更新，均已修正复核；不隐去这些失败。
- 隔离浏览器失败/重试与 760/1280px 截图通过，截图位于 `/tmp/patina-observed-ui-85KrMD/`，未访问生产数据。最终 index gzip 73.19 KiB、总 JS gzip 359.61 KiB，保持 73.25/370 KiB 原预算；浏览器临时 profile 的既有 `ENOTEMPTY` 清理警告仍存在。50,000 facts 仅证明输出和预算行为，不是峰值内存测量。

#### 流式备份第一步（2026-09-19，未出包）

- 上述分类有界聚合、挂起验收与本步导出改动已按用户要求提交为 `18504ff`，未推送。后续小批独立提交，不把本地提交等同于安装或公开发布。
- owner 保持在 `data/backup`，新增内部 `streaming` 实现，Desktop 与 daemon 复用。单一 SQLite read transaction 下逐行序列化到 64 KiB 缓冲的磁盘 ZIP，不构造整表 Vec、整表 pretty JSON 和最终 ZIP Vec；继续使用原 manifest、entry names、CRC32 和 Stored 格式，不变更备份版本。旧全量导出只保留为测试对照。
- 进程内一次导出，其他调用异步等待；阻塞文件写入移到 blocking worker。调用方取消后 worker 在后续行/写入及发布前检查取消标记；已开始的原子发布不能承诺可撤回。总 entry/归档预算保持不变，不声称单行或 SQLx 预取内存等于 64 KiB。
- 临时文件随机命名、`create_new`、0600，同目录完成后 fsync 再发布。人工导出原子 rename；定时导出使用 hard link 保证目标已存在时不覆盖，文件系统不支持时明确失败。失败或取消只清理本次临时文件，不扫描删除目录其他文件。发布后同步目录失败可能报告错误但目标已存在，不能描述为任何错误都未发布。
- 已补新旧快照字段对照、标题转义/NULL/web-native 关系/Tools 数据、重复定时导出、读取中途失败保留旧文件、取消及字节预算拒绝测试。已有备份读写、恢复事务和权限测试继续保留。
- 本步完整 `npm run check:full` 退出 0：614 项 Rust 测试通过、10 项 opt-in 忽略，35 项浏览器回归、Clippy `-D warnings`、架构和构建预算通过。index gzip 73.19 KiB、总 JS gzip 359.61 KiB，未提高预算。浏览器临时 profile 的既有 `ENOTEMPTY` 清理警告仍存在；没有以单元测试代替磁盘故障或大库峰值实验。`git diff --check` 通过。
- 定时备份校验复用现有恢复检查和分块 SHA-256，不再额外读取完整 ZIP 来计算 hash；但恢复解析/预览仍持有完整归档和 payload，尚未实现校验、预览、恢复整链路有界内存。还需补并发写入快照实验、真实取消/磁盘故障、大库峰值及端到端验收，不能将本步标记成流式备份全部完成。当前不改 beta.18 安装版、不出包、不提交推送。

#### 备份读取第二步（2026-09-19，未出包）

- 归档读取改为 `ZipArchive<File>`；解码器泛化到 `Read + Seek`，仍用原格式、checksum 和恢复语义，测试可继续使用内存归档。移除 `fs::read` 整包 ZIP 副本，不同时持有完整 ZIP 与全部条目文本。
- 保留原路径检查，打开后再次检查实际文件类型和长度；Unix 使用 `O_NOFOLLOW | O_NONBLOCK` 避免最终路径替换为符号链接或 FIFO 时静默跟随/阻塞。显式声明锁文件已有的 libc 依赖，未升级第三方版本。条目读取除元数据上限外，还限制实际解码长度；仍保留 CRC/manifest 校验。
- 新增真实磁盘读取与旧编码器对照、损坏 ZIP、截断文件、非普通路径和超大稀疏文件拒绝测试。恢复仍完整持有条目 JSON 与解析后的 payload；这一步没有实现逐条恢复或有界预览，也未测量大库峰值。下一步应逐条目验证后释放 JSON，再处理预览/校验的流式计数，保持事务失败不部分恢复。
- 验证：`npm run check:rust` 退出 0，616 项测试通过、10 项 opt-in 忽略，Clippy `-D warnings` 与 Rust 边界检查通过；`git diff --check` 通过。初轮编译发现 libc 未直接声明，补显式依赖后离线检查与完整 Rust 检查均通过。前端未改动，沿用前一步完整检查证据，未重复浏览器构建；未打包或访问生产数据。

#### 备份读取第三步（2026-09-19，未出包）

- 前一步磁盘读取已提交 `04cb249`，未推送。本步继续在 `data/backup` 内收口，不增加平台/客户端恢复实现。
- 改为先读取 checksum 元数据，再逐条目读取、校验和解析。内部 helper 要求 `DeserializeOwned`，原始 JSON 在函数返回时释放，不再保留所有条目的 JSON 集合和 checksum 引用列表。manifest 同样校验后再使用；可选条目仍按 manifest 声明或 checksum 中存在判断，未声明时保留旧版默认行为。
- 全部条目成功解析前不进入恢复写事务；新增晚期 imported activity 条目损坏、缺失 checksum、有效 checksum 下的非法 JSON，以及不支持算法的故障用例。Merge/Replace 均检查原设置保留、session 表没有被部分写入。
- 限制：内存仍包含完整已解析 payload 加当前最大条目的 JSON，未实现逐记录恢复、流式预览计数或恒定内存；也未重新测量大库 PSS/USS。多个错误并存时首个报错可能随读取顺序改变，但不能静默接受损坏归档。生产数据、安装版与服务未改动。
- 验证：`npm run check:rust` 退出 0，617 项测试通过、10 项 opt-in 忽略，Clippy `-D warnings` 和 Rust 边界检查通过；`git diff --check` 通过。泛型类型推断使旧导入不再用于生产代码，已清理并将测试专用类型放回测试模块。前端未变，沿用此前完整检查证据；当前没有新增成品或推送。

#### 备份预览、校验与性能批次（2026-09-19，未出包）

- `data/backup/preview` 使用 serde 数组 visitor 逐记录反序列化、验证类型后计数并释放，不使用 `IgnoredAny` 跳过类型验证，也不信任 manifest.counts。保留未知字段、旧版未声明可选条目和版本支持提示语义；版本安全规则归 domain 单一函数，预览与恢复共用。
- 预览、定时备份成功/崩溃恢复校验、远端归档检查均复用此读取逻辑。CRC32 随读取累计，检查末尾、JSON 类型、实际 entry 上限和归档预算。检查归档后从同一打开的文件句柄计算 SHA-256，避免路径被替换造成重新打开另一文件；这不是对同 inode 原地改写的完整防护。
- 桌面预览、定时成功/恢复校验、WebDAV 上传下载后的检查移入 blocking worker；原有 Desktop/daemon staging 已有的 blocking 调用保持。旧备份 retention 的核验/删除仍保留同步安全边界，不在本批改变删除任务取消语义；不声称全部备份 I/O 均已异步化。
- 新增单条记录存活测试、实际数量与虚假 manifest 数量对照、未来/旧版本提示、旧可选文件兼容、异步/同步预览与 SHA 对照；损坏晚期条目同时验证预览拒绝及恢复不部分写入。真实临时 SQLite 上并发修改 sessions/settings，并连续导出 10 次，跨表 generation 一致。导出目标为已有目录或定时目标为符号链接时失败并保留原对象，临时文件清理受验证。未覆盖断电、磁盘满或所有取消时序。
- 新增 opt-in `node scripts/perf/backup-benchmark.mjs`：私有 HOME/XDG、独立进程、50,000 条带约 1 KiB 合成标题的记录，不发现生产 profile。旧/新导出和完整 payload/流式预览分开测量，数据库哈希不变，导出结果计数及 quick_check 通过。证据 `/tmp/patina-backup-bench-XAoGMi/summary.json`，测试二进制 SHA-256 `ac86fc47f89a1ba2e952b5c8d1c138882e1e564b7ebc055581f2118ddad63b80`；测量在异步接线补丁前完成，流式算法未再改变。

| 合成 debug worker | 旧路径 | 新路径 |
| --- | ---: | ---: |
| 导出 USS 峰值增量 | 299.0 MiB | 9.7 MiB |
| 导出耗时 | 3.442 s | 2.930 s |
| 预览 USS 峰值增量 | 130.5 MiB | 7.6 MiB |
| 预览耗时 | 1.541 s | 6.964 s |

- 两条新路径通过预设 30 秒/64 MiB USS 增量预算，但流式预览明显变慢，不能描述成全面加速；新预览还包含 SHA-256，旧完整 payload 参考不含 hash。基线已含前两步磁盘 ZIP/逐条目 JSON 优化，不是最早版本。数字是采样的隔离 worker 增量，不是实际桌面 RSS、release 或完整恢复事务峰值。
- 尚未解决：完整恢复仍持有 payload 与 ID 映射；单个超大字段及 metadata 仍受 entry 预算约束，不承诺固定 64 KiB；CPU 时间、并发预览预算及真实客户端体验仍需后续验收。其他趋势/旧分类迁移、AppImage、长期稳定性和剩余安装矩阵不因本批通过而自动完成。
- 验证收口：完整 `npm run check:full` 退出 0；最后的异步接线及 hash 对照补测后，`npm run check:rust` 再次退出 0，620 项 Rust 测试通过、11 项 opt-in 忽略，Clippy `-D warnings` 与架构检查通过。35 项浏览器回归与构建预算保持通过，index gzip 73.19 KiB/总 JS gzip 359.61 KiB；既有浏览器临时 profile 清理 `ENOTEMPTY` 警告仍未解决。性能 runner 独立执行并通过，`git diff --check` 通过。按用户要求整批提交，不推送、不构建 DEB 或安装，不将自动测试称为生产实机验收。

#### 稳定性补验：活动网页跨挂起（2026-09-18）

- 真实 beta.18 补验：用户确认在 Zen 普通网页停留、保持前台挂起、恢复后继续浏览再切走。systemd 日志确认本地时间 23:22:41 至 23:22:49 挂起，检查区间为 `[1789744961000,1789744969000)`；只读查询原生会话、网页片段、标题采样与该区间的重叠数均为 0。未读取标题、域名或 URL。
- 挂起前最后网页与原生会话于 `1789744958904` 封口，比进入内核挂起提前约 2.1 秒，可能先由锁屏触发；不能将此单次实测当成“没有锁屏的纯 suspend 路径”证明。恢复后原生会话于 `1789744976459`、网页片段于 `1789744986880` 重新开始；与用户反馈共同证明恢复后继续记录。
- 首轮只读安装验收 `/tmp/patina-beta18-suspend-acceptance.json` 因 5 秒数据库检查失败而退出 1，空 stderr 掩盖原因；独立有界重试 `PRAGMA quick_check` 返回 `ok`。验收脚本现将完整性检查上限放宽到 30 秒，并保留终止/错误消息，不能将无结果描述成数据库损坏。11 项验收测试通过；重跑 `/tmp/patina-beta18-post-suspend-recheck.json` 退出 0，服务、lease PID、API/追踪就绪、Token 0600 和数据库检查通过。`NRestarts=1` 没有挂起前基线，不据此宣称本次重启计数未增加。
- 该项真实 Zen/systemd 联合验收已完成；长期运行、剩余安装/恢复故障矩阵及 AppImage 兼容仍待完成。流式备份源码正在实现，当前仅 `cargo check` 通过且存在待清理 warnings，尚未完成兼容性/故障测试，也未进入安装包。此前全量测试结果不能覆盖这一在途改动。
- 新增 `active_browser_suspend_resume_never_records_the_sleep_interval`，通过真实浏览器 HTTP handler、运行时电源事件入口与内存 SQLite 联合验证；使用合成 Zen 上报与注入时钟，不启动生产服务、不访问用户数据、不挂起电脑。
- 覆盖活跃网页在 suspend 时随 native session 同事务封口，即使网页事件订阅器没有运行也成立；挂起前捕获的前台快照在挂起后到达时不能重新打开旧记录；resume 后尚无新采样时不记录；新 native session 下同一网页产生独立片段，随后 lock 正确封口。两段边界与 duration 精确断言，挂起区间重叠为零。
- 单项测试及完整 `npm run check:full` 通过（退出码 0）：604 项 Rust 测试通过、10 项 opt-in 忽略，34 项浏览器回归、Clippy `-D warnings` 与构建预算通过。浏览器临时 profile 仍有 `ENOTEMPTY` 清理警告，本轮未修复。只补既有行为的回归覆盖，没有改追踪逻辑；beta.18 成品不重建，未安装、推送或发布。
- 这不替代真实 systemd 与 Zen 扩展联合验收，也不覆盖任意电源故障、hibernate 或不同硬件。真实活动网页跨 suspend、长期运行与剩余安装故障矩阵仍为稳定版门槛。

#### beta.18 候选与原生计时验证（2026-09-18）

- 新增既有 runner 的 `--background-delay` 模式，使用私有 HOME/XDG/D-Bus、空合成数据库和静态页面；不启动 tracking、不控制生产服务、不访问用户数据。
- `node scripts/native-window-lifecycle.mjs --background-delay` 实测通过，耗时 263.48 秒，退出码 0。确认一分钟隐藏期间提前重开不会被旧 timer 销毁；隐藏后关闭低耗开关保留主窗口；改长延迟使旧 timer 失效；改回一分钟后自动销毁并回收一次；随后可以重新建窗，数据库完整且无 session 写入。证据 `/tmp/patina-window-test-KA41xS/result.json` 与 `native.log`，runner 已结束并清理私有进程组。
- 原生测试二进制编译时版本仍为 beta.17，与最终 beta.18 的生命周期实现一致；不把它描述成安装版或完整 React 测试。隔离 portal 仍有缺少 PipeWire 等警告。持续追踪、实际产品页面与用户环境多轮操作仍在安装验收范围内。
- 版本文件统一为 beta.18，Cargo.lock 只修改本项目版本，未更新依赖。完整 `release:check` 退出码 0：603 项 Rust 测试通过、10 项 opt-in 忽略、34 项浏览器回归、Clippy、构建预算、扩展与签名 XPI 检查通过。日志 `/tmp/patina-beta18-release-check.log`。
- 本地未签名 DEB 已构建，构建命令显式 `createUpdaterArtifacts:false` 且清除签名环境变量，不读取私钥、不生成 updater 清单。成品为 `src-tauri/target/release/bundle/deb/Patina_1.9.0-beta.18_amd64.deb`，25,833,602 bytes；SHA-256 `1e20c36c22ca8850c73294fc1d15cb4787200a4287d4a0ba7d863654b32a4e6e`。`release:verify-daemon-deb` 通过，构建日志 `/tmp/patina-beta18-build.log`。
- 解包逐字节核对 daemon 与 release 构建一致；Desktop 仅允许 Tauri 的 `UNK` → `DEB` bundle marker 差异。包内 Desktop SHA-256 `eb0d6fec97255ef693f9e15c2aca637e4937de747a1f7d8a2794c87969edf1c7`，daemon SHA-256 `a4ac4057a08329e3c04206a74aa898d4ca04cd30e099a9ff92da94bf627022c7`。
- 实际解包 daemon 使用全新临时 HOME/XDG、Local 空库和随机端口，不启用 tracking：capability 版本为 beta.18、tracking owned=false、Token 文件 0600；未认证 heatmap 返回 401，反向区间 400，两天合法响应 200/202 bytes/零时长，SIGINT 后退出码 0。结果 `/tmp/patina-beta18-package-LABL8q/result.json`。未操作生产服务或数据，未验证已安装版本；不把空库冒烟当作持续追踪验收。
- 下一批按已授权路线继续稳定性补验、分类/趋势有界聚合、流式备份与 AppImage 兼容；不以等待人工安装为由重新请求每一项开发授权。本候选人工验收仍需设置一分钟后关闭到托盘、提前重开取消、持续追踪和真实 Data/History 页面确认。

#### 当前小批：低耗后台延迟（2026-09-18，未出包）

- 在 Desktop 常驻设置内增加“低耗后台延迟”滑块，范围 1–60 整分钟，默认 5 分钟；复用现有保存/取消流程与设置事务。低耗开关关闭时控件禁用但保留数值。中文/英文同步。
- 持久化键为 `background_optimization_delay_minutes`。Rust 写侧严格拒绝越界、非整数；旧配置缺失或损坏读取默认 5 分钟。daemon 仅作为设置数据 owner 持久化此偏好，不新增追踪策略；Desktop 启动加载或设置刷新后经本地命令应用。
- 隐藏主窗口按设置等待；重开使旧 generation 失效。运行时应用新的低耗开关/延迟会使原计时失效，若仍隐藏且开启低耗则从应用时重新等待完整时长；最终检查和销毁放在 UI 线程，避免检查后与重开交错。相同设置重复同步不重置计时。
- 不改变最小化到任务栏的行为、Widget 闲置保留时间、Data/History 五分钟返回首页策略、默认低耗开关或 daemon 追踪。悬浮窗专项继续暂停。
- 复用现有整数范围解析，没有引入依赖。入口压缩体积 74,798 字节（73.04 KiB），超过旧 73 KiB 上限 46 字节；因新增设置持久化/运行时字段与文案，将入口预算明确调整为 73.25 KiB，总 JS 预算保持 370 KiB，实测总量 358.65 KiB。没有为满足体积预算移除校验或改变加载架构。
- 隔离浏览器截图确认 1280/760px 控件可读、无重叠，测试预览 `http://127.0.0.1:1422` 只用假数据，不连接生产库或 daemon。自动测试不等于真实 GTK/WebKit 定时回收验证；下一候选仍需测试一分钟关闭回收、提前重开取消，以及后台持续追踪。此前 beta.17 的五分钟安装验证不能冒充新时长验证。
- 最终 `npm run check:full` 通过：603 项 Rust 测试通过、9 项 opt-in 测试忽略，34 项浏览器 smoke 通过，Clippy `-D warnings`、构建及预算通过。单元覆盖默认值/边界、非法事务不部分提交、API 写侧、策略锁内更新与生命周期 generation；浏览器覆盖保存/取消/禁用保留/刷新恢复。日志 `/tmp/patina-background-delay-final-full.log`；保留浏览器临时 profile 清理 `ENOTEMPTY` 警告，不宣称清理测试通过。源码版本仍为 beta.17，用户安装的包不含本批改动，未提交、推送、构建新 DEB 或安装。

#### 日聚合第一步（2026-09-17，未发布）

- 已编写共享仓储 `daily_activity` 和只读 `GET /api/v1/heatmap`，覆盖三个 API surface 与字段级 OpenAPI。单一 SQLite read transaction、逐日紧凑 facts、固定 active 采样时间；复用领域优先级和小时桶分配算法，排除在优先级处理后执行，不传输标题或 URL。
- 预算明确为最多 378 天、每进程一个查询、30 秒仓储超时、每日 20,000 facts、20,000 排除设置、app key 最多 1,024 UTF-8 字节。超限失败，不截断统计或回退全量读取。SQLite 排序及实际峰值内存仍需实验确认，不能仅凭 Vec 上限宣称端到端内存达标。
- 发现客户端兼容门槛：原桌面 `shouldTrackProcess` 还有临时进程、安装器与标题相关过滤，现有 API compiler 没有完全相同的语义；前端小时桶还会转换成区间，按日切分不一定等同于领域按日分配。下一步先补共享 fixture 和差异判定，再决定迁移位置，不能直接替换并静默改变历史统计。
- 本步骤结束时 Desktop 热力图、53 周明细缓存、Tauri adapter 尚未切换，不能宣称桌面内存已下降。版本保持 beta.16，不出包、不安装、不推送。
- 验证：`npm run check` 与 `npm run check:rust` 分别通过，覆盖完整检查链；Rust 587 passed / 7 ignored，Clippy `-D warnings` 通过，浏览器 smoke 31 项通过，前端构建和 bundle 预算通过。新增六项 Rust 测试覆盖聚合对照、排除顺序、23/25 小时边界、active 截止、空库、预算失败、三个 surface 的路由和日期参数校验。没有执行生产数据库查询或安装操作。
- 环境记录：沙箱内发布脚本子进程输出及 loopback bind 测试受限，沙箱外复核通过；浏览器 smoke 留有一次临时 profile 清理 `ENOTEMPTY` 警告，不把临时目录清理声明为通过。真实 DST 时区端到端、SQLite 查询计划及 Desktop/WebKit/daemon PSS/USS 对照仍未验收。

#### 日聚合第二步：桌面缓存收口（2026-09-17，未发布）

- Desktop 的热力图 snapshot、两项 LRU 缓存、页面 state 和首屏预热改为持有 `date + duration` 日汇总，不再保留整年 session 数组。最近范围 371 项，单年范围最多 378 项；其余趋势页面缓存不属于本次改动。
- 同范围的页面查询与预热复用一个 pending 请求；清理缓存时递增 generation，迟到结果不能重新填充缓存，旧任务结束也不能移除新任务。暂时仍通过既有 SQLite adapter 查询明细，只有查询结束后的驻留数据收敛，SQLite/JSON/IPC 瞬时峰值尚未消除。
- 热力图使用下一本地日期的午夜作为 day end，不再固定加 24 小时；年份周数按日历日期差计算。读取的 session 先裁剪到显示范围，避免对超长范围无意义遍历。此处修复夏令时漏计/重计，不改变安装器等历史进程过滤、现有热力图排除设置行为或小时桶位置语义。
- `daily-activity-cases.json` 由 Rust 和 TypeScript 共用，记录 7 组按日优先级/排除/active 截止/桶分配场景，并显式记录旧桌面结果。13 组桌面进程过滤用例保留 metadata 敏感规则，不能用 tracking 写侧的 `should_track` 替代：两者原有职责和过滤集合不同。
- 已确定迁移差异：旧热力图不应用用户 app exclusion；跨午夜小时桶先按整段范围分配、再伪装成桶起点的连续区间，新 API 则按每日窗口分配。后端接管前必须完成历史过滤的读侧迁移，并明确这些修正的兼容行为，不能仅按总量碰巧一致验收。
- 本步骤的 5 万条合成 session 测试验证日汇总无原对象引用、序列化长度不到原明细的 1%；这只是结构与编码体量检查，不是 Desktop/WebKit 的 PSS/USS 实测。新增 UTC、新加坡、平壤、纽约和 Lord Howe 的隔离 TZ 测试，覆盖 23/25 小时、半小时 DST 和历史时区回退时年份多出一周的问题。
- Rust 时区回归发现 Apia 被跳过日期的午夜仍可能被底层转换接受，已添加 UTC timestamp → Local round-trip 验证，明确拒绝不存在的午夜，不改为无声归一到下一天。
- 验证：完整检查链的前端部分通过，包含 31 项浏览器 smoke、构建及 bundle 预算；Rust 首次运行由新增 Apia 用例暴露问题，修正后 `npm run check:rust` 通过（589 passed / 7 ignored，Clippy `-D warnings` 通过）。共享 7 组日聚合 fixture、13 组桌面过滤用例、30 项 Data read-model 测试通过；补充平壤用例后 `npm run test:data-range` 复核通过。尚未进行真实生产数据的内存验收。
- 下一步仍是后端统计规则对齐与客户端 adapter，而非发布或恢复悬浮窗专项。本地版本仍为 beta.16，未生成新 DEB、安装或推送。

#### 日聚合第三步：历史过滤迁移（2026-09-17，未发布）

- 新增领域读侧 `activity_read_policy`，与实时 tracking 过滤保持独立；日聚合在优先级分配后应用其判定。没有改写其他 summary endpoint 或追踪写入规则。
- 原有 117 项内置映射提取为 `src/shared/classification/defaultMappings.json`，TypeScript 导入、Rust 编译期嵌入同一份静态兼容目录。791 组共享 golden fixture 覆盖别名、大小写、引号、安装器、版本后缀、特殊空白及标题条件，防止双实现漂移。
- 每日紧凑 facts 仅保留是否计入的判定；只对 metadata 敏感的 exe 在同一事务内逐行查询 app name/title，不将标题加入 UNION 排序或长期缓存。app name 上限 1,024 UTF-8 字节、title 上限 16,384 字节，超限拒绝，不截断分类后继续统计。
- 尚未切换 Desktop adapter。下一步收口用户排除/别名和导入桶分配差异，再接共享聚合入口；随后以查询计划及隔离 PSS/USS 测量验收，不把规则迁移视为内存收益实测。
- 本步骤不改悬浮窗、不访问生产数据库、不打包安装、不提交推送，版本仍为 beta.16。
- 验证：`npm run check:full` 通过，含前端全部检查、31 项浏览器 smoke、构建与 bundle 预算；Rust 591 passed / 7 ignored，Clippy `-D warnings` 通过。共享 791 组过滤案例在两端均通过；仓储测试覆盖标题条件过滤、普通应用的大标题不参与读取、敏感 metadata 超限拒绝。浏览器临时 profile 清理仍有 `ENOTEMPTY` 警告，未宣称该清理问题已解决。

#### 日聚合第四步：排除与别名收口（2026-09-17，未发布）

- 核对真实写入链后发现，前三步的排除读取只看旧 `__app_excluded`，会漏掉设置页及 API 当前写入的 `__app_override`。已修正日聚合仓储，不改追踪写入、不迁移数据库，也不顺带扩展其他 summary endpoint。
- `activity_read_policy::canonical_executable` 与前端共用 598 组别名 fixture；记录 exe、override key 与 legacy key 使用相同 canonical identity。历史过滤仍由原有 791 组 fixture 校验。
- 当前 override 优先于旧排除字段，只有 literal `track: false` 且未 `enabled: false` 时排除。显式禁用当前 override 不恢复陈旧 legacy 排除。不存在当前 override 时兼容旧 boolean；当前 alias 决策冲突、JSON 损坏或不是 object 时整次失败，不用数据库偶然行序决定时长，不自动修复设置。
- 设置预算改为两类 key 合计最多 20,000 项，key 最多 1,024 UTF-8 字节、value 最多 16,384 字节。所有读取仍在同一 snapshot 内，超限不截断结果。
- 相对旧 Desktop 热力图，迁移明确包含两项统计修正：应用排除设置实际生效；无精确位置的小时桶按每日交集比例分配，不再假定活动从桶起点连续发生。共享 fixture 保留新旧差异，仓储回归覆盖现代 override 排除的本机记录仍抑制导入记录，以及跨边界 bucket 的比例结果。
- 下一执行步为 typed daemon client、embedded 共用仓储的薄 command，以及前端日汇总 adapter。需要显式呈现旧 daemon/超限/查询失败，不能静默回退到旧的整年明细查询；接入后再做查询计划、响应体积、时延和 PSS/USS 验收。
- 当前未切换 Desktop 查询、未做内存实测、未改悬浮窗、未出包安装或提交推送，版本仍为 beta.16。其他 summary endpoint 的现代 override 一致性是独立遗留缺口，不声明本次修复覆盖所有统计接口。
- 验证：`npm run check:full` 通过，包含前端测试、浏览器 smoke、构建和 bundle 预算，Rust 594 passed / 7 ignored，Clippy `-D warnings` 通过；791 组过滤、598 组别名与 7 组日聚合 fixture 对照通过。新增回归覆盖当前 API 写入后的排除读取、alias 合并、legacy 优先级、disabled override、损坏/冲突/超限设置、读取不修改设置及跨边界桶分配。`git diff --check` 通过。

#### 日聚合第五步：桌面接入（2026-09-17，未发布）

- 开发版 Data 热力图与首屏预热改走 `dailyActivityRepository`，经 `cmd_get_daily_activity` 在 daemon 模式转发 typed client，embedded 模式调用同一日聚合仓储。前端生产 loader 不再调用 `getSessionSummariesInRange`，其他趋势页面仍保持原读取链路。
- `domain/daily_activity.rs` 持有 DTO 与严格本地日历日期解析，HTTP handler 和 IPC command 共用，避免两套 DST 规则。typed client 对本请求使用 18 秒超时、保持 64 KiB 响应上限和 Bearer 认证；服务端仍为 15 秒 handler / 30 秒仓储预算。错误不会切换到 Desktop 的数据库查询。
- 前端对 1–378 天的响应逐日校验边界、数量与安全整数，拒绝时区不一致、缺天、负时长及非法采样时间。最早活动时间与 totals 从同一次 snapshot 更新；cache key 包含边界 timestamp。
- 查询失败时不展示零值或旧图冒充新结果，显示错误和图标重试按钮；旧 daemon 的 404 显示更新/重启提示。保留已有 Quiet Pro tokens，未改正常页面布局。旧 bootstrap 因 `heatmapReadVersion: 2` 失效，避免沿用旧排除/小时桶语义。
- 当前只是源码功能接入。下一步执行同一合成数据上的查询计划、响应体积、耗时及 Desktop/WebKit/daemon 峰值 PSS/USS 对照；验收后才组装下一个 DEB 候选。不对已安装版本作变更，版本仍为 beta.16，未提交或推送。
- 验证：`npm run check:full` 通过，Rust 595 passed / 7 ignored、Clippy `-D warnings` 通过；33 项 Data read-model 测试、32 项真实浏览器 smoke、前端构建和 bundle 预算通过。新增 HTTP transport 测试覆盖认证 header、参数拒绝、404/401/非法 JSON/响应大小上限；前端覆盖残缺和错位响应、重试、旧缓存失效及 DST adapter。
- 另以 `PATINA_UI_SCREENSHOTS_DIR=/tmp` 运行浏览器 smoke，复核 1280/760 宽度的热力图错误状态截图与日期跳转/重试流程，无水平溢出或控件重叠。这些使用隔离数据和 Tauri mock，不替代原生 WebKit 与真实 daemon 的整链路手工验收。临时浏览器 profile 清理仍有 `ENOTEMPTY` 警告，未声明已修复。

#### 日聚合第六步：查询级性能验证（2026-09-17，未发布）

- 新增 `npm run perf:daily-activity`，仅在 `/tmp/patina-daily-bench-*` 中生成 50,000 条本机合成 session（每条 60 秒，约 1 KiB 标题），查询 365 天。无导入事实和网页数据，不与早期 50k session + 网页实验混作同一基准。独立只读进程对照真实旧 SQL/JSON 参考路径与日聚合；不读取正式数据、不启动追踪或桌面服务。
- 首轮发现日聚合耗时 10,857ms：SQLite 按 start_time 索引逐日回读带标题的数据页。改用现有 native/import-exact 覆盖索引，小时桶改为双边界索引查找，无新 schema/index 迁移。完整 UNION 查询计划加入普通回归测试；覆盖索引仍逐日扫描，未证明多年份或大量导入数据的时延达标，SQLite 临时排序仍需纳入整链路测量。
- 修正后两轮日聚合分别为 3,613ms / 3,750ms，通过 5 秒、64 KiB 响应和 64 MiB 采样 USS 增量预算。最后一轮旧明细响应 64,688,895 bytes、USS 增量约 287.7 MiB；日聚合响应 25,531 bytes、USS 增量约 5.4 MiB。两端总时长均为 3,000,000,000ms，quick_check 为 ok，数据库读取前后 SHA256 不变。旧路径耗时在 3.17–4.50 秒波动，不宣称新路径稳定快于旧路径。
- 证据：首轮 `/tmp/patina-daily-bench-U1uui8`；修正后 `/tmp/patina-daily-bench-ngIFYv`；最终 runner `/tmp/patina-daily-bench-lr9Rnq/summary.json`。最终测试二进制 SHA256 为 `de500e0e383a0085a8f2bf0fa6133a89f317a2a80159e5ca2c64a2ebf5e1a266`，合成数据库 SHA256 为 `2e9eae3ad966d7aa54664aebc022e0ba9c707b7c0c437e7209bfdf89b9ae2162`。
- 这是 debug 查询进程的 10ms 采样，不是精确分配峰值，不包含 Tauri IPC、HTTP、JS、WebKit 或 daemon host；旧参考路径不精确复刻 `tauri-plugin-sql` 内部。查询级门槛通过不等于用户桌面已降低相同内存。下一步仍是隔离 Desktop/WebKit/daemon 整链路峰值、产品内热力图与日期跳转验收，再决定 DEB 候选。
- 验证：`npm run check:full` 通过，Rust 596 passed / 8 ignored（新增 benchmark 为显式运行的 ignored test），Clippy `-D warnings`、32 项浏览器 smoke、前端构建和 bundle 预算通过；benchmark 最后一轮 exit 0，脚本语法及 `git diff --check` 通过。浏览器临时 profile 清理仍有 `ENOTEMPTY` 警告，不声明已解决。
- 版本保持 beta.16，未改悬浮窗、未打包安装或提交推送。

#### 日聚合第七步：真实 React/WebKit/daemon 链路（2026-09-17，未发布）

- 新增显式 `npm run perf:heatmap-desktop`。两个独立 debug 测试进程运行生产 Desktop bootstrap（daemon-client preview）与真实 daemon runtime，加载实际构建的 React，走真实 Tauri IPC 和 HTTP typed client，不 mock 日聚合结果。私有 Local HOME/XDG/D-Bus、随机端口；只借用 Wayland 显示服务。50,000 条合成 native session 移到当前最近 348 天内，tracking 暂停，音频/网页桥接及登录偏好关闭，不读取正式活动。
- 最终 `/tmp/patina-heatmap-test-Aiu9xQ/result.json` 为 exit 0 / passed，原生测试耗时 346.97 秒。首次/重开均收到 371 天、25,906 bytes 的 IPC JSON，总时长 3,000,000,000ms，耗时 3,256ms / 3,203ms。两轮热力图和双击昨日格子进入 History、显示合成应用通过；无水平溢出，捕获的 JS error/unhandled rejection 均为空。不替代截图或视觉闪烁验收。
- 沿用实际五分钟销毁计时，不手动 trim；关闭后 WebProcess PID 545529 退出，重开为 554530。Desktop PID 545393、daemon PID 545298 保持不变。正常 SIGINT 退出 daemon 后，sessions 仍为 50,000，总量不变，quick_check=ok、foreign_key_check 为空。

| 阶段观测（MiB，非整机占用） | Desktop + WebKit PSS / USS | daemon PSS / USS |
| --- | --- | --- |
| 首次 Dashboard | 347.0 / 279.5 | 40.5 / 24.3 |
| 首次热力图加载后 | 503.2 / 434.3 | 41.1 / 24.8 |
| 销毁确认后四秒 | 98.5 / 63.8 | 41.0 / 24.2 |
| 重开 Dashboard | 300.8 / 232.8 | 40.8 / 24.2 |
| 重开热力图加载后 | 428.7 / 359.4 | 41.1 / 24.4 |

- 表格使用事件前最后一个完整样本（销毁行为用 `destroyed.json` 后四秒）；首轮 Data 加载区间 Desktop 组采样峰值约 531.0 MiB PSS / 462.2 MiB USS，daemon 约 41.1 / 24.8。Desktop 主进程从 Dashboard 到热力图 USS 仅增加约 2.3 MiB，前台增量主要在 WebProcess；这不能进一步归因为某个组件或泄漏。约 250ms 非原子采样、不含无法归属的辅助进程；test executable 与 release 二进制不同，daemon 的基线不能直接与历史 release 数字比较。未做同配置旧明细 UI 对照，不声称整机改善百分比或全部内存问题解决。
- 编译证据：测试二进制 SHA256 `df64e9ba9f21babbb8b44c7f8eb4b3e9cafe193afbe53c9ad674b306c4bc5add`，前端 index SHA256 `542484da209966e751e49914be0d02e2e8176381f296b9ba388cf8fe1dcd8190`。1182 个样本、两轮页面报告、完整性报告和进程日志保留在该私有目录；临时进程与 D-Bus 已收尾，不删除合成证据。
- 验收发现并修复主窗口通知插件初始化缺少 `notification:allow-is-permission-granted` 的未处理错误，只开放读取权限状态，不开放发送或申请权限，并增加 capability 回归测试。早期尝试另修正测试 profile、debug 固定 1420 URL 和只读 invoke 的观测方式；`F3qSfu` 在首轮 UI 通过后主动中止，以便将 runner 的正常退出信号对齐包内 `KillSignal=SIGINT`，不把中止实验算完整通过。
- 完整检查链的前端部分通过（32 项浏览器 smoke、构建及 bundle 预算）；Rust 边界检查首次拦住测试入口直接 SQL，随后将造数/校验移入 `data/repositories/daily_activity/desktop_fixture.rs`，没有放宽规则。修正后 `npm run check:rust` 通过：598 passed / 9 ignored，Clippy `-D warnings` 通过；新增 fixture roundtrip 检查禁用的外部来源、行数与总量。原生证据采于此测试辅助函数归属调整前，调整后不重复未改变的五分钟窗口链路。脚本语法与 `git diff --check` 通过。
- 隔离 portal 缺少 PipeWire/window-list 的警告与原生 `gtk_widget_get_scale_factor` critical 仍可见，未声明已修复。下一步为 release 成品/候选门禁及安装验收，前台 WebKit 占用、多年份/大量导入数据、真实追踪连续性与长期多轮测试仍需独立验证。当前版本保持 beta.16，未打包、安装、提交或推送。

#### 日聚合第八步：源码发布门禁收口（2026-09-18，未发布）

- 当前工作树的 `npm run release:check` 完整 exit 0：版本文件校验、前端检查、32 项浏览器 smoke、构建及 bundle 预算、Rust 边界、598 passed / 9 ignored、Clippy `-D warnings`、GNOME/Chromium 扩展检查、Firefox 签名 XPI 校验及当前版本 changelog 校验全部通过。日志 `/tmp/patina-release-check-20260918-unrestricted.log`；9 项 ignored 专项不由此命令执行，上一节原生实测证据仍单独保留。
- 初次沙箱运行在时区子进程创建处报 `EPERM`，获准后在沙箱外重跑完整门禁通过，没有放宽检查。浏览器临时 profile 清理仍报 `ENOTEMPTY` 警告，不宣称临时目录清理已修复。
- `CHANGELOG.md` 的 `Unreleased` 已补热力图功能、兼容行为与通知权限修复；清除路线快照中已过期的“下一步接入/整链路验收”提示。版本仍为 beta.16，版本化 changelog 校验针对 beta.16，不代表未来候选版本的发布说明已验证。
- 本轮未生成新 DEB、未安装、未修改生产数据库或服务、未提交或推送。`HEAD` 仍为 `2c60d88`，相对本地远端跟踪引用领先四个提交，热力图工作仍未提交；未刷新远端发布状态。
- 后续候选验收顺序：确定新 beta 版本并同步版本文件和完整发布范围说明；在获准的候选构建/发布流程中生成匹配版本 Desktop + patinad 的 DEB；运行 `release:verify-daemon-deb` 检查成品。安装另行确认，不把旧 beta.16 包当作包含本次热力图的新包。
- 安装后核对 Desktop/daemon 版本一致，再验证热力图最近范围与年份切换、双击日期进入 History、排除规则生效、关闭/重开与后台持续追踪；以同一真实数据记录分组 PSS/USS，不要求用户仅凭单进程 RSS 降幅判断。生产数据上的验证保持只读，排除设置变更及故障注入优先放在隔离 profile。大规模导入/多年份、长期运行和前台 WebKit 内存仍未完成验收，不阻塞已界定的查询优化进入下一 beta 测试。

#### 日聚合第九步：beta.17 DEB 候选（2026-09-18，未安装/未发布）

- 版本文件统一为 `1.9.0-beta.17`，Cargo.lock 仅更新本项目版本，未升级依赖。GitHub 只读查询确认最近公开预发布是 beta.12；beta.17 changelog 因而包含 beta.13–16 的启动、窗口回收与 Wayland 兼容修复，以及本次热力图改动，不将暂停的闪烁专项宣称为完成。
- 新版本 `npm run release:check` 完整 exit 0，598 passed / 9 ignored、32 项浏览器 smoke、Clippy、前端构建与预算、版本/changelog 和扩展检查均通过；本轮未再出现浏览器 profile 清理警告，但没有对该偶发问题作修复声明。日志 `/tmp/patina-beta17-release-check.log`。未重复未改变的五分钟原生专项，第七步证据仍独立有效。
- 本地候选使用明确的 `createUpdaterArtifacts:false` 覆盖构建，仅用于人工安装测试；不读取签名私钥、不生成 updater 清单或签名，也不改变正式发布配置。构建日志 `/tmp/patina-beta17-build.log`，`release:verify-daemon-deb` 检查通过。
- 成品：`src-tauri/target/release/bundle/deb/Patina_1.9.0-beta.17_amd64.deb`，25,817,176 bytes（约 24.6 MiB）；SHA-256 `d0b54b6e54ed9a880817c27beeef9a343755215dcba62873a457be363b95061a`。包名仍为 `patina`，安装将升级现有同名包，不是并行产品。
- 解包后逐字节核对两份 executable：`patinad` 与本次 release 构建完全一致；Desktop 仅允许 Tauri 将 `__TAURI_BUNDLE_TYPE_VAR_UNK` 改成 `__TAURI_BUNDLE_TYPE_VAR_DEB` 的三字节差异。首轮原始哈希严格比较因此中止，核对依赖源码与实际字节后才调整测试，并非忽略未知差异。包内 Desktop SHA-256 `520f5feb1c05dcd88006432adc7f8aeea3aeeb95388de3d34895cd11bacae4c6`，daemon 为 `e79e80e8e5f0c61e4369bc6e22846868ac6f43a6a60e5f1e18fd30bf1d2482b9`。
- 使用解出的 release daemon、私有 HOME/XDG、显式 Local 空库和随机 loopback 端口，不启动 tracking，不使用真实 D-Bus/systemd。版本为 beta.17，Token 文件 0600，未认证 heatmap 为 401，反向日期为 400，合法两天汇总为 200/202 bytes/零时长，OpenAPI 包含 heatmap。SIGINT 后 exit 0，所有测试进程结束。证据 `/tmp/patina-beta17-package-I4u6Up/result.json`；一次性脚本 `/tmp/patina-beta17-package-smoke.mjs`。
- 当前成品尚未安装，未证明已安装 Desktop/daemon 已同步升级，也未重新测量 release UI 内存。下一步按第八步清单进行安装确认、双版本诊断、真实热力图/History、关闭重开与后台持续追踪验收。没有操作生产数据库、安装服务或公开 Release。

#### 日聚合第十步：安装后的只读验收（2026-09-18，人工流程未完成）

- 用户报告已安装 beta.17。只读 `release:inspect-installed-patinad --phase managed --expected-version 1.9.0-beta.17` exit 0；包版本正确、systemd active/running、owner 交接 completed、lease PID 与服务 PID 一致，API 与 tracking/browser bridge ready，数据库 quick_check=ok，Token 为当前用户 0600 普通文件。证据 `/tmp/patina-beta17-installed-20260918.json`。`NRestarts=1` 仅为本次基线，不能据此判断本轮发生崩溃。
- `/proc/PID/exe` 哈希确认正在运行的 Desktop PID 867872 与 daemon PID 868600 均为第九步候选中的二进制，不仅是磁盘安装了新文件。没有自动重启、启停服务、改设置或写入测试数据。
- 用真实 Production 数据作一次有界 371 天 heatmap GET：200、24,429 bytes（约 23.9 KiB）、1,118ms；连续日期与非负整数校验通过，87 天有数据，昨日单日查询与范围中对应日期结果一致。前后 current 的采样时间推进，tracking active、probe ok。该一致性检查不是与另一套独立统计实现的全量对照。
- 安装后两次非原子分组内存快照：Desktop + WebKit PSS 约 594.6 → 604.5 MiB，daemon 约 18.7 → 18.9 MiB。后一个样本中 Desktop 主进程约 151.2 MiB，WebProcess 约 440.8 MiB，NetworkProcess 约 12.6 MiB。期间用户可能操作页面，因此不能把前后差额归因于 API 查询，也不是峰值、泄漏证明或新旧版本对照。证据 `/tmp/patina-beta17-live-check.json`，一次性只读脚本 `/tmp/patina-beta17-live-check.mjs`；不输出标题、URL 或 Token。
- 用户已确认 Data 热力图正常加载，双击活动日期能跳转对应 History。只读设置确认 `background_optimization=1`，关闭/最小化字段未持久化，按领域默认值为 Tray/Widget；用户随后确认已使用主窗口关闭按钮隐藏到托盘，而不是最小化或退出。
- 关闭后进行 330 秒只读观察，67 次 current 查询全部成功、采样时间持续推进；Desktop 与 daemon PID/启动身份保持不变，旧 WebProcess 确认退出，最终无页面进程。桌面组观测 PSS 从回收前约 717 MiB 降至 109.5 MiB，USS 89.4 MiB；daemon PSS 19.0 MiB。监测启动晚于用户点击关闭，不能用脚本开始到进程退出的间隔替代产品五分钟计时。证据 `/tmp/patina-beta17-closed-monitor.json`；不证明每个 tracker tick 或所有 AFK/崩溃场景。
- 关闭后 managed 复核 exit 0，服务 PID 868600 和 NRestarts=1 与基线一致，数据库 quick_check=ok。两次安装采集之间 sessions 51,060→51,080、web segments 70,751→70,763，记录继续产生，计数不等于精确无丢失证明。证据 `/tmp/patina-beta17-after-close-20260918.json`。已请用户从托盘重开并确认 Dashboard/当前活动/Data；尚待结果。
- 用户要求下一批增加可自定义的“低耗后台延迟”，默认保持五分钟，支持较短测试选项；已记录到路线文档的桌面偏好阶段。本轮不改设置、计时或产品代码，以免污染安装验收。

#### 安装反馈：窄窗口热力图（2026-09-18，修复未出包）

- 用户确认从托盘重开后 Dashboard、当前活动和 Data 均正常，同时报告横向分辨率较小时热力图滚动条消失。此反馈完成核心重开人工流程，但不能把窄窗口 UI 验收同时记为通过。
- 复现为 900px 及以下 CSS 断点将 `.data-heatmap-content` 改成 column，配合 `align-items:flex-start` 使未限定宽度的滚动元素被全年日历撑宽，再被 paint containment 裁切。隔离 Playwright/实际 CSS 在 900px 下测得父宽 788px、滚动区域宽 825px、只能滚动 2px；不是 daemon 查询失败。
- 在原样式 owner 中为滚动视口加 `width/max-width:100%`，日历 body 保持 `max-content`，并使用已有 gap 变量统一月份和日期列。没有缩小格子、增加新控件或改查询语义。相同 CSS 复现页修正后视口为 788px，可滚动 39px 到末尾；完整 React 页面回归覆盖 390/600/760/900/901/1280px 与缩窄后恢复。
- 新增浏览器回归校验视口边界、可滚动范围、星期列宽及月/日对齐；原始代码先在月份列错位断言失败，断点撑宽另由隔离 CSS 复现证实。补测试后发现相邻错误恢复测试依赖上一页为 History，已让其自行进入该前置状态，不修改产品行为来迁就测试。
- 最终 `npm run check` exit 0，33 项浏览器 smoke、前端构建及 bundle 预算通过。日志 `/tmp/patina-heatmap-scroll-check-final.log`；浏览器临时 profile 清理仍有 `ENOTEMPTY` 警告，未声明解决。截图 `/tmp/heatmap-scroll-{width}.png`；另用 Playwright 真实横向 wheel 和末尾日期 dblclick 验证 History 导航，截图 `/tmp/patina-heatmap-fixed-760.png`。仅验证热力图区，不宣称整个产品的 390px 移动适配或原生 WebKit 安装验收完成。
- 本轮只改 CSS/测试/文档，未重建 DEB、未安装、未提交或推送。临时预览 `http://127.0.0.1:1422` 复用现有 smoke 假数据与真实 React/CSS，不连接生产数据库或 daemon。下一批仍为用户要求的“低耗后台延迟”设置，届时再合并成候选验收，不为这几行 CSS 单独发包。

## 1. 目标

把当前由 Tauri desktop 拥有的后台追踪主链渐进迁入本地 daemon，使关闭桌面 UI 后仍能可靠记录，并让本机浏览器 UI、桌面客户端和未来 TUI / CLI 复用同一运行状态。

本设计不改变 Patina 的个人、本地优先边界。`patinad` 不是云服务，也不引入账号、团队协作或远程数据库。

## 2. 非目标

- 不在本轮删除 Windows 源码
- 不在本轮重写桌面 UI 或切换到 GPUI
- 不拆分独立仓库
- 不一次性把所有 Tauri command 改成 HTTP
- 不扩大 KDE、wlroots 或移动端支持
- 不为 MCP 提供任意文件操作能力

## 3. 当前 Stage 2H.3 状态

当前分支已经提供并验证：

- `patinad` 二进制入口
- 不依赖 `AppHandle` 的 SQLite pool 打开与 schema 准备
- Production / Local / Dev profile 隔离和 Tauri-free storage anchor 解析
- pending migration 与缺失自定义挂载的 fail-closed 行为
- desktop / daemon 按 profile 唯一 owner 的 `RuntimeLease`
- 按 profile 隔离、owner-only 的 API credential store
- desktop / daemon 共用的受限 HTTP 请求解析与生命周期
- 可选的最小 localhost API
- 与实际能力一致的 `/api/v1/health`、`/api/v1/openapi.json`
- listener、连接任务、SQLite pool 和 lease 的显式关闭顺序
- daemon 专项、生命周期和架构边界测试
- host-neutral `RuntimeContext`、clock 与 `RuntimeEventSink`
- 不依赖 `AppHandle` 的 API handlers 和共享 `ApiRuntimeContext`
- desktop / daemon 共用的完整只读 GET API 与按 surface 过滤的 OpenAPI
- desktop runtime snapshot provider 与 daemon 明确不可用状态
- tracking 启动自愈、电源封口和 watchdog 的共享数据/事件边界
- 真实临时 XDG daemon 进程下的只读 API、写请求拒绝和优雅退出验证
- daemon-owned 有界 `RuntimeEventHub`、进程内单调序号和 replay window
- bearer token 认证的 `/api/v1/events` SSE、`Last-Event-ID` replay、resync 信号和 keepalive
- `/api/v1/capabilities` 宿主/协议能力协商，不把未迁移 owner 误报为 ready
- event stream、listener、连接任务、SQLite pool 和 lease 的有序关闭
- 显式 `--serve-api --track` tracking preview，不改变默认 desktop owner
- daemon-owned tracking/watchdog、实时 tracker snapshot 与 session 写入
- tracking task 取消、有限退避重启，以及 SQLite/lease 之前的有序退出
- 正常退出按最后成功采样时间封口 active session
- host-neutral systemd-logind watcher，覆盖 Manager sleep/shutdown 与 Session lock/unlock/LockedHint
- daemon power task 的取消、退避重连，以及 power → tracking → SQLite 的关闭顺序
- `shutdown` 立即封口和重复 lock/suspend/shutdown 幂等语义
- 显式、可克隆、可取消的 Linux audio source，不依赖 Tauri host
- daemon 按 `audio_participation_enabled` 启停 PulseAudio/pipewire-pulse 播放流探测
- 音频 probe 故障与无音频分离，已暂停流不参与持续参与判断
- 显式、可克隆、可取消的 Linux MPRIS source，不依赖 Tauri host
- 多 MPRIS 播放器保留有界快照，当前窗口匹配优先于无关活动播放器
- 当前窗口对应播放器的 paused 状态可立即结束 media grace，而不会被其他播放器遮蔽
- 浏览器活动 HTTP transport 使用 Axum，只绑定 loopback，具有 64 KiB body、5 秒 handler、8 请求并发和 5 秒关闭预算
- 浏览器 bridge CORS 只回显 `moz-extension://` 或 `chrome-extension://` Origin，不再使用 `Access-Control-Allow-Origin: *`；无 Origin 的本机诊断请求仍可用
- 浏览器 Token 校验、隐私规则、前台浏览器判断和 SQLite 写入不再依赖 `AppHandle`
- daemon tracking preview 从 profile 设置读取浏览器桥接和 local API 端口/Token；audio、browser bridge 和 local API 配置可由对应 runtime owner 在线应用
- tracking 事件会在离开浏览器、AFK 或暂停时封口网页段；异常退出按 active row 最后可信 `updated_at` 修复，不计入停机空白
- 浏览器 connected 使用 75 秒心跳宽限；desktop 与 daemon watchdog 每 15 秒检查一次，并在扩展过期时按最后成功上报时间封口
- desktop 继续通过薄 Tauri adapter 使用同一桥接核心
- daemon tracking、power、audio、media 与 web activity 的任务状态、重试、取消和退出封口已拆入对应 `app/daemon/runtime/*` owner 模块；聚合 `runtime.rs` 只保留依赖装配和有序关闭

当前实现仍不能发布为正式后台服务，原因包括：

- API listener 端口/Token 已迁移到 daemon owner；systemd service 和可验证 restart ticket 已完成 preview
- desktop 可通过用户会话 D-Bus 查询 `patinad.service` 的安装、启用和运行状态；设置诊断可识别 unit 缺失、systemd 不可用和提前启用造成的 owner 冲突
- desktop Rust host 已有仅连接 `127.0.0.1`、不跟随重定向、带响应大小与请求超时限制的 typed `patinad` client；Bearer Token 不进入 Debug 或前端 JavaScript
- client 会先验证 `runtime_host=daemon`、协议版本范围、tracking ownership 和 event stream capability；可连接的 desktop API、错误 Token 与不兼容协议不会被误判为 daemon ready
- Rust host 已能组合 `/current`、`/sessions/active` 和 `/events` 为只读 runtime snapshot；`/current.sampled_at_ms` 用于过滤早于快照的 replay 事件
- adapter 先建立 SSE 再读取快照，首次从 sequence `0` 使用有界 replay；断线携带最后确认 cursor 重连，`resync-required` 会清除 cursor 并完整重读
- JSON、SSE、认证失败、错误 runtime host、replay gap、窗口切换刷新和 watch shutdown 已使用真实 Axum loopback server 验证
- DEB 尚未自动启用 systemd user service，desktop 也尚未切换为 daemon client
- daemon 尚无浏览器 UI
- Tauri desktop 默认启动尚未切换为 daemon client
- 默认启动仍使用 embedded owner；只有显式传入 `--daemon-client-preview` 才进入 daemon client 模式
- preview 模式不获取 desktop runtime lease，不执行启动存储迁移，不启动 embedded tracker、watchdog、API、browser bridge、Tools、audio/media/power 或 remote status owner
- preview 模式只打开已存在且具有当前 schema 的 SQLite 数据库，保留窗口、tray、desktop behavior 和 updater
- daemon `/current` 已通过 protocol 2 返回完整 `runtime_snapshot`；客户端把它镜像到原有 Tauri tracking state，并转发现有 `active-window-changed` / `tracking-data-changed` 事件
- daemon 连接断开时清除 live snapshot，不把旧窗口继续显示为当前状态；daemon 本身不随 desktop 退出
- Token 不存在或 daemon 暂不可达时 preview desktop 仍可打开，并在 client runtime state 中保留明确错误
- 真实 GNOME 会话验收确认 desktop 与 daemon 可使用同一 production profile 共存，runtime lease 始终属于 daemon，desktop 退出后 daemon API 和 tracking snapshot 继续更新
- daemon client 恢复路径已改为 owner-only 归档暂存、持久预约、systemd 受控重启和启动维护恢复；新实例在后台任务启动前提交恢复事务，Desktop 跨重启按 request ID 查询终态

### 3.1 当前收敛批次

本批次解决 Linux 稳定产品线与 daemon 架构线继续分叉的问题，不改变 `main` 的已发布行为，也不把尚未完成的 daemon 反向合入稳定分支。

实施顺序：

1. 将 Linux `main` 自 patinad 分支点之后的已验证提交单向合入本分支，保留活动详情、应用/分类/网页趋势、安全活动导入与本地定时备份。
2. 对合入能力逐项决定 owner：只读查询可继续复用 transport-neutral read model；导入、备份调度、恢复和任何 SQLite mutation 必须进入 daemon 写侧清单，不能因兼容 embedded 模式而在 daemon client 模式直接打开写连接。
3. 对照上游 `1.9.5` 的行为修复审查暂停、锁屏、休眠、采样失败、网页区间去重和备份恢复边界。已有 daemon 等价保护时补回归测试，不复制 Windows runtime；确有缺口时在 tracking、web activity 或 backup owner 内修复。
4. 完成 backup/restore 与 remote backup 的 owner 收口。受控恢复、remote upload、列表和下载恢复衔接均已实现；远端归档不经过 Desktop 路径，直接复用 owner-only 暂存、重启前预约和 daemon 启动维护模式。
5. 完成首次启动迁移、服务启停、默认 daemon owner 和双 owner 防护，再进入 daemon-backed DEB beta。

本批次验收：

- patinad 分支包含 Linux `main` 的稳定功能，版本与 changelog 状态不倒退
- daemon client 模式不存在新增的直接 SQLite 写路径或第二个后台调度 owner
- 上游正确性修复已逐项记录为“已有等价保护、已移植或明确不适用”
- frontend、真实浏览器 smoke、Rust、Clippy、bundle 与架构边界门禁全部通过
- 合流结果只推送 patinad 分支，不改变当前 Linux Release

### 3.2 本轮正确性审计

| 行为契约 | 当前结论 | 后续动作 |
| --- | --- | --- |
| 启动恢复边界 | 已移植 | active session 只使用持久化的最后成功窗口采样封口；缺失、越界或未来采样不会把停机时间计入使用时长。 |
| 采样失败后的当前会话恢复 | 已移植 | 同一应用刷新 metadata，错误残留的 active app 会按当前成功采样重新切分。 |
| watchdog 与新会话竞态 | 已移植 | watchdog 只封口不晚于其观测边界启动的 active session，不能用旧采样关闭新会话。 |
| browser bridge 端口临时占用 | 已移植 | Axum listener 保留旧配置的原子换端口语义，并使用有界退避自动恢复。 |
| Data 网页趋势重叠区间 | 已移植 | 按浏览器来源和规范域名求区间并集，不重复计算重叠心跳或重复数据，也不填补真实空白。 |
| lock / suspend 与 in-flight probe | 已通过实机验收 | Desktop 与 daemon 共用 lifecycle generation、pending stop 和 transition gate；旧窗口探测不能在 lock/suspend 后恢复 active 状态。`1.9.0-beta.3` 的真实 GNOME 锁屏验收确认约 168.8 秒锁屏区间未计入活动；休眠/恢复验收确认原生 session 和网页活动在同一时刻封口，约 68.1 秒和 79.7 秒休眠窗口均未计入活动，恢复后两条链路继续写入。两次操作期间 daemon PID、restart count 和唯一 lease owner 均未变化。 |
| 暂停与 in-flight probe | 已移植 | `tracking_paused=true` 与 active session 封口在同一 SQLite 事务提交，托盘、Desktop 设置与 daemon API 通过 transition gate 更新 lifecycle generation，旧采样不能在暂停后续写。 |
| 网页活动与原生浏览器 session 绑定 | 已移植 | 网页写入必须匹配当前活动的同名浏览器 session，并持久化 relation；原生 session 结束时 SQLite trigger 在同一事务内截断网页段。备份格式向后兼容保存 relation，Replace/Merge 都使用恢复后的 session ID 重建关系。 |
| restore 的 active timing 边界 | 已移植 | daemon maintenance restore 把 active session、title sample 和网页段封口到备份导出时及各自最后可信观测的上限，不把停机时间补入活动。 |

### 3.3 合入功能的写侧边界

- 活动详情、应用/分类/网页趋势和导入数据聚合属于只读能力，可以继续复用 transport-neutral read model。
- 历史清理由 Rust data owner 统一覆盖原生 session、导入事实和网页活动；标题清理覆盖原生与导入标题；按应用删除只按 executable 清理原生和导入事实，网页历史继续使用独立的域名删除语义。daemon API 已覆盖这三类维护操作。
- 活动导入提交、批次列表和批次删除已由 daemon owner 接管：Desktop 只把预览后未变化的 CSV 写入 profile 控制目录中的 `0700` 暂存目录和 `0600` 随机票据文件，API 只传票据、文件名与预览指纹；daemon 一次性消费文件并重新检查 128 MiB 上限、SHA-256 和 CSV 内容。API 不接受任意本机路径或大文件正文，该入口不作为 MCP 通用文件读取工具。
- 定时备份已由 daemon owner 接管：调度循环只随 tracking owner 启动，配置和运行状态通过 `/api/v1/backups/schedule` 读写，变更通过 SSE 通知 Desktop 重读；embedded owner 仅保留为兼容路径，同一 profile 不得同时运行两套调度器。
- 按应用删除已由 daemon data owner 接管：请求必须显式确认并限定 1 至 512 个 executable，可选时间范围必须同时提供完整半开区间；原生和导入事实、批次计数在同一事务更新，Desktop 只接收删除计数和刷新事件。
- 备份恢复已由 daemon owner 接管：Desktop 只预览和创建 owner-only 随机暂存票据，daemon 验证后预约 systemd restart，新实例在启动后台任务前执行单事务恢复；daemon client 模式不回退为 Desktop 直接写库。
- remote backup 的 URL、用户名、远端目录和最近完成时间已通过 daemon app-settings owner 写入；密码按 profile 存入系统凭据服务，不进入 SQLite、HTTP 响应、日志、OpenAPI 示例或 MCP 输出。
- 显式上传、远端列表与恢复下载已由 daemon 串行执行。Desktop daemon-client 模式只发送非密钥配置、索引 ID、策略与显式确认；daemon 校验 index 派生路径，有界下载实际归档，并直接复用受控启动恢复状态机，不返回本机路径。

### 3.4 Stage 2H.3c.6 受控恢复完成状态

受控恢复已按四个可独立验证的小批次完成，避免一次同时修改备份格式、文件边界、daemon 启动顺序和 Desktop 交互：

1. **备份关系完整性**：在现有 `web_activity_segments` 备份条目中加入向后兼容的 `native_session_id`，Replace/Merge 使用恢复后的 session ID 映射重建 `web_activity_native_sessions`，并覆盖旧备份无该字段的兼容测试。
2. **owner-only 暂存与预约**：复用活动导入的随机 ticket 思路，但使用独立恢复目录和持久 reservation。Desktop 只暂存已预览且指纹一致的归档；daemon API 只接受 ticket、SHA-256、大小、Replace/Merge、`confirmed: true`，并在请求 systemd restart 前再次验证归档。
3. **启动维护恢复**：新 daemon 在 SQLite migrations 之后、API credential/settings 加载和所有后台 task 之前执行 reservation。恢复事务先把归档中的 active timing 封口到备份产生时的可信上限，再恢复 sessions、title samples、网页关系、普通 settings、Tools 和导入数据；local API、browser bridge、remote status 与 WebDAV 目标等主机集成设置保留当前值且不从归档补入。成功/失败都持久化可查询终态，失败不覆盖为成功也不循环重启。
4. **Desktop typed client 与重连状态**：恢复命令在 daemon client 模式下创建预约并进入重连等待，按 restore request ID 查询 completed/failed；embedded 模式暂时保留现有兼容实现。文件选择和预览仍留在 Desktop，不把任意路径、归档正文或 restore 能力暴露给 browser UI、MCP、CLI/Agent。

安全与删除约束：暂存根目录必须是当前 profile control root 下的真实 `0700` 目录，文件必须是新建 `0600` 普通文件；拒绝 symlink、hard-link 替换、超限文件、内容/指纹变化和跨 profile ticket。成功后只删除 reservation 精确绑定的暂存文件；失败文件保持 owner-only 供显式重试或取消，不做模糊路径清理。任何阶段失败都必须保持原 SQLite 数据可继续启动。

恢复事务同时写入以 request ID 和 archive SHA-256 约束的 durable receipt。若进程在数据库提交后、reservation 标记完成前退出，新实例只补记 completed 和清理精确暂存文件，不重复执行 Replace/Merge。失败 reservation 不自动重试；显式取消仅允许 failed 状态，并保留失败原因供诊断。

### 3.5 当前批次：remote backup owner

1. **已完成 owner 审计**：已枚举 WebDAV secret、测试连接、上传、下载、列表和 remote-status 路径；remote-status 是独立兼容能力，不与 WebDAV backup 混为同一 owner。
2. **已完成 fail-closed 与设置收口**：daemon-client 模式下 WebDAV 非密钥设置不再直接写 SQLite；普通 app-settings 白名单只接受 URL、用户名、远端目录和完成时间，拒绝密码键。
3. **已完成凭据与上传 owner**：Linux 密码按 Production/Local/Dev profile 存入 Secret Service；Windows 冻结兼容路径保留 Credential Manager。`POST /api/v1/backups/remote/upload` 只接受非密钥配置和 `confirmed: true`，daemon 从自身 pool 生成 snapshot、使用 `0700` 临时目录与 `0600` 文件、复核归档并串行上传，最后删除精确临时文件。
4. **已实现列表与有界下载**：远端 index 由 daemon 以 1 MiB 上限读取，逐项验证产品、版本、ID、重复项、大小和 ID 派生路径；归档下载使用 `create_new`、`0600` 和 512 MiB 上限，超限或失败只删除本次临时文件。
5. **已完成并验证恢复衔接**：用户先根据索引元数据确认；daemon 下载并复核实际归档及其索引元数据后写入 2H.3c.6 的 owner-only staging，直接预约同一 systemd 启动恢复。调度失败时先检查票据是否已被 reservation 持有，无法证明安全时保留文件。前端/集成门禁、516 项 Rust 测试和 Clippy 已通过；跨 systemd 重启的真实远端服务验收并入 Stage 2H.3d 的 DEB 实机清单。

### 3.6 下一阶段：Stage 2H.3d 默认 owner 切换

Stage 2H.3d 不做一次性切换，按下面五个可回滚批次推进：

1. **2H.3d.1 systemd 控制基础（已实现，固定 unit 安装、启动与崩溃恢复已通过实机验收）**：`platform/linux` 已补齐固定 `patinad.service` 的 enable/disable/start/stop、8 秒超时、幂等短路与操作后复核；`app` 层在 Production embedded 启动前会停止提前运行的 packaged daemon，Dev/Local 不受影响。当前不开放通用 unit 名称、shell 命令、HTTP、MCP 或 UI 开关；`1.9.0-beta.1` 已确认 packaged unit 能由首次交接启用并启动，且一次受控 `SIGKILL` 后由 systemd 以新 PID 自动恢复。设置页启停与登录偏好 mutation 仍归入 2H.3d.5 的剩余实机验收。
2. **2H.3d.2 登录偏好拆分（数据语义已实现）**：已新增 host-owned `background_tracking_at_login`，并保留 `launch_at_login` 作为“桌面客户端随登录打开”；旧数据库首次打开时，新键只在缺失时继承旧值，此后不再被旧键覆盖，新安装保持现有默认行为。`start_minimized` 仍只依赖桌面客户端偏好；备份 Replace/Merge 保留当前机器的后台登录偏好。普通 UI patch、HTTP 和 MCP 仍不能直接写入后台服务偏好；首次 owner 交接已按该值应用固定 unit，日常设置与失败对账留给 2H.3d.4 专用入口。
3. **2H.3d.3 两阶段 owner 交接（代码、中断自动化与 DEB 正常路径实机验收已完成）**：第一进程只写入 owner-only cutover reservation、启用 unit 并安排受控重启，不在 embedded tracker 存活时启动 daemon；新 Desktop 进程读 reservation 后进入 daemon-client 模式，启动并协商 daemon，成功后才提交完成状态。`1.9.0-beta.1` 已确认 reservation 提交为 `completed`、Production lease 始终属于 daemon，Desktop 关闭和重开均未抢占追踪或 API。daemon 不可用或版本不兼容时显示暂停与修复诊断，不自动回退 embedded；这些失败路径仍需设置页实机重试/回滚验收。
   - **2H.3d.3a reservation 基础（已实现）**：`app/runtime_owner_cutover` 已提供 `prepared → activating → completed/failed` 持久状态机、请求 ID 约束、profile 校验、32 KiB 读取上限、owner-only `0600` 原子文件和幂等转换。文件缺失时允许 embedded；除后续显式完成的 `rolled-back` 外，任何 reservation 都选择 daemon-client 或 fail-closed 方向，failed 状态不隐式重试，损坏、不可信或 profile 错配文件直接 fail closed。本批不启用或启动 unit。
   - **2H.3d.3b embedded 准备与重启（已实现，DEB 正常路径已通过实机验收）**：仅 Production 且 user manager、固定 unit 和状态检查可用时触发；先持久化 reservation，再按后台登录偏好 enable/disable 固定 unit 并校准独立 Desktop autostart，随后请求 Tauri 受控重启且不启动 embedded runtime。Dev/Local、unit 缺失和 systemd 不可用时继续旧 embedded 路径；写入 reservation 后的失败会持久化为 failed。
   - **2H.3d.3c daemon-client 激活与确认（已实现，DEB 正常路径已通过实机验收）**：新进程由 reservation 自动选择 managed client，先把状态推进到 activating，并等待旧 Desktop `RuntimeLease` 释放后才启动固定 unit；client 在 15 秒内轮询 capability，只有 runtime host、协议、tracking owner 与 `tracking.ready` 全部成立才标记 completed。实机确认 Desktop 与 daemon 同时存在时端口、lease 和 capability 仍由 daemon 持有，Desktop 退出后记录继续增长。永久协商错误立即失败，暂时不可达可重试；failed、损坏和不可信 reservation 均不回退 embedded。显式 preview 不参与该状态机。
   - **2H.3d.3d 中断恢复自动化（已完成，service 崩溃恢复已通过实机验收）**：状态机测试覆盖每个持久化边界的重启 owner 决策、重复启动、错误 request ID、service failed、API 未就绪、版本不兼容和旧 Desktop 尚未释放 lease；`1.9.0-beta.1` 实机向固定 service 注入一次 `SIGKILL` 后，systemd restart count 增加、PID 更新、daemon lease/API/tracking 恢复且 SQLite `quick_check` 保持 `ok`。
4. **2H.3d.4 设置与回滚入口（正常路径实机通过，故障矩阵待补）**：在 Quiet Pro Settings 中提供后台服务状态、启停和显式回滚。停用 daemon 前必须先封口并停止服务，确认 RuntimeLease 已释放后才能预约下一次 embedded 启动；不允许两个 owner 同时运行，也不把服务管理暴露给浏览器 UI、MCP 或 Agent。
   - **2H.3d.4a 交接诊断（已实现）**：Tauri 专属诊断同时返回固定 unit 与 owner cutover 状态，区分未请求、准备、激活、完成、失败及 reservation 损坏；Settings 对接管中、接管失败、managed 正常和 managed 服务停止使用不同状态与提示，并展示有界失败原因，不暴露 Token。
   - **2H.3d.4b 显式重试与重新接管（重新接管实机通过，failed/blocked 实机待补）**：仅允许本机 Tauri command 在确认后重试 failed/blocked 交接，或从可信的 `rolled-back` embedded 状态重新接管。failed/blocked 重试会在任何 reservation 变更前停止可能残留的 daemon、等待 lease 释放，再以当前偏好和新 request ID 原子重建 owner-only reservation。rolled-back 重新接管不会在 embedded lease 存活时启动 daemon，只创建新的 `prepared` 预约并受控重启，下一 Desktop 进程等待旧 lease 释放后再启动服务。损坏或 symlink reservation 不会被重新接管入口替换；能力未开放给 HTTP、MCP、browser UI 或普通 app-settings patch。
   - **2H.3d.4c 登录偏好应用（已实现并通过 DEB 实机验收）**：后台追踪开关只修改 `background_tracking_at_login` 并对账固定 unit 的 enable/disable，不把“当前运行”与“下次登录启动”混成同一语义；Desktop 登录和启动最小化继续走独立 XDG autostart 偏好。专用 Tauri command 以 completed reservation 记录持久意图，再应用 unit 并同步 SQLite 镜像；managed Desktop 启动时按 reservation 重新对账 unit 和 host-owned 数据，因此任一步中断都能在后续启动继续收敛。systemd 状态与意图不一致时诊断显示 `preference-mismatch`，普通 settings patch 不能绕过专用入口。`1.9.0-beta.4` 已确认关闭和重新启用时 reservation、SQLite 镜像与 unit enable 状态双向一致，当前 daemon PID、restart count 和 active 状态不变。
   - **2H.3d.4d 显式回滚（beta.6 正常路径实机通过）**：本机确认式 Tauri command 先持久化 `rolling-back`，再让 systemd 停止 daemon，使 tracking/web session 通过正常 shutdown 封口；确认 lease 释放后禁用 unit、对账 Desktop autostart、保存后台登录偏好，最后提交 `rolled-back` 并受控重启。`rolling-back` 中断仍保持 client/fail-closed，可重复恢复；只有 `rolled-back` 才允许 embedded，且 embedded 启动会再次停用意外残留的 unit。损坏 reservation 可被原子替换，不跟随或修改 symlink 目标。
   - **2H.3d.4e Quiet Pro 控件（正常操作实机通过，故障态待补）**：Settings 的后台服务诊断区按后端能力和 reservation 状态显示登录启动、重试/重新接管和回滚控件；重试与回滚必须经过确认，单一 action 状态会在操作期间禁用重复提交。`prepared/activating` 等进行中状态不开放变更，`rolling-back` 只允许幂等继续回滚，`rolled-back` 只允许重新接管；服务管理仍不开放给 HTTP、MCP、browser UI 或普通设置 patch。
5. **2H.3d.5 DEB 成品与实机验收（进行中）**：覆盖首次迁移中断、重复执行、unit 缺失、systemd 不可用、服务崩溃、Token/端口不一致、旧 XDG autostart、pending storage migration 和自定义挂载目录。最后在已安装 DEB 上验证登录启动、关闭 UI 后持续记录、重开 UI、锁屏/睡眠、浏览器活动、升级、卸载与数据保留。
   - **2H.3d.5a 成品静态验证（已实现）**：发布工作流在上传前解包最终 `.deb`，核对 `patina` 包名、版本、`amd64` 架构、Patina Desktop 与 `patinad` 可执行文件、固定 user unit、安全选项、GNOME 扩展 UUID，并拒绝通过维护脚本提前 enable/start `patinad.service`。该检查不安装软件，也不替代真实用户会话验收。
   - **2H.3d.5b DEB-only beta 发布契约（已实现）**：带预发布后缀的 daemon-backed 版本只构建和上传 `.deb`、对应签名、DEB updater 元数据及扩展资产；稳定 tag 仍保留 AppImage、DEB 和通用 AppImage fallback。发布说明、bundle target、资产复制、GitHub Release 附件和 `latest.json` 平台项由同一版本策略决定，并有自动化防止 beta 混入 AppImage。预发布 manifest 只挂在对应 prerelease，不替换稳定 `/releases/latest/`；专用 beta 自动更新通道不属于首次实机验收前置条件。
   - **2H.3d.5c 已安装包实机验收（进行中）**：只读验收采集器、旧版升级前基线、数据目录外的可恢复备份、`1.9.0-beta.1` DEB 安装、静态成品校验和完整 release gate 已完成。首次 owner 交接已达到 `completed`；关闭 Desktop 后 daemon 继续记录，重开 Desktop 未形成第二 owner；固定 service 崩溃后由 systemd 自动恢复；Firefox/Zen 扩展也已在 daemon 重启后重新连接浏览器桥接。`beta.1 → beta.2 → beta.3` 连续覆盖安装及受控 daemon 重启已确认 restart ticket、实例切换、数据库完整性、计数不倒退和扩展重连。`beta.2` 暴露的 completed reservation Desktop 重开错误等待健康 daemon lease 已在 `beta.3` 修复：实机启动跨过原 5 秒故障窗口，未再输出 lease timeout，daemon PID 与 lease 不变。`beta.3` 的 GNOME 锁屏/解锁已确认边界封口，恢复后原生与网页追踪继续写入，service 与 daemon lease 保持稳定；用户报告的休眠/恢复已有追踪空档证据，但尚缺系统 suspend/resume 事件确认，不作为完整休眠验收通过。音频参与在 `beta.3` 已确认 PulseAudio session 可匹配 Zen；同时发现 Zen MPRIS 使用 `firefox.instance_*` 而前台可执行文件未归一的问题，并在 `beta.4` 修复。`beta.3 → beta.4` 覆盖安装、受控重启和实机复测确认两路均为 `matched`，最终由 `system-media` 驱动参与状态。`beta.4` 也已确认后台登录偏好关闭与重开时 reservation、SQLite 和 systemd unit 双向对账，当前 daemon 不被误停。`beta.6` 已通过设置页回滚自动重启与再次接管验收；`beta.7` 已通过备份导出/读取、remove 卸载数据保留、重装不自动启服和首次打开恢复追踪验收；最新重装后的 Zen 扩展重连及切走封口也已通过；`beta.7` 已补齐真实系统 suspend/resume 日志及数据库边界证据，挂起期间无计时、恢复后新会话正常写入。以上为已执行场景，不代替最终发布门槛复核。

2H.3d.5c 使用同一 working 文档收口，不再新建一次性顶层文档。仓库提供 `npm run release:inspect-installed-patinad -- ...` 作为只读证据采集器；它只检查固定包路径、systemd 状态、owner 文件、SQLite `quick_check` 和裁剪后的 capability，不输出 API Token、窗口标题或 URL，不安装软件、不控制服务、不覆盖已有证据文件。输出文件使用 `create_new` 和 `0600`。

#### 当前验收结论（2026-09-09，beta.9 候选）

**最终发布结果：** `v1.9.0-beta.12` 已于 2026-09-09 从 `e2705d6` 发布为 DEB prerelease，Actions `34357826059` 成功。公开资产包含 DEB、GNOME/Chromium/Firefox 扩展和仅有 `linux-x86_64-deb` 的更新清单；GitHub Latest 仍为 `v1.8.4`。下载后的 DEB 通过成品检查及仓库配置公钥的 Minisign 验签，大小 24835988 bytes，SHA-256 `21515feaa34ec720ba516da424f6218a734497b885fd8398e679dba14938b10c`。下载验证文件在 `/tmp/patina-beta12-release-verify-yQNe2X`，没有安装；正式 daemon PID 583229、NRestarts=1 保持不变。以下失败尝试是历史过程，当前发布状态以本段为准。

最新候选为 beta.12：beta.11 的元数据检查通过，但新增本地 gate 调用无参数版本校验时缺少默认值，Actions `34357279422` 提前失败，未打包。现已让该 CLI 默认读取 package.json，并增加真实子进程测试覆盖无参数、显式版本和错误版本。提交与标签继续不可变，不以失败尝试称为发布成功。

后续发布记录：beta.10 的本地 `TZ=UTC npm run release:check` 通过（569 Rust passed / 6 ignored，31 browser smoke），但提交遗漏 Cargo.lock，Actions `34356819593` 在版本校验拒绝，仍未生成安装包。beta.11 补齐锁文件并将版本校验前置到本地 release gate；两次失败 tag 均保留，发布以新标签推进。本机 beta.9 和生产后台没有被改动。

发布状态更新：用户授权发布后，提交 `4764687` 和 `v1.9.0-beta.9` 已推送；Actions `34355224634` 在 UTC 环境的午夜汇总测试失败，未进入签名和打包，也未形成 GitHub Release。已本地复现 `/summary/today` 在区间起止相同的零点返回 500，修复仅让内建日/周汇总返回零贡献，显式空范围仍为 400。保留 beta.9 标签，后续候选为 beta.10；以下安装事实仍对应本机 beta.9，不因源码版本变化自动升级。

| 场景 | 状态与边界 |
| --- | --- |
| 单 owner、关闭/重开 Desktop、服务崩溃恢复 | 已通过，见 beta.1 至 beta.3 证据 |
| Desktop 健康状态刷新、版本对齐 | beta.9 只读 managed 验收通过；用户确认不再出现间歇“追踪运行时未就绪”。不扩大为所有异常场景均已验证 |
| 回滚自动重启、再次接管 | beta.6 已通过 |
| 备份选择/取消、导出、产品内解析 | beta.7 已通过；未执行恢复写入 |
| remove 卸载、数据/Token/备份保留、重装、追踪恢复 | beta.7 已通过；不覆盖 purge |
| Zen 重连、切走封口、真实 suspend/resume | beta.7 已通过；挂起前无活动网页，不代表活动网页跨挂起已验证 |
| 登录偏好与独立后台启动 | 配置/unit 对账及 Linger=yes 下重启登录自启已通过，Desktop 未启动也能记录；不覆盖 Linger=no 或仅注销再登录 |
| 故障与维护路径 | failed/blocked 重试、Token/端口不一致、缺失 unit、自定义挂载目录/pending migration 等已有自动化或实现，未全部做真实安装故障注入 |
| 受控备份恢复、远端恢复 | 合成数据的磁盘恢复/重开、receipt 幂等、失败保留和清理回滚已通过；真实临时 systemd 服务的跨进程 Replace/Merge 及失败回滚通过；私有 Secret Service 凭据到 HTTP 下载、预约及 systemd 重启恢复链路已通过。不覆盖第三方 WebDAV/TLS、上传及任意崩溃时刻 |
| 桌面 UI 内存 | 第一批重预热/堆回收修复及隔离 Wayland 验收已完成，合成数据主进程启动 USS 244.5→101.9 MiB、备份后关闭至 79.7 MiB。用户安装 beta.13 后报告 257M→219M，口径和长期表现待核对。beta.14 又修复 Widget 隐藏计时、创建取消清理及未显示窗口鼠标穿透崩溃；626.64 秒原生回归、release gate 和本地 DEB 检查通过，候选尚未安装或发布。最后 WebView 回收、启动路径、诊断口径、有界聚合和流式备份仍待完成 |
| 发布 | beta.12 DEB prerelease 已发布，下载成品与公钥验签通过；本机仍安装 beta.9，未自动升级。稳定版前仍需 AppImage 兼容或退役迁移方案 |

下一步顺序：明确限制的 DEB beta 已发布；先验证并处理桌面内存回收，再继续活动网页跨挂起和剩余安装故障矩阵。独立凭据远端恢复已补齐下述隔离链路，后台自启已通过当前 Linger=yes 的重启登录场景，不为扩展矩阵擅自修改用户登录配置。不得因为 Beta 发布就将 Stage 2H.3d 或稳定版整体标为完成。Flatpak 属于后续安装格式评估，不替代 AppImage 更新承诺或桌面 provider 适配；恢复策略 UX 不阻塞已通过的只读归档校验。

2026-09-09 健康状态修复：Desktop 原本只在 SSE 数据变化或 resync 时刷新快照，与前端 8 秒心跳过期判断不匹配。隔离 HTTP 回归测试已在修复前复现后台采样推进、客户端仍持有旧时间的问题。runtime adapter 增加 2 秒周期重读，使用后台采样时间，不以客户端请求成功时间续命；读取失败仍清除实时快照并进入重连。测试覆盖无事件刷新、冻结采样时间不被改写、追踪不可用时清除旧数据，同时保留既有 SSE、配置切换与退出测试。随后已打入 beta.8 并由用户安装，真实故障发生时刻的关联与新包复测仍待完成。

周期刷新还必须保留晚到的 SSE 数据失效通知：即使事件时间早于最新快照，也仍通知客户端刷新统计，不因快照较新而丢弃事件。隔离回归已覆盖该顺序。健康修复的 `npm run check:full` 已通过；补充晚到事件保护后再次执行 Rust 全量校验（565 项通过、3 项忽略）。前端包含 30 项真实浏览器测试；测试退出时出现临时 Chromium profile 清理 `ENOTEMPTY` 警告，测试与构建结果通过，未改动正式用户目录。安装包复测、真实 systemd 跨进程恢复及 WebDAV 恢复不包含在这些自动化结论中。

验收脚本已加强：managed 同时检查运行中的 serverVersion 与期望版本、service PID 与 lease PID 一致；无法查询 systemd 不再视为停服；uninstalled 必须保留 owner-only Token。PID 与 capability 校验仍不能替代动作前后对比或证明所有潜在进程不存在。旧证据文件不改写，后续用新文件重新采集。

本轮收口验证：`test:release` 通过（24 项发布策略、3 项 DEB 静态验证、11 项已安装验收测试），版本/Changelog 校验、架构检查和 `git diff --check` 通过。加强后的 `/tmp/patina-beta7-closeout-strict.json` managed 复查通过。此前 beta.7 的完整 release:check 与成品构建证据仍适用于未再改动的运行时代码；本轮只新增验收脚本检查和文档，不重打同版本安装包。

#### beta.8 候选与构建存储（2026-09-09）

- 健康状态修复已形成 `1.9.0-beta.8` 本地未签名 DEB，用户已安装，未推 tag 或发布。`npm run release:check`、版本一致性检查与成品 `release:verify-daemon-deb` 全部通过；Rust 565 项通过、3 项忽略，Clippy 通过。候选包含匹配的 Desktop、daemon、unit 与 GNOME 扩展。
- 成品：`src-tauri/target/release/bundle/deb/Patina_1.9.0-beta.8_amd64.deb`，约 25 MiB；SHA-256：`89b59b129966676ffb67b89de563c278c3051b482f72f1e4bb7876a531ee79dd`。
- 安装前证据 `/tmp/patina-before-beta8-storage-check.json` 的 quick_check 未在采集时限内取得结果，不标为通过；随后单独只读检查返回 `ok`，完整重采集 `/tmp/patina-before-beta8-recheck.json` 全部通过，运行版本仍为 beta.7、daemon PID 1473，未控制正式服务。
- 初始构建存储调查：target 约 61 GiB，其中 debug 约 56 GiB、增量缓存约 36 GiB、debug/deps 约 18 GiB；release 约 4.4 GiB。node_modules 约 234 MiB，上游源码副本约 35 MiB。主要占用是可重建 Rust 调试产物，不是 Windows 源码或正式数据库。
- dev/test 改为有限调试信息并关闭增量编译，Release 参数不变；清理边界、复原完整调试信息与编译时间代价见 `docs/linux-development-setup.md`。首次新配置验证后新旧缓存并存，target 暂约 63 GiB。用户随后授权清理：确认无 Cargo/rustc 构建进程且目录为非符号链接的规范路径后，仅删除 `src-tauri/target/debug/incremental`；target 降至约 28 GiB，文件系统可用空间从约 81 GiB 增至 116 GiB。保留 debug 依赖、所有 Release 产物、应用数据与密钥，beta.7/beta.8 DEB 清理前后 SHA-256 一致。
- 安装后证据 `/tmp/patina-beta8-after-install-cache-cleanup-baseline.json`：软件包为 beta.8，数据库 quick_check 为 `ok`，service/lease PID 1473 一致，但运行中 daemon 仍报告 beta.7，版本匹配检查未通过。采集时无 Desktop 进程，尚不能标为升级完成；未擅自重启正式服务。
- 用户打开 Desktop 后重采集 `/tmp/patina-beta8-after-open.json`，后台仍为 beta.7、PID 1473，数据库完整，版本匹配检查仍未通过。代码检查确认客户端按协议兼容协商，并没有仅因服务版本变化而自动重启的实现；此前“打开新版 Desktop 会自动完成后台升级”的描述不成立。版本差异提示与确认式受控升级需要纳入后续收口，不能擅自重启正式服务或宣称安装验收完成。健康刷新修复位于 Desktop，是否仍跳动待用户反馈。
- 下一步完成正式后台的显式受控升级与健康状态复测，同时继续独立的 WebDAV 恢复验证。旧 DEB 保留供回退，生产数据库和签名私钥未改动。

#### 后台版本诊断与确认式重新加载（2026-09-09，未打包）

- 在现有设置诊断中显示 Desktop 与运行中 daemon 的版本。版本差异为警告，不等同于协议不兼容；仅对已完成接管、可控且提供 service-lifecycle scope 的 Production managed client 显示重新加载入口。
- 编排落在 `app/daemon_service/upgrade.rs`，复用 typed client 的 `/api/v1/system/service` 与 `/api/v1/system/service/restart`，不增加 HTTP endpoint、不直接执行 systemctl restart。命令再次检查确认、owner 状态和用户确认时的运行版本，并使用既有 mutation gate。
- POST 只发一次，45 秒内有界等待同一 ticket completed、新 instance、Desktop/daemon 版本一致及 tracking ready；拒绝、旧确认、错误票据、仍为旧版本或超时不报成功，也不自动重试。仅重新加载已安装程序，不下载更新，不改变登录偏好。
- 隔离 HTTP 测试覆盖确认过期不写入、成功、拒绝、错误版本、错误票据和单次 POST；真实 Chromium 的 mock-Tauri 测试覆盖提示颜色、取消、确认、防重复、完成前不报成功以及 1280/620 宽度布局。截图位于 `/tmp/patina-daemon-reload-ui-review/`，未连接生产后台。
- 最终 `npm run check:full` 通过：Rust 567 passed / 4 ignored，31 项浏览器 smoke、前端回归、构建、bundle budget 和 Clippy 通过；Changelog 与 diff 空白检查通过。首次全量检查暴露旧源码扫描测试包含自身的问题，已恢复测试模块顺序而未放宽断言；Clippy 的布尔表达式建议已修正。浏览器临时 profile 清理仍有非致命 `ENOTEMPTY` 警告。只读确认正式 service 仍为 PID 1473、active/running、NRestarts=0。
- 本轮尚未更新版本、重打 DEB、提交或推送。已安装 beta.8 不包含新控件；生产后台没有因本轮开发而重启。下一候选需另行打包并在用户确认后实机核验，健康跳动反馈、WebDAV 恢复及剩余安装矩阵仍未完成。

#### beta.9 本地安装候选（2026-09-09）

- 版本文件、Cargo.lock 和发布规范同步为 `1.9.0-beta.9`；重新执行完整 `npm run release:check` 通过，Rust 567 passed / 4 ignored、31 项浏览器 smoke、Clippy、扩展校验及版本一致性通过。
- 仅构建 DEB，命令行临时设置 `bundle.createUpdaterArtifacts=false`，未修改正式 updater 配置，也未读取签名私钥。成品验证确认 Desktop、daemon、固定 user unit 和 GNOME 扩展齐全，包不自动启用或启动服务。
- 成品：`src-tauri/target/release/bundle/deb/Patina_1.9.0-beta.9_amd64.deb`，约 25 MiB，SHA-256 `704aa6033bff3a42375106316732b7d21bff859b8a92111cf2c4ddd75af4ea5b`。包内 daemon 与本次 Release 构建逐字节一致；Desktop 仅存在 Tauri 的 3 字节 `UNK` 到 `DEB` 包类型标记差异。旧 beta.8 包保留，SHA-256 未变。
- 安装前 `/tmp/patina-before-beta9-baseline.json` 通过：包 beta.8、后台 beta.7，service/lease PID 1473 一致，quick_check=ok，Token 普通文件且 0600；未安装、未控制正式服务。
- 人工验收顺序：正常退出 Desktop（不是只关闭窗口），覆盖安装 beta.9 后重新打开；核对设置诊断 Desktop 为 beta.9、Daemon 为现有旧版，再显式确认“重新加载后台”。等待成功后检查两者为 beta.9、tracking ready、service/lease 为同一个新 PID，并观察健康状态是否仍跳动。不要以新包已安装替代后台已切换，不使用强制退出代替受控重启；失败或超时先核对状态，不连续重试。
- 构建交付时仅完成本地构建与静态验收，未提交、推送、打 tag 或发布；后续实机结果如下，不以构建通过代替运行版本核验。

#### beta.9 版本切换与远端下载回归（2026-09-09）

- 用户确认 Desktop 和 Daemon 均显示 beta.9。只读 `/tmp/patina-beta9-after-reload.json` managed 检查全部通过：包和 API 版本 beta.9，service/lease 新 PID 583229 一致，active/running、NRestarts=1、tracking/browser/Tools ready，quick_check=ok，Token 普通文件且 0600。相对安装前 PID 1473 已完成实例交接；本轮核验没有控制生产服务。长期健康状态是否仍跳动仍待用户反馈。
- 新增 `data/remote_backup/transfer_tests.rs`，由临时 loopback HTTP 服务返回合成导出的真实 ZIP 和索引。只提取同一 data owner 内的私有客户端下载/暂存函数，生产入口的 profile-scoped 系统凭据读取不变；测试不使用用户的 WebDAV 配置、密码、备份或数据库。
- 一项集成测试覆盖 8 个场景：成功下载、非法索引路径、重复 ID、缺失索引、401、损坏 ZIP、索引元数据不符、归档 404。校验失败不产生暂存票据，成功票据通过 SHA-256/大小/0600 校验；临时下载被清除，无关文件及合成数据库保持原样，恢复 receipt 为零。
- 目标测试和完整 `npm run check:rust` 通过：568 passed / 4 ignored，Rust 边界检查与 Clippy 通过；本轮未改 UI，未重复运行前端全量检查，沿用 beta.9 构建前的前端验收证据。
- 这仅覆盖 HTTP 下载、归档校验和暂存，不覆盖上传、真实 WebDAV 服务兼容、Secret Service、远端恢复预约与 systemd 重启完整链路。下一步在独立凭据会话中串联这些边界，不能仅将既有本地恢复测试与本轮下载测试相加就宣称端到端验收完成。
- 不重打或覆盖已有 beta.9 DEB；本轮源码回归改动未安装、未提交或推送。

#### 发布评审（2026-09-09）

- 用户确认 beta.9 健康状态不再跳动，正常路径问题收口。远端 Release 只读核对显示最近公开版本为 1.8.4，beta.1 至 beta.9 仍是本地候选；不能把本机安装或已有提交当作远端发布成功。
- 当前适合评估带明确限制的 DEB prerelease，不适合发布 1.9.0 stable。稳定版前仍需独立凭据与 systemd 的完整远端恢复、活动网页跨挂起、剩余安装故障矩阵，以及 AppImage 兼容或明确退役方案。恢复预览 UX、TUI、本机浏览器 UI 和大规模平台删除不阻塞本轮 beta，也不在发布前顺带扩张。
- 发布前修正中英文 README 的旧默认 owner 描述，并说明 stable/main 与 daemon 分支、DEB-only、备份前置和显式版本重新加载。新 WebDAV 回归只提取私有客户端函数并补测试，没有引入新的凭据入口或 HTTP 协议；它在源码中但不在此前用户安装的 beta.9 二进制中。
- 本轮最终 `release:check` 通过：568 项 Rust 测试通过、4 项按约定忽略，31 项真实浏览器 smoke、构建、bundle budget、Clippy 和扩展验证通过。Chromium 临时 profile 清理仍有非致命 `ENOTEMPTY` 警告，不影响通过结论，也不作为测试基础设施已完全收口的证明。
- 已提交 `6b13973` 并推送至 `origin/feature/patinad-daemon`，连同此前积累的 51 个提交一并同步；未合并或推送 `main`。公开 beta/tag 动作等待用户确认，不能将本次分支推送称为已发布。

#### 私有凭据远端恢复与内存基线补充（2026-09-09）

- 使用已安装 beta.9 二进制，在每个独立临时 HOME/XDG 下启动私有 D-Bus/Keyring；只在该私有总线保存合成密码。真实 daemon 从受控 Basic-auth HTTP fixture 读取索引、下载 exporter 生成的 ZIP，经 HTTP 202 预约、systemd 新实例执行，再检查终态、数据库、receipt 和所属 staging。未修改生产服务、凭据或数据。
- Replace、Merge、INSERT 失败回滚三场景通过，PID 分别为 `785280 -> 785663`、`785710 -> 785734`、`785854 -> 785880`。证据目录分别为 `/tmp/patina-systemd_test_3107bede40edb5bc00cf94c9896e388a`、`/tmp/patina-systemd_test_322248a9223e35dbfe8fa3e3ef3fdb13`、`/tmp/patina-systemd_test_f590984f68b4b573b6e022a1dcc5eff7`。每次归档只下载一次，成功清理所属暂存，失败保留暂存与原有数据；源归档和无关文件保持不变。
- 这是私有真实 Secret Service + 受控 HTTP 下载 + 当前用户 manager 的真实跨进程恢复，不代表第三方 WebDAV 服务、TLS、上传、真实账号或任意断电点均已验收。默认测试忽略两项 opt-in 恢复入口及私有 worker；只按开发文档逐项显式运行。
- `npm run check:full` 通过：568 Rust passed / 6 ignored，31 browser smoke、前端回归、构建、bundle budget、边界与 Clippy 通过。浏览器临时 profile 清理的非致命 ENOTEMPTY 警告仍在，未将其隐藏。
- 重用测试框架的本地恢复也在 beta.9 复跑三场景通过：PID `787135 -> 787164`、`787194 -> 787221`、`787287 -> 787348`，未因新增远端路径退化。
- 内存证据 `/tmp/patina-beta9-memory-observed.json`：Desktop 主进程 PSS 266868 KiB，WebKit Network 14563 KiB、WebProcess 351464 KiB，桌面合计 632895 KiB（约 618 MiB）；daemon 18062 KiB（约 18 MiB）。这是未确认窗口状态的单次观测，不是泄漏证明或前后优化对照。工具不读取 Token、数据库、活动内容或进程环境。
- 当时尚未执行人工关闭/重开对照；后续证据见下一节。Flatpak 两种候选架构与权限边界已回写平台文档。

#### 桌面内存回收调查（2026-09-09）

正式安装仍为 beta.9。以下是同一 Desktop PID 582450 的只读采样，时间为 UTC，单位 KiB；PSS 包含按比例分摊的共享页，USS 为私有驻留页。

| 状态 / 时间 | Desktop PSS | Desktop USS | Desktop + WebKit PSS | WebProcess |
| --- | ---: | ---: | ---: | --- |
| 第一次关闭并等待 / 14:46:24 | 300804 | 268544 | 316434 | 已退出 |
| 重开 Dashboard / 14:51:37 | 392616 | 366596 | 787567 | 新 PID 1037875 |
| 第二次关闭并等待 / 15:08:54 | 400878 | 368588 | 416586 | 已退出 |

- 证据依次为 `/tmp/patina-memory-hidden-report.json`、`/tmp/patina-memory-reopened-dashboard.json`、`/tmp/patina-memory-second-hidden.json`。daemon PID 583229 的 PSS 约 20 MiB，没有相似增量。
- 两次关闭后的 Desktop USS 相差 100044 KiB（约 97.7 MiB）。这不是 RSS 共享页重复计算可以解释的现象，但一次循环还不能区分对象泄漏、原生缓存和分配器未归还的空闲页。
- 当前锁定的 tauri-runtime-wry 2.10.1 在 Linux 上保留 WebContext，以复用 WebKit Network 进程；不能把仍存活的单个 Network 进程直接视为本次泄漏证据。相关上游背景为 [Tauri #14626](https://github.com/tauri-apps/tauri/issues/14626)。
- 本轮采用独立 HOME/XDG、D-Bus、虚拟显示和空数据库中的发布二进制堆分析，不对正式 Desktop 注入调试器、不读取正式数据库或 Token、不控制正式 daemon。虚拟 X11 结果不能直接替代 GNOME Wayland 验收。保持后台优化默认关闭、既有五分钟回收阈值不变。
- 正式版补采样 `/tmp/patina-memory-investigation-production-unchanged.json`（15:34:29 UTC）：同一 Desktop 的 RSS 仍为 517908 KiB，USS 为 367540 KiB，较第二次关闭后约 26 分钟前略降 1048 KiB；daemon PSS 为 20973 KiB。当前观察没有显示隐藏期间继续快速增长，不能据此排除重开引起的保留。

隔离对照已完成，证据位于 `/tmp/patina-heap-investigation-RInJGl`：

- 使用已验签 beta.12 DEB 的解包副本、`--daemon-client-preview`、空数据库、暂停追踪、关闭音频和网页 bridge、独立 HOME/XDG/D-Bus、Xvfb。beta.9 到 beta.12 的前端、主窗口回收及 Desktop runtime adapter 代码没有变化；但 profile 内容、显示后端、运行时长及 profiler 开销不同，不能直接把两组绝对内存差值当作优化收益。
- heaptrack 从启动开始采集，不附加正式进程。隔离副本的 GDB 附加被系统拒绝，未降低 ptrace 安全限制，也未执行 malloc_trim。虚拟 X11 中仍加载了 NVIDIA 库，不能称为纯软件渲染实验。
- 首次关闭在回收前几秒被提前重开，只算快速复用，排除出销毁对照。之后两次均等到 WebProcess 实际退出：15:31:16 UTC 的 Desktop PSS/USS 为 132249/107528 KiB，15:37:18 UTC 为 132537/107816 KiB。同一 Desktop PID 1130779 的 USS 仅增加 288 KiB；重建期间 WebProcess 从 1131064 换为 1156569，截图确认 Dashboard 正常恢复，Network PID 1131043 复用。
- 完整 `desktop.heaptrack.zst` 记录约 1040 秒，malloc 类分配峰值约 12.85 MB。`desktop.massif` 在两次销毁后的约 670 秒、1030 秒分别记录 9460210、9432690 bytes 存活堆，未见与正式环境 98 MiB 对应的增量。WebKit 自有分配器、直接映射和驱动缓冲区不保证被 malloc 拦截完整覆盖，不能据此宣称全部原生内存没有问题。
- 进程通过 SIGTERM 结束，报告中的 `leaked` 表示当时尚未释放的分配，并非本项目已证实的泄漏；原生库全局缓存和未运行的退出清理也可能计入。没有把该字段当作修复依据。
- 临时 Desktop、daemon、Xvfb、私有 D-Bus 及核对到的 portal 服务均已退出，所属测试进程组无残留。正式 Desktop PID 582450、daemon PID 583229 保持存活。中途读取未完成压缩流的临时报表有解析警告，已弃用；结论仅使用退出后完整读取成功的报告。

以上是第一轮仅有 X11 对照时的结论，后续 Wayland 和数据量对照见下一节。不能把实验环境变量直接放入正式启动配置，也不因空数据未复现就将内存验收关闭。

- 本轮调查未更新版本、安装或发布；仅修改本节调查记录，产品行为未变。

#### 系统链路与数据量对照（2026-09-10）

本轮确认了可复现机制，但不是正式实例已经修复的验收：批量读取在 Desktop 中创建 SQL 行、JSON 对象和 IPC 缓冲区，释放后 glibc 仍可保留大量空闲页。仅销毁 WebView 不保证这些页归还操作系统。

正式实例只读事实：

- Desktop PID 582450 的 RSS 517916 KiB，匿名映射私有驻留 282728 KiB、主堆 41740 KiB；46 个线程、67 个文件描述符。daemon PID 583229 仍是小体积后台进程，未随 Desktop 出现相似增长。分类后的映射/线程证据为 `/tmp/patina-heap-investigation-RInJGl/production-chain-audit.json`，不含堆内容、Token 或活动明细。
- 仅 stat 正式数据库文件，大小 223916032 bytes；仅读取用户此前给出的 ZIP 中央目录，备份共 229369337 bytes 未压缩条目，其中网页活动 197925520、会话 12304152、标题样本 17726598、图标 1329637 bytes。文件大小不是常驻内存大小；没有读取正式数据库行或备份条目内容，也没有按这一份旧备份推断当前各表行数。
- 现有 Linux `platform/linux/resource.rs` 把 `VmData` 填入兼容字段 `private_usage_bytes`，不能将其当作 USS；正式进程该值约 68 GiB，是虚拟地址空间口径。本轮使用 smaps 的 RSS/PSS/USS，不受此字段影响。修正诊断口径应单独保持字段兼容、缺失值语义和前端校验一致。

隔离方法与结果：

- 使用 beta.12 解包二进制、私有 HOME/XDG/D-Bus、随机 API 端口、暂停 tracking、禁用音频/网页 bridge，并借用本机 GNOME Wayland 显示服务。临时探针只在精确匹配解包可执行文件和临时 HOME 时生效；通过自身 GTK 窗口触发关闭，通过真实 Tauri 命令执行合成备份，不控制正式窗口。未更换 WebKit 分配器或图形环境变量。
- 合成数据为 50000 条应用会话、10000 条网页记录，启动前数据库均为 97558528 bytes（运行后配置和缓存写入会改变文件大小）。网页使用 `fixture.invalid` 和合成 favicon，没有复制用户数据。日期对照只把应用会话移到 400 天前，网页数据保持相同。分类迁移已完成的对照显式设置 `__classification_manual_confirmation_migration::v1`，排除一次性全历史迁移。
- 下表只列 Desktop 主进程，单位 MiB；开启 UI 时仍需另算 WebProcess，因此不把这些数字称为整个产品总量。每组为单次实验，不能当作跨机器预算。

| 场景 | RSS | USS | 证据目录后缀（`/tmp/patina-native-lab-`） |
| --- | ---: | ---: | --- |
| 空数据 Wayland，第二次窗口销毁后 | 237.1 | 86.5 | `vmZFNV` |
| 空数据，仅隔离 trim 后 | 211.6 | 61.1 | `vmZFNV` |
| 有数据，新配置打开 Dashboard，尚未备份 | 423.1 | 272.6 | `XrGFoU` |
| 完成合成备份、关闭并实际销毁 WebProcess | 418.7 | 268.4 | `XrGFoU` |
| 上一状态，仅隔离 trim 后 | 240.2 | 89.8 | `XrGFoU` |
| 相同数据量但日期较旧，仍需首次分类迁移 | 336.8 | 186.1 | `H1VEzo` |
| 分类已迁移，会话在当前查询范围内 | 395.0 | 244.5 | `qISbxb` |
| 分类已迁移，会话移出查询范围 | 236.4 | 85.8 | `1ZzMe1` |

- 有数据副本销毁后 `mallinfo2` 在用块约 13.1 MB、空闲块约 240.7 MB；隔离 `malloc_trim(0)` 后 USS 减少 182864 KiB（约 178.6 MiB），在用分配基本不变。`fordblks` 不是物理驻留计数，trim 后仍可包含不驻留的空闲地址空间，不能将其直接与 USS 相加。空数据重复销毁的 USS 增量仅 440 KiB。
- 短时堆分析在 `cl571g/startup.heaptrack.zst`，完整文本报告为 `heap-summary.txt`。约 59 秒内有 5503413 次 malloc 类调用，堆峰值约 112.11 MB。主要调用栈包括 `tauri_plugin_sql -> IndexMap<String, JsonValue>::insert`（该热点约 44.80 MB）和 `IpcResponse::body -> serde_json`（约 16.78 MB）。另有 Mesa/LLVM 图形分配；不同热点峰值不可直接相加。该组包含 profiler 开销，不能与无 profiler 的绝对内存直接比较，也不把 SIGTERM 退出时的 `leaked` 字段当作泄漏证明。
- 合成备份通过真实 `cmd_export_backup` 输出 107722343 bytes，ZIP 条目 CRC 检查通过。它制造了额外瞬时分配，但本组备份完成后的 RSS 未超过启动后的保留量，因此不能说备份是唯一原因，更不能把它当作每次重开增长的解释。

已对齐的代码链路：

1. `AppShell.tsx` 前台打开后调用 `prewarmDataFirstScreen`；Data 预热读取 7 日趋势和最近 53 周热力图。`dataReadModel.ts` 的热力图仍通过 `getSessionSummariesInRange` 获取逐条会话，返回给前端后才聚合。
2. `startupWarmupService.ts` 还预热分类候选；`classificationPersistence.ts::loadObservedSessionStats` 拉取范围内明细后再在前端合并。候选列表的最终 `limit=120` 不等于 SQL 明细读取上限。首次旧分类迁移另外调用起点为 0 的全历史查询，必须与每次启动预热分开看。
3. `tauri-plugin-sql` 的 SQLite 实现先 `fetch_all`，再为每行创建 `IndexMap<String, JsonValue>`，随后由 Tauri 序列化 IPC 响应；上述堆栈已实测命中。当前 Desktop 的这部分读模型还没有完全客户端化，不能误称所有重读数均已交给 daemon。
4. 前端缓存数量有限，隐藏会清理部分重缓存；WebProcess 已实际退出。Desktop 的 glibc arena 仍可能保留已释放页，形成长期高 RSS/USS。正式实例大映射形状与此相符，但未在正式进程内测量分配器，不能承诺其中每一 MiB 都可回收。
5. `data/backup.rs` 的导出同时持有完整 payload、多个 pretty JSON 字符串和 `Cursor<Vec<u8>>` ZIP；这是额外的大数据峰值风险。未来流式化必须保留一致性快照、checksum、归档兼容、原子发布与失败清理，不能在本轮调查中顺手重写。

后续实施边界与顺序：

- 先在已有低耗后台语义内评估一次性、受生命周期保护的 Linux 空闲堆归还；正式默认开关和五分钟等待不变。必须测试快速重开、widget、长查询并发、延迟及追踪持续性，不引入高频强制 trim 定时器。
- 根本优化留在分类/Data 读模型 owner：减少 Dashboard 启动时不必要的重明细预热，分离轻量分类配置与重候选统计，逐步让后端提供有界聚合结果。优先核对已有 trend/summary 与领域编译逻辑，避免重复 API；不得用简单 SQL SUM 破坏跨日、AFK、分类排除、导入优先级和 active session 语义。
- 随后独立处理备份流式化与网页元数据体积；不因为 ZIP 中网页记录最多就断言 favicon 是主要字段，字段级数量/字节统计尚未执行。
- 不以更换 UI 框架、减少 Tokio 线程数、删除数据或重启正式进程代替根因修复。当前产品代码和安装包均未改变，本轮只有调查文档变动；真实安装后的多轮回归仍待修复阶段完成。

收尾验证：2026-09-10 00:28 +0800，六组临时 Desktop/daemon 和私有 D-Bus 均已退出；仅删除经路径、文件类型与所有者校验的合成数据库及合成备份，回收 628991591 bytes，保留采样和堆分析证据（`native-cleanup.json`）。正式 Desktop/daemon 的 PID 与启动时间均未改变，最终 Desktop RSS/PSS/USS 为 517904/391916/368564 KiB，daemon 为 26404/21340/21232 KiB；无存活 WebProcess，Network 子进程仍在。正式数据库及用户备份未修改。这是调查收尾，不是内存优化验收。

#### 桌面内存第一批修复（2026-09-10，隔离验收通过）

范围限定为既有 Desktop 生命周期和启动预热编排，不改 tracking、数据库 schema、统计口径、备份格式或版本号。

- 取消 AppShell 每次回到前台时触发的 Data 首屏重预热；Data 页自身仍按需加载趋势和热力图，保留已持久化的聚合首屏缓存。
- 取消启动 warmup 的分类候选查询；分类规则仍由既有 ProcessMapper 初始化路径加载，Mapping 页继续自行加载完整候选。旧分类一次性迁移保持不变，不通过跳过迁移降低占用。
- Linux GNU 平台在低耗后台主窗口销毁请求成功后等待两秒，再到 blocking worker 做一次最佳努力的 `malloc_trim(0)`；重新确认低耗开关、同一次隐藏 generation 和所有 WebView 已消失。窗口仍在、快速重开或 widget 存活时跳过，不重试、不增加周期定时器。GNU 以外不调用此接口。该函数可与分配并发，但不能保证回收全部空闲页或消除重开时的分配延迟。
- 开关默认关闭与五分钟销毁等待不变。后端聚合和备份流式化仍属于后续批次；本轮不宣称重查询已全部迁往 daemon。

验收要求：前端完整 check、Rust check/test/Clippy、隔离 Wayland 合成数据的启动/隐藏/销毁/重开采样。正式安装、跨多轮长期使用和 widget/长查询并发的真实体验不得仅凭单元测试标为通过。

自动验证：`npm run check:full` 通过，Rust 572 passed / 6 ignored，Clippy、前端构建和 bundle 预算通过，31 项真实浏览器 smoke 覆盖 Data/History/Mapping 导航及热力图。首次沙箱内 release 子进程测试未取得预期 stderr，取得本机测试权限后完整重跑通过；浏览器 smoke 结束时临时 profile 清理出现 ENOTEMPTY 警告，不计作产品失败，也不隐去这项工具清理问题。`perf:startup-bootstrap` 的合成编排预算通过，不将其当作真实 GUI 启动速度测量。release 模式二进制编译通过，仅用于隔离验收，未安装或发布。

实际修复版对照证据：`/tmp/patina-native-lab-nxHM9Z/samples.jsonl` 和 `desktop.log`。临时二进制 SHA-256 为 `0debaafc4947ccd81b7252d7707bc7deda139c8507b0bd32182853d68eecea34`，由当前源码 release + `tauri/custom-protocol` 构建，不冒充已发布 beta.12 的相同成品。复用前述 GNOME Wayland 隔离方案、50000 条合成会话和 10000 条网页；分类迁移 marker 启动前设为 1 并只读复核。采集脚本最初打印的 migrationAlreadyComplete 字段遗漏新 mode，已修正显示条件；初始化 SQL 本身正确，不据错误打印字段判定迁移状态。

| 修复版阶段 | Desktop RSS / USS（MiB） | 结果 |
| --- | --- | --- |
| Dashboard 启动稳定 | 250.7 / 101.9 | 对应旧版已迁移、近期数据对照为 395.0 / 244.5 |
| 合成备份完成 | 394.0 / 245.2 | 仍存在备份内存峰值，未宣称流式化完成 |
| 关闭后，尚未销毁 | 393.8 / 244.8 | 没有在短时隐藏时提前 trim |
| 五分钟后实际销毁并自动回收 | 222.0 / 79.7 | 无 WebProcess；仅一次 released=true，调用耗时 17 ms |
| 销毁后重开稳定 | 257.3 / 108.2 | Desktop PID 不变，WebProcess 换为新 PID |

这次没有通过探针调用 trim，下降来自产品自己的销毁及空闲堆归还流程；不能把销毁前后全部降幅仅归因于 trim。快速重开复用旧 WebProcess，旧 hide generation 到期后未误销毁或多触发 trim。开启 UI 时仍须另算 WebProcess（初次 USS 266.8 MiB，重开 208.6 MiB），主进程的 79.7 MiB 不是整组产品总量。关闭回收后的 Desktop + Network 总 PSS 为 113.6 MiB，未包含独立 daemon。

同一隔离 daemon PID 保持存活；因追踪暂停，该项不是采样持续性实测。合成 SQLite quick_check/foreign_key_check 和 50000/10000 行数复核通过；107659856 bytes 合成备份 ZIP CRC 通过。临时进程和私有 D-Bus 均退出，仅清理指定合成数据库及备份 211792464 bytes，保留日志（`cleanup.json`）。正式进程只读复核见 `/tmp/patina-memory-after-first-fix-validation.json`，Desktop/daemon 的 PID 和启动时间均未变化。剩余门槛：正式安装后的多轮测试、活跃 widget/长查询及真实追踪连续性体验；单轮结果不等于所有场景内存问题关闭。

#### beta.13 外部审查核对与后续修复（2026-09-15）

审查基线为已推送的 `855ad0a`。用户报告安装后系统监视器读数由 257M 降至 219M；尚未确认进程分组、RSS/PSS/USS 口径和关闭等待时长，不能据此宣称真实数据长期验收通过。以下改动不包含在既有 beta.13 DEB 中。

本轮局部修复（owner 为 Desktop 窗口生命周期）：

- 确认最小化到 Widget 裸调用 hide，绕过主窗口隐藏代次与五分钟销毁计时。复用 `hide_main_window_for_background`，不改变低耗默认开关、等待时长或任务栏最小化行为。
- 确认 Widget 创建期间 close 看不到窗口时，完成创建后的取消分支只 park、不安排销毁。由已有 Mutex 状态在完成创建时返回最新取消代次，park 后安排既有销毁计时，不重新 hide 覆盖更新后的用户意图。路径解析提前到 begin_show 之前，避免解析失败遗留 create_in_progress。补测重复 close、创建中 reopen 和旧代次失效；不等同于所有原生窗口事件并发均已验收。

后续边界与顺序：

1. **启动与最后 WebView 回收**：确认 autostart 先创建隐藏 Main，读取偏好后再决定 Widget；确认 trim 仅挂 Main 销毁，Widget 最后销毁时没有补偿入口。由 Desktop 生命周期 owner 协调，平台层只提供 allocator 操作，不交给 daemon、不新增周期性 trim。延迟建窗必须保留手动启动、升级重开意图、设置加载失败可恢复、tray 可达，并避免晚到的启动流程隐藏用户已打开窗口。先验证 Destroyed 与 WebView 注册表清理次序，再决定取消代次、去重及最多一次补偿；固定两次尝试不能保证任意长 SQL 结束后回收。
2. **诊断口径**：确认 Linux private_usage_bytes 读取 VmData，应在下一轮性能验收前改为私有驻留页；smaps_rollup 不可读时返回未知，不用 VmData 兜底。字段扩展和 UI 保持兼容。
3. **Data/分类有界聚合**：确认 Data 53 周逐 session 查询、两套明细缓存及分类候选按需重查询仍存在。现有 trend handler 的 activity_read_model::load_snapshot 仍 fetch_all 全范围记录；仅改 HTTP 可减少 IPC 放大，但可能把峰值搬到 daemon。必须保持导入优先级、排除、分类、本地日期、active session 和采样截止语义，并测量 daemon 峰值，不能把响应只有数百条视为计算内存有界。
4. **流式备份**：确认完整 payload、pretty JSON 和 ZIP Vec 多份驻留。由 data backup owner 独立执行，保留一致性事务、归档/checksum 与恢复兼容、失败不覆盖目标；覆盖取消、磁盘满、长读事务下 WAL 增长及临时文件安全清理。
5. **Widget 独立入口**：确认与 Main 静态共用入口，后续拆分 bundle 并实测；静态依赖本身不能证明等量主 UI 常驻。

审查有一处事实纠正：Main 当前是 Mutex 内的 desired_visible 与 hide_generation，默认显示意图 false，不是默认 true 的 AtomicBool。是否需要显式 Absent 状态由启动和事件契约决定，不直接照搬重构；上述资源缺陷也不统一定为紧急 P0。

本轮不变更版本号、不重打包、不自动安装或触碰正式 runtime；不将合成实验约 113.6 MiB PSS 当作所有机器的承诺值。下一阶段跨路径回收和延迟建窗应先按上述边界补实现方案与真实 Wayland 验收，不混入数据读模型或备份迁移。

本轮源码验证：`npm run check:full` 通过，Rust 574 项通过、6 项忽略，Clippy、前端构建及 31 项浏览器 UI smoke 通过；浏览器临时 profile 清理仍报告 ENOTEMPTY 警告。7 项窗口状态单测通过，包含新增两项取消/重开回归；这些是状态与构建验证，不是原生 Widget 创建中取消或五分钟销毁的端到端实测。尚未提交、推送或安装本轮修改，远端审查基线仍为 `855ad0a`。

#### beta.14 原生窗口回归（2026-09-15）

复现入口现保存在 `scripts/native-window-lifecycle.mjs` 和仅 cfg(test) 编译的 `app/native_window_tests.rs`，默认 cargo test 跳过，需要显式运行。runner 先在正常构建环境编译，再以白名单环境、私有 HOME/XDG、私有 D-Bus、合成 SQLite 和真实 Wayland 窗口执行；不初始化 tracker、systemd 或产品 API。页面使用静态测试内容，不代表完整 React/IPC 或实际用户数据库验收。

第一次 `/tmp/patina-window-test-E5xyQ6` 受控取消 Widget 创建时复现真实崩溃：Tao 0.34.8 的 CursorIgnoreEvents(true) 对尚未 realize 的 GTK window 调用 unwrap，退出码 134。隐藏窗口不接收输入，因此移除 park 中多余的 set_ignore_cursor_events(true)，保留隐藏、尺寸、位置及重新显示流程。失败证据保留；runner 的子进程日志管道在退出后显式关闭，避免 D-Bus 激活子进程持有管道使 runner 不退出。

本轮受控回归范围：创建取消后的隐藏 Widget 在真实五分钟后销毁；重开后的 Main 不受旧计时影响；最小化到可见 Widget 后，Main 在真实五分钟后销毁而 Widget 保留；Main 可再创建。取消场景重开不调用普通 focus/close-Widget 回调，否则新排的销毁计时会掩盖原缺陷。计时常量不缩短，取消钩子只在测试编译存在。

候选版本准备为 `1.9.0-beta.14`，不覆盖 beta.13 文件名；签名配置、默认开关和数据协议不变。是否可以安装以本节后续原生结果、release gate 和成品检查为准，不把测试包准备等同于公开发布。

最终结果：

- 修复后的 `/tmp/patina-window-test-RGIDzX` 原生回归通过，运行 626.64 秒、退出码 0、未超时。取消 Widget 已销毁，旧 Main timer 已失效；第二轮 Main 已销毁、Widget 可见，再次创建 Main 成功。本轮没有缩短两次五分钟计时。
- 原生回归编译时仍标为 beta.13，产品窗口修复与最终候选一致；之后版本切到 beta.14，测试内数据库断言改为复用现有 data 测试助手以遵守 SQL 边界。最终源码由完整 release gate 编译验证。对原生实验库另行只读复核 quick_check=ok、foreign_key_check 为空、sessions=0，不把这些结果扩大为真实用户数据或完整 UI 验收。
- `npm run release:check` 通过：574 项 Rust 测试通过、7 项忽略（含独立执行的原生长测），31 项浏览器 UI smoke、前端构建、Clippy 和三类扩展检查通过。浏览器 smoke 仍有临时 profile 清理 ENOTEMPTY 警告；私有 portal 因隔离环境缺少 PipeWire/GNOME 窗口服务出现警告，不代表正式 tracking 故障。
- 两个测试进程组最终均已退出。第一次失败实验的私有 document portal 未响应 SIGTERM，经临时 HOME/runtime 与进程名复核后定点终止；runner 收尾增加私有进程组的一秒退出宽限及强制清理，关闭继承日志管道。最终 runner 语法检查通过，该清理增补未再重复十分钟窗口实验。保留两个小型合成 fixture 和日志，没有清理或读取正式数据库。
- 本地构建命令为 `npm run tauri build -- --bundles deb --config '{"bundle":{"createUpdaterArtifacts":false}}'`，`release:verify-daemon-deb` 通过。成品 `src-tauri/target/release/bundle/deb/Patina_1.9.0-beta.14_amd64.deb`，25602832 bytes；包元数据 `patina / 1.9.0-beta.14 / amd64`，包含匹配 Desktop/daemon、unit 和 GNOME 扩展。
- DEB SHA-256：`acc08e8a16446b68ef58c88ab10d6d3a2a6478339cf3b19e80102c71226092a7`。未读取私钥，永久 updater 配置、公钥和地址不变。
- 用户已安装并重开 beta.14，报告系统监视器主进程关闭前后约 258M/220M。只读 `smaps_rollup` 分组采样在无 WebProcess 时记录 Desktop 主进程 RSS/PSS/USS 约 221.7/89.6/74.0 MiB，Network 进程 PSS/USS 约 13.0/8.8 MiB，Desktop 与 Network 合计 PSS/USS 约 102.6/82.8 MiB；daemon PSS/USS 约 13.0/12.9 MiB。该样本说明 220M RSS 不能直接解释为同量私有内存，不证明任意长期循环均无泄漏。
- beta.14 已以 `0e00c4a` 提交在 `feature/patinad-daemon`，尚未推送、打 tag 或公开发布；远端仍为 `855ad0a`。有界聚合、流式备份和 Widget 入口拆分仍未完成，不承诺可见 Widget 存活时内存归零或固定预算。

#### Desktop 生命周期第二批修复（2026-09-16，原生验证通过）

本批 owner 仍是 Desktop 宿主生命周期与 Linux 资源诊断，不修改 `patinad` tracking、数据库 schema、统计语义、备份格式、低耗默认值或五分钟窗口销毁等待。

- allocator 回收从 Main 私有计时器移到 Desktop 级最后 WebView 边界。Main 或 Widget 发出真实 `Destroyed` 事件后，宿主将请求按 generation 合并，等待两秒再重新检查低耗开关、退出意图、最新请求和空 WebView 注册表；只在 GNU Linux blocking worker 执行一次 `malloc_trim(0)`。重开窗口、仍有 Widget/Main、关闭低耗或退出应用都会跳过；不增加周期 trim，也不保证捕获任意更晚结束的长查询。
- 登录自启动不再先构建隐藏 Main。SQLite 初始化后同步读取桌面行为和更新后重开意图，再形成 `MainVisible`、`MainTaskbarMinimized` 或 `WidgetOnly` 计划；默认 Widget 自启动只创建 Widget。手动启动和更新后重开始终显示 Main，任务栏模式仍保留真实 Main。设置读取失败时手动启动回退显示 Main；自启动保留 tray 可达并记录错误。
- Wayland 在第一个窗口创建前可能无法从应用上下文取得主显示器。Widget-only 启动会先构建不可见的 1x1 窗口，再以透明、跳过任务栏的最小 surface 完成 monitor discovery，随后应用正常边界并显示；发现失败会恢复隐藏状态并进入已有延迟清理，不遗留永久 WebView。该实现尚需安装包观察是否存在可见的一像素闪现，不将原生回归通过扩大为视觉验收。
- Linux 进程内诊断从 `smaps_rollup` 读取 RSS/PSS/Swap，以 `Private_Clean + Private_Dirty + Private_Hugetlb` 计算 USS；缺失字段返回未知。兼容的 `working_set_bytes/private_usage_bytes` 在 Linux 上分别映射 RSS/USS，不再读取 `VmData`。外部进程分组脚本仍是跨 Desktop/WebKit/daemon 验收主工具。
- 单元测试覆盖回收请求失效与全部门禁组合、启动计划组合、smaps 解析和缺失字段语义。真实 Wayland 原生回归 `/tmp/patina-window-test-lTEFEe` 在 630.09 秒后通过：默认 autostart 只创建 Widget、不创建 Main；创建取消后的 Widget 被清理，旧 Main timer 不误销毁重开窗口；Main 销毁而 Widget 存活时不回收，最后 WebView 销毁后只执行一次 app-level 回收；合成数据库保持 `sessions=0`。隔离 portal 的 PipeWire/窗口列表警告不影响断言。当前仍不形成新 beta，不打包、不安装，也不触碰正式 runtime。
- 最终 `npm run check:full` 通过：Rust 580 项通过、7 项忽略，Clippy 以 `-D warnings` 通过，前端测试、31 项浏览器 UI smoke、生产构建和 bundle budget 通过。浏览器 smoke 仍有临时 profile 清理 `ENOTEMPTY` 警告，不影响断言。第二批尚未推送、打包或安装；已安装版本仍是 beta.14 第一批生命周期修复。

#### beta.15 悬浮窗闪烁与内存候选（2026-09-16）

- 用户报告 Linux 悬浮窗频繁闪烁。代码确认移动事件会重新发起吸附，延迟到达的 compositor 事件可再次触发布局；原生布局每次重复设置尺寸、位置、置顶和显示。该链路是可修复的反馈来源，尚不能确认覆盖用户全部闪烁现象。
- Widget controller 仅在用户拖动结束的待收尾阶段响应移动通知；无用户拖动的通知不写布局。原生已显示窗口不重复 show/置顶，尺寸和位置相同时跳过调用。回归模拟 compositor 保留偏移位置并连续通知，确认只吸附一次，同时保留漏发移动事件和拖动释放竞争场景。
- 本地候选版本为 `1.9.0-beta.15`，包含 `0e00c4a`、`0b79846` 生命周期修复及上述变更。安装验收需要观察静置、展开/收起、拖动、切换应用、隐藏/重开和自启动，随后按 Desktop/WebKit/daemon 分组记录 PSS/USS。真实视觉验收仍待用户安装后完成。
- `check:full` 通过：580 项 Rust 测试、31 项浏览器 smoke、前端/replay/构建和 Clippy；7 项 Rust 测试按设计忽略。仍有浏览器临时 profile 的 `ENOTEMPTY` 清理警告。版本、changelog、GNOME/Chromium/已签名 Firefox 扩展校验通过。本批没有重新执行十分钟原生长测，上一批结果不能代替本批实际视觉验收。
- 本地 DEB 已构建并通过 `release:verify-daemon-deb`：`src-tauri/target/release/bundle/deb/Patina_1.9.0-beta.15_amd64.deb`。SHA-256 为 `a7c16d48b4395b97bff1854016f9aeacc5b884a06ad3763f02895f198ac3107a`。构建只通过 CLI 关闭 updater 签名产物；未安装、推送、打 tag 或公开发布。

#### beta.16 Wayland 悬浮窗边界修正（2026-09-17）

- beta.15 安装后用户报告拖动仍频繁闪烁且只吸附左边，主进程读数约 246M→212M；因此 beta.15 不能算闪烁验收通过，也不能把该 RSS 读数作为私有内存预算。
- owner 为 Desktop Widget 与 Linux 窗口平台边界。GTK Wayland 不提供可靠全局位置，X11 QueryPointer 也不能代表原生 Wayland 拖动。按实际 raw display handle 区分后端，X11 保留吸附；原生 Wayland 保留 compositor 拖动、禁用无效绝对定位和坐标推导，不改写保存的左右偏好。全局按键查询不可用返回 null；DOM pointercancel 不当成松手，释放或后续本地 pointer enter/move 的 buttons=0 结束拖动。
- 首次 Widget 的 monitor discovery 需要 GTK 事件循环，自启动读取设置后异步创建 Widget，避免 setup 的 block_on 阻塞首次映射。GNOME 扩展目前仅焦点读取；完整 Wayland 吸附需要后续设计受限的 compositor provider，不能把本轮降级标为功能完成。
- 验收范围：无全局坐标时不写位置、不吸附；X11 实际后端仍可吸附；拖动中不提前收尾，释放后能继续展开/收起；用户静置闪烁需安装观察。完成此项后继续既定 Data/分类有界聚合、流式备份和 Widget 独立入口。
- 自动检查：581 项 Rust 测试、31 项浏览器 smoke、前端/replay/构建通过；Clippy 首次提示 Option 的 `let...else` 可用 `?` 简化，等价修改后单独复查通过。GNOME/Chromium/已签名 Firefox、版本和 changelog 校验通过。浏览器临时 profile 仍有 `ENOTEMPTY` 清理警告。
- 本地 DEB 构建与 `release:verify-daemon-deb` 通过，成品 `src-tauri/target/release/bundle/deb/Patina_1.9.0-beta.16_amd64.deb`，SHA-256 为 `76120b42757a379e8df68d3be245e967eb69f66361be77218f6b112d0bef135a`。未安装、推送或发布，真实拖动与闪烁不因编译通过而视为验收完成。
- 真实 Wayland 生命周期回归 `/tmp/patina-window-test-qSOb4H` 通过，耗时 630.18 秒，runner 退出码 0。默认 autostart 只创建 Widget，创建取消清理、旧 Main timer 失效、Main 销毁而 Widget 保留、最后 WebView 一次回收和合成数据库不变均通过。隔离 portal 仍有缺少 PipeWire/窗口列表的警告，收尾 D-Bus 断连提示未导致测试失败；该静态测试页面不覆盖 React 悬浮窗的真实拖动视觉表现。

下一阶段的有界聚合先落 Data heatmap：由 `data/repositories/activity_read_model.rs` 提供一致性读取，`domain/activity_read_model.rs` 保留本机记录优先、导入桶分配和区间裁剪语义，Desktop command 与 daemon handler 复用同一计算入口。前端仅接收按本地日期聚合的 DTO，移除 heatmap 的两套逐会话缓存。当前 `load_snapshot` 的 `fetch_all` 与 contributions 多份克隆均需纳入峰值测量；按天分批仍不能保证单日极端数据有界，不能直接宣称流式完成。先确定单次读取上限、重叠活动处理及一致性事务，再以跨日、DST、active cutoff、排除与导入覆盖对照现有结果；随后才扩到分类候选和其他趋势视图。

#### beta.13 本地安装候选（2026-09-15）

- 当前分支仍为 `feature/patinad-daemon`，版本文件统一为 `1.9.0-beta.13`；只更新 Cargo.lock 中的自身版本，未升级依赖。CHANGELOG 已记录本批内存修复与剩余限制。
- `npm run release:check` 通过，包括 572 项 Rust 测试（6 项忽略）、Clippy、31 项真实浏览器 UI smoke、前端构建和 GNOME/Chromium/Firefox 扩展检查。浏览器 smoke 有临时 profile 清理 `ENOTEMPTY` 警告，不影响测试结果，不能称为无警告执行。
- 本地构建命令为 `npm run tauri build -- --bundles deb --config '{"bundle":{"createUpdaterArtifacts":false}}'`；匹配版本的 Desktop 与 daemon 均完成 release 编译。只在本次 CLI 关闭 updater 签名产物生成，没有读取私钥或修改持久化签名配置、公钥和更新地址。
- 成品：`src-tauri/target/release/bundle/deb/Patina_1.9.0-beta.13_amd64.deb`。包元数据为 `patina / 1.9.0-beta.13 / amd64`，`release:verify-daemon-deb` 通过，包含 Desktop、daemon、默认未启用的 user unit 和 GNOME 扩展。
- DEB SHA-256：`bbee0f1a76b843881c0803c48024c0e7ec89069461f70e6c81f58703f1c2e15b`。
- 尚未安装、提交、推送、打 tag 或公开发布，不改变 beta.12 的已发布状态。安装会升级同名 `patina` 包，不是隔离副本；安装前保留可用备份，安装后完全退出并重开 Desktop，按诊断确认 Desktop/daemon 版本，需要时使用已有确认式后台重新加载。
- 实机下一步：开启并保存“低耗后台”，关闭主窗口且不保留 widget，等待超过五分钟后观察进程分组内存，再重开重复两至三轮。分别记录 Desktop、WebKit 与 daemon，不能把单主进程 USS 与系统监视器应用总量混为一谈。真实数据长期表现、长 SQL 与回收重叠仍待验收，不以本地包构建成功代替这些验证。

#### 内存回收边界补验（2026-09-10，隔离验收通过）

使用同一修复候选（SHA-256 见上一节），不改默认值、超时或产品代码。临时探针只对精确候选路径和匹配的私有 HOME 生效；通过真实 Tauri command 控制测试开关和 widget，不调用手动 trim。每组仍为独立 HOME/XDG/D-Bus、随机端口和合成数据库，不操作正式服务。

| 场景 | 当前结果 | 证据目录（`/tmp/patina-native-lab-` 后缀） |
| --- | --- | --- |
| 启动时低耗关闭 | 超过五分钟后主窗口仍隐藏存活，trim=0；重开 DOM 就绪，数据库完整 | `yKjTWi` |
| 隐藏后关闭低耗开关 | 已排队的回收被取消；主窗口保留、trim=0，重开及数据库检查通过 | `n1OvcN` |
| 主窗口隐藏后打开 widget | 主窗口到期销毁，widget 仍可见，trim=0；重开及数据库检查通过 | `bapHBj` |
| 同一 Desktop 两轮销毁/重开 | 同 PID 完成两轮，每轮 trim 一次（5/3 ms），两次重开 DOM 就绪；数据库完整 | `cnJRnE` |
| 合成窗口持续记录 | 销毁前后保留同一 active row；357 次采样最大间隔 1119 ms，切换应用后封口 364261 ms；重开、数据库和记录数检查通过 | `V4ThvT` |

四组生命周期实验并行使用本机 Wayland，不能把它们的绝对 PSS/USS 与上一节单副本数字直接当作性能回归；共享页归属会随其他副本退出而变化。重复回收组额外采集 RssAnon、Private_Dirty 和 Private_Clean，以区分匿名内存与共享页口径变化。矩阵日志在 `/tmp/patina-boundary-matrix-iWE9wa`。

持续记录组只在私有 D-Bus 导出合成焦点和 idle 数据，不读取真实窗口内容。首个测试尝试发现断言误用了 active row 的 `duration`（按设计为 null），已主动停止并清理临时进程，改为可信采样时间推进、采样间隔和最终封口时长后重跑；这是测试修正，不是修改会话存储语义。真实安装、长时间使用以及长 SQL 查询与回收重叠仍保留为后续验收，不据模拟 provider 扩大为真实 GNOME 环境全部通过。

两轮回收均在真实五分钟阈值后执行，没有缩短测试用阈值。第二轮 RssAnon 从 97640 降到 61980 KiB；同期其他副本退出，Private_Clean 从 1184 增至 29012 KiB。因此第一次/第二次销毁后的 USS 49008/91148 KiB 不能直接作为同条件内存增长对照。两轮 malloc 在用块约 16.03/18.25 MB，也不据此宣称零增长或长期平台已经建立；本组证明生命周期及再次回收有效，不代替单副本长期预算。

五组最终验收均通过，持续记录组的 daemon PID 1330095 保持不变，原会话 id 50001 在销毁前后相同；可信采样时间推进 321398 ms，切换合成应用后产生第二条会话，最终合成数据计数为 sessions=50002/web=10000。其他四组暂停 tracking，计数保持 50000/10000，均通过 SQLite quick_check/foreign_key_check。本轮没有再修改产品代码或重建候选；上一轮完整 check 对应的候选哈希未变，新增的是实际边界实验。

收尾：六个临时 profile（含主动中止的一组）的 Desktop/daemon、合成 provider 和私有 D-Bus 已退出，额外匿名内存采样也已结束；只清理精确合成数据库文件，共 624795648 bytes，证据在矩阵目录的 `cleanup.json`。正式 Desktop/daemon 的 PID 及启动时间未变化，见 `/tmp/patina-memory-after-boundary-tests.json`。未安装、提交、推送或发布；下一步准备安装候选并进行真实数据多轮复测，长 SQL 重叠仍为独立待验项。

#### 本地恢复首次实测（beta.8）

新增显式 opt-in 测试 `app/daemon/backup_restore/systemd_tests.rs`，使用已安装的 `/usr/bin/patinad` beta.8；常规 `cargo test` 默认忽略，运行方式见开发文档。每个场景创建随机临时 user unit 和独立 `0700` HOME/XDG 根，固定使用其内部 Dev profile、随机端口和私有凭据，源归档通过既有 Rust exporter 从合成记录生成。追踪暂停、音频/网页/远端状态禁用，子进程断开桌面 D-Bus。没有读取用户备份或调用正式 `patinad.service` 的写操作。

| 场景 | PID 交接 | 证据目录（内含 `0600` evidence.json） |
| --- | --- | --- |
| Replace | 443222 → 443245 | `/tmp/patina-systemd_test_d6ab3fd68f409ac4b56495df3e9ac305` |
| Merge | 443277 → 443301 | `/tmp/patina-systemd_test_94df64afdebacc416d69664bcde3eed2` |
| Replace INSERT 失败 | 443405 → 443432 | `/tmp/patina-systemd_test_8d25bdbaca2acbe8b9890dc06c2d8000` |

三条路径均从真实 HTTP `202` 预约，经 daemon 受控退出与 systemd 新进程消费，到终态查询和数据库复核。Replace 仅保留源记录，Merge 保留源与原有记录，成功各一条 receipt 且只清理所属 staging；失败事务保留原有记录、无 receipt、保留失败归档。源 ZIP 与无关文件不变，quick_check/foreign_key_check 通过。完整 Rust 校验为 565 passed / 4 ignored，Clippy 与边界检查通过；显式运行该 ignored 测试通过（内部三场景）。第一次运行出现的清理提示来自显式 stop 后 Drop 再次停止已卸载单元，已修正并完整重跑；最终无残留测试单元。正式服务仍 active，PID 1473、NRestarts=0。

范围限制：使用当前用户 manager 的隔离临时服务，不是新账号或完整隔离登录会话；复用固定服务环境标记以协商协议，实际 unit 名与产品服务不同，不代替包内所有 sandbox 配置验证。已证明正常受控进程交接及恢复事务失败，不包括任意断电/进程被杀时刻、真实 WebDAV 服务或远端凭据生命周期。小型合成证据保留在临时目录，不自动递归清理目录。

#### 隔离维护与恢复验证（2026-09-09）

复用现有 restore owner 和 data 测试，新增三项测试，全部使用合成数据、临时磁盘数据库或内存数据库。未读取用户导出备份，不调用 systemctl，不操作 Production/Local/Dev 的现有目录；SQL 造数和故障注入只在 data 层 cfg(test) 辅助模块内，生产恢复代码不变。

- `isolated_restore_survives_reopen_and_receipt_replay_for_both_strategies`：分别验证 Replace/Merge，真实导出 ZIP、创建 staging、调用 schedule、等待内部 restart 通知，关闭并重新打开 SQLite 后执行 startup restore；替换只保留备份记录，合并保留现有记录。模拟数据库已提交但 reservation 仍为 running，再次打开数据库时根据 receipt 收敛，不重复恢复或删除后续新记录。原备份及无关 staging 文件保留，只移除本次票据。
- `isolated_replace_failure_rolls_back_deletes_and_records_no_receipt`：合成 ZIP 通过校验后，在恢复 INSERT 阶段用 SQLite trigger 注入失败，确认先前 DELETE 随事务回滚；重新打开数据库仍保留旧数据、无成功 receipt、failed 不自动重试。错误 request ID 不清理票据，显式取消只清理所属 staging 文件。quick_check 和 foreign_key_check 通过。
- `isolated_cleanup_failure_rolls_back_earlier_table_deletions`：清理在后续网页表 DELETE 失败时，之前的原生会话/标题删除全部回滚，数据库完整。

执行结果：目标测试 3/3 通过；完整 `npm run check:rust` 最终为 564 passed / 3 ignored，Clippy 与边界检查通过。首次受沙箱 loopback 绑定限制，30 项已有网络测试报 EPERM；获得测试执行权限后完整重跑通过，不忽略失败项。本次不修改版本、不重打 DEB。上述是进程内集成测试和真实磁盘重开，不等同于 OS 进程崩溃、真正 systemd restart、真实远端服务或任意断电时刻；这些门槛继续保留。

<details>
<summary>beta.5 至 beta.7 实机证据时间线（当时的待办不代表当前状态）</summary>

重启登录后台自启验收通过（beta.7，2026-09-09）：用户重启后登录，未打开 Desktop。boot ID 从 `1db84155-debf-4dae-b8f8-a118e6f52eb4` 变为 `a47013be-b2a0-4ea1-8f3d-a1674e47cdae`；本次 boot 的 systemd 日志记录 15:10:45 +0800 自动启动 patinad。`/tmp/patina-beta7-after-reboot-login.json` 严格 managed 检查通过：API serverVersion=1.9.0-beta.7、unit enabled/active、PID 与 lease 均为 1473、NRestarts=0、quick_check=ok。进程检查仅有 patinad、无 Patina。新 daemon 启动后已有 sessions=5、web=20 条新增记录，旧 session 跨新 daemon 启动边界计数为 0；settings 保持 background_tracking_at_login=1、launch_at_login=0。证明当前 Linger=yes 下重启登录后无需 Desktop 即可恢复追踪；不宣称已验证 Linger=no、仅注销/登录或登录前隐私行为。

登录启动待验基线（beta.7）：用户已开启后台登录启动并关闭 Desktop 登录启动。`/tmp/patina-beta7-before-relogin.json` managed 检查通过，service enabled/active、PID 686373；SQLite settings 为 background_tracking_at_login=1、launch_at_login=0、start_minimized=1，`~/.config/autostart/Patina.desktop` 不存在。completed reservation 中 desktopLaunchAtLogin=true 是交接时快照，不能替代当前 Desktop 偏好；completed 启动路径仅对账后台偏好，不据此重新开启 Desktop 自启动。宿主 loginctl 确认 Linger=yes，注销不能保证 user manager/daemon 退出，因此改为用户主动系统重启后验收，不改 Linger。重启前 boot ID 为 `1db84155-debf-4dae-b8f8-a118e6f52eb4`。用户重启并登录后先不打开 Patina，核对新 boot、新 daemon、无 Desktop 进程及追踪写入；当前尚未执行此项，不标为自启通过。

`2026-09-09 / beta.5` 回滚进展：service 已停止并禁用，reservation 已持久化为 `rolled_back`；用户强制退出并重开 Desktop 后，lease 转为 desktop，原生活动恢复且 SQLite `quick_check=ok`。回滚后的自动重启未完成，不能将强制退出后重开视为自动重启通过。随后用户确认重新接管自动重启成功，`/tmp/patina-beta5-recutover-verified.json` 的 managed 检查通过：reservation completed、daemon lease、API 与 tracking ready，数据库完整；unit disabled 是当前登录偏好，不代表服务没有运行。验收采集器已修正将磁盘 `rolled_back` 错按诊断接口 `rolled-back` 比较的误报，并补充回归测试。

`beta.6` 候选修复：回滚和重试命令统一使用 `request_restart()`，避免异步命令调用永不返回的 `restart()` 后持续占用执行线程与服务操作锁。这是已发现的明确风险，不等于已证明上次卡住的唯一根因。先安装候选并复测无需强制退出的回滚及再次接管，再进入卸载/重装的数据保留验收；实际休眠边界还需系统 suspend/resume 证据，不能只用用户唤醒反馈和追踪空档替代。

`beta.6` 本地验证：`release:check` 全部通过（Rust 560 passed / 3 ignored，30 项真实浏览器 smoke，Clippy 与扩展检查）；版本一致性与成品 `release:verify-daemon-deb` 通过。本地未签名安装候选为 `src-tauri/target/release/bundle/deb/Patina_1.9.0-beta.6_amd64.deb`，SHA-256 为 `86d9e3d4b26f8bad48b3cfba85658b9e6d3ad888e1389b178ce1bed6a4ea42ba`。这是本地安装候选，不表示已发布。

`2026-09-09 / beta.6` 回滚复测通过：用户确认不再卡住，已自动回到桌面内置追踪并显示 beta.6。宿主只读证据 `/tmp/patina-beta6-rollback-host-verified.json` 的 rolled-back 检查全部通过：固定 unit inactive/disabled、ExecMainPID=0、reservation rolled_back、lease role=desktop，且对应 Desktop PID 603711 经进程检查仍在运行；SQLite quick_check=ok，activeSessions=1。最初受沙箱限制无法访问 systemd 的采集不作为服务状态证据。本次无需强制退出的回滚已通过。随后用户确认重新接管成功，`/tmp/patina-beta6-recutover-host-verified.json` 的 managed 检查全部通过：service active、reservation completed、daemon lease PID 与 ExecMainPID 均为 614399，API/tracking ready、quick_check=ok、activeSessions=1。unit disabled 保留当前未登录自启的偏好，并非运行失败。beta.6 的回滚与再次接管闭环通过；卸载/重装的数据保留及实际系统休眠证据仍待完成。进入卸载前先导出并验证最新备份，保存在当前数据目录之外；然后正常退出 Desktop 并停止固定 daemon 服务，再执行包卸载，不删除 profile 或使用 purge。

`beta.6` 备份阻塞项：用户点击备份后尚未出现保存窗口，Desktop 即闪退。宿主日志确认主线程在 zbus executor 抛出 `there is no reactor running`，panic 穿过 WebKit 回调导致 abort。同步 Tauri picker 调用了 portal-backed 同步 rfd API，缺少 Tokio runtime；`beta.7` 将五个相关选择入口改为 async command + AsyncFileDialog，不改备份数据格式或写库逻辑。闪退后 `/tmp/patina-beta6-after-backup-crash.json` 的 managed 检查仍通过，daemon 与数据库正常；这不代表备份成功。卸载验收暂停，待新包验证打开/取消选择器、实际导出和归档预览后继续。

`beta.7` 本地验证：完整 `release:check` 通过，Rust 561 passed / 3 ignored，30 项真实浏览器 smoke、Clippy、扩展检查及版本一致性均通过。`src-tauri/target/release/bundle/deb/Patina_1.9.0-beta.7_amd64.deb` 已通过成品验证，SHA-256 为 `f6748afb07039e898627db8c1023bb3aed679a63d04f165c0e31dfc13e924a27`。这是未签名的本地安装候选；自动化未覆盖真实系统保存对话框，待用户安装后验证打开/取消、实际导出及归档预览，未发布。

`beta.7` 文件选择器实机进展：用户确认已完成覆盖安装、重新打开以及保存窗口打开/取消和导出操作。`/tmp/patina-beta7-after-backup-verified.json` 的 managed 检查通过，包版本为 beta.7，service active、daemon lease 一致、quick_check=ok。daemon PID 仍为升级前的 614399，因此不把包版本检查当作新版 daemon 已运行的证明。备份文件路径和归档完整性尚待核验；卸载验收仍暂停，新版 daemon 重启后须另行确认。

`beta.7` 导出归档完整性已核验：`/home/arinp22/Documents/Patina-backup-20260909-133503.zip` 是数据目录外的普通文件，权限 0600，大小 229370935 字节，SHA-256 `ba996f28eb19cd798dcee2180e8997c69ef72507101e222a1f849dcd7865e177`。ZIP 全条目 CRC、checksums.json 的 CRC32、JSON 解析、manifest 文件清单与记录数一致；13 个条目无重复名称、路径穿越或 symlink，大小均在读取限制内。format=PatinaBackup、backup_version=1、schema_version=10、app_version=1.9.0-beta.7，包含 sessions=45494、title_samples=103858、web_activity_segments=60123。仅只读验证，没有执行恢复；产品内归档预览和恢复演练不能由此替代。下一步先确认产品内预览可读，不确认恢复，再正常退出 Desktop，准备停服和卸载前最终基线。

`beta.7` 产品内读取验证：用户选择上述本地备份后显示“恢复策略”框，未点击恢复。代码确认该框仅在 `prepareBackupRestoreWithDeps` 调用 `previewBackup` 并通过 restoreSupported 检查后打开，因此可记录产品内解析/兼容检查通过，但不能记录“已展示摘要”或“已恢复”。当前 UI 把摘要放在策略框“恢复”按钮之后的第二次确认框中；先前要求“选择后直接看到预览”的指引不准确。用户可直接取消策略框，无需执行任何恢复。此按钮命名和摘要位置存在歧义，后续 UI 改善应保留最终确认与写入边界，不在卸载验收中扩大修改范围。下一步正常退出 Desktop，再准备停服和卸载前最终基线。

`beta.7` 卸载前停服基线：用户正常退出 Desktop 后，经进程检查确认已退出，再以 `systemctl --user stop patinad.service` 正常停止服务。`/tmp/patina-beta7-before-uninstall-stopped.json` 检查通过：service inactive/dead、ExecMainPID=0、ExecMainStatus=0，Patina/patinad 进程均不存在；数据库 quick_check=ok，sessions=45525、webSegments=60136、activeSessions=0、activeWebSegments=0，Token 普通文件 0600 保留。磁盘 lease 元数据仍记录旧 PID 614399，不视为运行 owner，不删除该文件。停服时 systemd 提示 unit 文件变更尚未 daemon-reload，卸载/重装后需复核加载状态。当前仅完成停服和基线采集，尚未卸载；后续只执行包 remove，不执行 purge、autoremove 或手工删除 profile，卸载后逐项核对包文件移除、数据/备份/Token 保留及记录数量不变。

`beta.7` remove 卸载验收通过：用户执行包卸载后，`/tmp/patina-beta7-uninstalled-verified.json` 检查通过。Desktop、daemon、unit 包属文件均不存在，systemd LoadState=not-found、ActiveState=inactive，Patina/patinad 均无残留进程。数据库保留且 quick_check=ok，counts 与停服前逐字段一致（sessions=45525、webSegments=60136、activeSessions=0、activeWebSegments=0）；Token 普通文件、0600 权限及文件元数据与基线一致，未输出其内容。外部备份 ZIP 的 SHA-256 仍为 `ba996f28eb19cd798dcee2180e8997c69ef72507101e222a1f849dcd7865e177`。此项仅覆盖 remove，不代表 purge/自动清理或恢复演练已验证。下一步重装同一 beta.7 DEB，先不打开 Desktop，核对安装不会自行启动服务，再启动 Desktop 检查新版 daemon 和追踪恢复。

`beta.7` 重装后、首次打开前验收通过：用户重新安装且尚未打开 Desktop，`/tmp/patina-beta7-reinstalled-before-open.json` 的 installed 检查通过。包版本 beta.7，Desktop/daemon/unit 文件恢复，systemd loaded 但 inactive/dead、disabled、ExecMainPID=0，进程检查确认 Patina/patinad 均未自动启动。SQLite quick_check=ok，全部 counts 与卸载前停服基线一致，Token 文件元数据一致。安装未意外开始追踪；首次打开后的新版 daemon 身份、owner 和追踪恢复仍待验证。

`beta.7` 重装首次打开验收通过：`/tmp/patina-beta7-reinstalled-opened.json` 的 managed 检查全部通过，API serverVersion=1.9.0-beta.7，service active、daemon lease 与 ExecMainPID 同为新 PID 686373，tracking owned/ready；进程检查为一个 Patina 和一个 patinad。SQLite quick_check=ok，activeSessions=1。只读核对停服边界之前仍有全部 45525 条会话，跨越停服边界的旧会话为 0，首次新增会话 start_time=1788933020074，停服区间未被补入旧会话。浏览器桥接 owned/ready，但该快照网页数量仍为 60136，尚不作为最新扩展重连证据。remove/重装/追踪恢复闭环已通过，不代表 purge、恢复演练或完整系统休眠验收通过。

`beta.7` 重装后 Zen 扩展重连和网页封口验收通过：用户在 Zen 浏览后切到其他应用，`/tmp/patina-beta7-reinstalled-zen-verified.json` 的 managed 检查通过，daemon PID 686373、NRestarts=0、quick_check=ok。网页数量从 60136 增至 60141，activeWebSegments=0。只读查询确认 5 条新增记录均为 browser_kind=firefox、browser_exe_name=zen、source=browser-extension，均有关联 native session 且边界位于该 session 内；最后一段 duration=19984ms，已封口。未读取或输出 URL/标题。最新重装后的扩展重连缺口已关闭；实际系统挂起/恢复日志与追踪边界的联合验收仍待完成，不用锁屏或单独的活动空档代替。

`beta.7` 实际系统挂起/恢复联合验收通过（2026-09-09）：systemd-suspend 日志确认 14:01:44 +0800 进入 suspend，14:07:46 返回，14:07:47 unit 正常完成。数据库只读核对 [1788933704000, 1788934066000) 区间：sessions、web_activity_segments、session_title_samples 的重叠计时均为 0。挂起前最后会话结束于 1788933701020，恢复后首个新会话开始于 1788934070866，之后继续产生新会话。`/tmp/patina-beta7-after-suspend.json` 的 managed 检查通过，API serverVersion=beta.7、tracking ready、daemon PID 686373、NRestarts=0、quick_check=ok。采集时 activeSessions=0，但已有恢复后的写入证据，不以单个活跃计数判断恢复失败。该项验证 suspend，不扩大为 hibernate、混合睡眠或所有硬件兼容性保证；挂起前无活跃网页，未单独验证活跃网页跨 suspend 的封口场景。

</details>

实机验收操作顺序：

1. **升级前基线与可恢复备份**：关闭不必要的写入操作，通过设置页导出一份已验证的结构化备份，并把它保存在当前 Patina 数据目录之外；记录现有包版本、数据库完整性和行数基线。没有可读取的备份不得进入安装步骤。
2. **安装后、首次切换前**：安装静态验证已通过的 `X.Y.Z-beta.N` DEB，立即确认 Desktop、`patinad` 和 unit 来自同一包；维护脚本不得启动或启用 unit，用户数据与旧 XDG autostart 仍存在。
3. **首次 owner 交接**：启动 Desktop，完成显式迁移与受控重启；确认 reservation 为 `completed`、lease owner 为 `daemon`、systemd service active，并且 capability 同时报告 daemon runtime、tracking ready 和 managed service ready。任一条件失败均保持 fail-closed，先使用设置页重试或回滚，不手工删除 owner 文件。
4. **常驻与 Linux 信号**：关闭 Desktop 后保持正常操作，确认 session 与网页活动继续增长；再验证 GNOME 锁屏/解锁、睡眠/恢复、Zen/Firefox 网页同步、音频参与和 MPRIS，检查每个边界都封口且统计不倒退、不重复。
5. **崩溃恢复与偏好对账**：记录 `ExecMainPID`/`NRestarts` 后只对固定 `patinad.service` 注入一次失败，确认 systemd 生成新 PID、API 恢复、仍只有 daemon lease；分别验证后台登录偏好开关与 unit enable 状态收敛。
6. **显式回滚与再次接管**：从设置页执行回滚，确认 service 停止、reservation 为 `rolled-back` 且 daemon lease 释放；随后使用显式重试重新完成 managed 状态。不得通过删除 lock/reservation 模拟成功。
7. **升级与卸载**：用后一 beta 覆盖前一 beta，确认包内 Desktop/daemon 协议一致、数据和设置保留；卸载包后确认包属文件消失而数据库、备份与 Token 留存且数据库仍通过 `quick_check`。完成后可重新安装当前候选包继续使用。

建议证据命令如下；每个 `--output` 必须使用尚不存在的绝对路径：

```bash
npm run release:inspect-installed-patinad -- --phase baseline --expected-version 1.8.3 --output /tmp/patina-before-beta.json
npm run release:inspect-installed-patinad -- --phase installed --expected-version "$BETA_VERSION" --output /tmp/patina-beta-installed.json
npm run release:inspect-installed-patinad -- --phase managed --expected-version "$BETA_VERSION" --output /tmp/patina-beta-managed.json
npm run release:inspect-installed-patinad -- --phase rolled-back --expected-version "$BETA_VERSION" --output /tmp/patina-beta-rolled-back.json
npm run release:inspect-installed-patinad -- --phase uninstalled --output /tmp/patina-beta-uninstalled.json
```

以上文件只证明采集时刻。关闭 UI 后持续记录、锁屏/睡眠、浏览器活动和崩溃重启仍需在动作前后各采集一次，并核对 PID、restart count、session/web row counts 与时间边界；单份“最终正常”快照不能替代中断过程证据。

2H.3d.3d 的自动化证据矩阵：

| 故障或中断点 | 安全行为 | 自动化证据 |
| --- | --- | --- |
| `prepared` / `activating` / `completed` 后进程退出 | 重启后仍选择 daemon client，且只有前三种状态允许启动或确认服务 | `every_persisted_boundary_keeps_one_safe_startup_owner` |
| `failed` 或 `rolling-back` 后进程退出 | 保持 fail-closed；失败不隐式重试，回退中不启动 embedded | `every_persisted_boundary_keeps_one_safe_startup_owner`、`failed_cutover_never_falls_back_to_embedded_or_retries_implicitly` |
| `rolled-back` 已提交 | 只有该最终状态恢复 embedded owner | `rollback_keeps_the_client_owner_until_all_external_work_is_complete` |
| 桌面端重复启动或重复状态推进 | `prepare`、`mark_activating`、`mark_completed` 和 rollback 均幂等 | `prepare_is_idempotent_and_creates_an_owner_only_marker`、`activation_and_completion_are_durable_and_idempotent`、`rollback_from_failure_clears_failure_and_is_idempotent` |
| 旧进程携带过期 request ID | 不能激活、完成或标记新 reservation 失败 | `wrong_request_id_cannot_advance_the_reservation` |
| service 启动失败 | 持久化 `failed`，后续启动不回退 embedded | `every_persisted_boundary_keeps_one_safe_startup_owner` |
| daemon API 未 ready 或暂时不可达 | 在有界确认窗口内重试 | `cutover_confirmation_retries_startup_and_fails_fast_on_protocol_errors` |
| daemon 协议不兼容或 runtime host 错误 | 立即停止重试并持久化失败路径 | `cutover_confirmation_retries_startup_and_fails_fast_on_protocol_errors`、`only_permanent_negotiation_errors_abort_cutover_immediately` |
| 旧 Desktop 尚持有 RuntimeLease | 新 owner 等待；超时则拒绝继续，不抢占 lease | `release_barrier_waits_for_the_previous_owner_without_taking_ownership`、`release_barrier_times_out_while_an_owner_is_alive` |

切换约束：开发版和现有已安装稳定版继续默认 embedded；只有完成 reservation 的安装版才默认 daemon client。`--daemon-client-preview` 在 beta 验收期继续保留，显式 embedded 回滚只用于开发/故障恢复并必须经过 RuntimeLease。Stage 2H.3d.5 通过前不发布 daemon-backed stable，也不恢复 AppImage 发布。

## 4. 目标结构

迁移期共享运行时结构：

```text
platform/linux ─┐
data/sqlite ────┼─> engine runtime ─> RuntimeEventSink
domain ─────────┘          │
                           ├─ patinad host
                           ├─ Tauri desktop host
                           └─ tests

patinad
  ├─ tracking / watchdog
  ├─ SQLite runtime write side
  ├─ power / audio / MPRIS
  ├─ browser activity bridge
  └─ local API + event stream

Tauri desktop
  ├─ window / tray / WebView
  ├─ Dashboard / History / Settings
  ├─ desktop updater
  └─ daemon client

Browser UI
  ├─ Dashboard / History / Data
  ├─ Apps / diagnostics
  └─ localhost API + event stream client

TUI / CLI
  └─ daemon client
```

## 5. 核心边界

### 5.1 `RuntimeContext`

持有运行时真正需要的数据库 pool、设置访问、clock 和状态对象。engine handler 和 tracking 主链依赖 context，不依赖 `AppHandle`。

### 5.2 `RuntimeEventSink`

提供最小事件出口。Tauri host 把事件映射为 Tauri event；daemon host 把事件写入本地 event stream；测试使用内存 sink。

该边界只表达事件，不承接业务判断或序列化所有权。

### 5.3 `RuntimeLease`

按 Production / Local / Dev profile 建立唯一后台写侧 owner。同一 profile 中，desktop embedded runtime 和 `patinad` 不能同时启动 tracking。

获取失败时必须返回现有 owner 的可诊断信息，不通过竞争端口或 SQLite lock 间接判断所有权。

### 5.4 Storage bootstrap

把 profile、XDG roots、data/WebView anchor、pending migration 和 fail-closed 校验提取为不依赖 Tauri 的启动边界。desktop 与 daemon 必须调用同一个 resolver。

自定义数据目录不可用时，不允许在默认位置创建替代数据库。

### 5.5 API runtime context

API handler 依赖 pool、runtime snapshots、settings 和平台诊断 provider。HTTP transport 只负责请求限制、鉴权、路由和响应，不拥有业务数据。

desktop 与 daemon 复用同一 transport 和 endpoint registry，OpenAPI 从实际启用的 endpoint 集合生成或校验。

Stage 2F.2 已由 Axum + Tower 承接通用 HTTP API、SSE、浏览器 bridge、并发预算和优雅关闭，并删除两套自写 parser/server loop 与 SSE writer。普通 API、SSE 和浏览器 bridge 分别使用 32、8 和 8 的 fail-fast 并发预算。API 只接受 loopback Host，有 Origin 时只允许 loopback HTTP(S) 或 `tauri://localhost`；bridge 只接受 loopback Host，有 Origin 时只允许 `moz-extension://` 或 `chrome-extension://`。无 Origin 的本机 Bearer 客户端保持可用，两类 listener 继续使用独立 credential 和 origin policy。

### 5.6 Browser UI client

`patinad` 在 loopback 上提供静态浏览器 UI、HTTP API 和 event stream。第一版复用现有 React feature 与 read model，通过 browser runtime gateway 替换 Tauri IPC 和直接 SQLite 入口。

浏览器 UI 不直接打开数据库，也不获得 tray、任意文件选择、安装更新或窗口激活能力。浏览器使用 same-origin、HttpOnly、SameSite session，不获得长期 API Token；MCP、CLI 和 Agent 继续使用 owner-only Bearer Token。写侧能力开放前必须验证 CSRF、origin、loopback Host 和日志泄漏边界。

Tauri 当前继续作为桌面客户端。未来如果实测证明 GPUI 更适合 Linux，替换范围只限桌面客户端，不改变 daemon、浏览器 UI、TUI、MCP 或数据协议。

## 6. 分阶段实施

### 阶段 0：修正骨架

- 支持 Production / Local / Dev profile 隔离
- 复用 storage anchor 与 fail-closed 规则
- 建立 `RuntimeLease`
- 统一 HTTP transport
- 修正 OpenAPI 与路由能力声明
- 保持 tracking 由 desktop 拥有

验收：开发 daemon 不触碰生产库；自定义目录正确解析；第二个 owner 明确拒绝启动；daemon 不写 session。

### 阶段 1：共享运行时边界

状态：已完成。

- 引入最小 `RuntimeContext` 与 `RuntimeEventSink`
- 让 API handlers 摆脱 `AppHandle`
- 让 tracking 和 watchdog 的数据访问依赖共享 context
- desktop 继续作为默认 runtime host
- daemon 提供完整只读 API

验收：桌面行为无回归；daemon 只读 API 与现有 API 契约一致；两种 host 使用同一 handler。

### 阶段 2：daemon 接管后台

状态：Stage 2A 至 Stage 2H.2 的 preview 能力迁移、数据语义、loopback transport、daemon owner 收口和 systemd restart handoff 已完成；显式 Desktop client 的只读 runtime 和首批写侧 command 已接通，默认 owner 切换仍待实施。

- 已完成：有界事件中心、受认证 SSE、replay/resync、能力协商和干净关闭
- 已完成：显式模式下 daemon 接管 tracking/watchdog、实时快照、session 写入和退出封口
- 已完成：共享 logind power source 与 daemon lock/suspend/resume/shutdown owner
- 已完成：daemon 接管 Linux audio source，按设置启停并在退出时取消
- 已完成：daemon 接管 Linux MPRIS source，多播放器按当前窗口优先解析并在退出时取消
- 已完成：daemon 接管 browser activity bridge，共用鉴权、隐私、记录、事件和受限请求生命周期
- 已完成：通用 API/SSE 和浏览器 bridge 使用 Axum + Tower，具有独立并发预算、各自 Host/origin 边界、task readiness 和有界关闭
- 已完成：daemon owner 拆分，聚合 runtime 只保留装配和有序关闭
- 已完成：显式 `--daemon-client-preview` 通过 typed daemon client 和 event stream 获取 tracking 状态，且不启动第二套 tracker
- 已完成首批写侧转发：AFK threshold、tracking pause、audio participation、classification 与 Tools 通过 daemon API 执行
- 已完成：browser runtime、local API 配置和白名单内普通 app settings 由 Desktop Rust host 转发给 daemon；端口或 Token 变化会主动重建 client 与 event stream
- 已完成：Settings session cleanup 与历史窗口标题清除由 Rust data owner 事务执行；daemon-client 模式通过要求 `confirmed: true` 的 typed daemon API 执行，前端不再直接发删除 SQL
- 已完成：本地备份导出使用跨表 SQLite snapshot transaction 和 owner-only 原子文件发布；备份读取已限制 archive/entry/解压总量并拒绝符号链接与重复 ZIP entry
- 已完成：活动导入提交/列表/删除通过 owner-only 暂存票据和 typed client 迁移到 daemon，Desktop 不向 API 发送任意路径或大文件正文
- 已完成：本机定时备份由 daemon 唯一持有调度循环、配置与运行状态；Desktop typed client 可读取和显式确认完整配置，任务关闭会等待当前归档安全结束
- 已完成：按应用删除通过受确认的 daemon API 在单事务内清理原生和导入事实并返回计数；Desktop preview 不再直接打开 SQLite 写入
- 已完成：受控 backup restore 以及 remote backup 非密钥配置、Linux 系统凭据和显式上传的 daemon owner 迁移
- 已完成并验证：remote backup 列表、有界下载与受控恢复衔接；daemon client 不再把下载路径交回 Desktop

验收：关闭 UI 后继续记录；重开 UI 恢复当前状态；AFK、锁屏、睡眠、恢复和异常封口正确；统计不倒退、不重复。

### 阶段 2F.1：后台稳定化

该阶段不增加用户功能，先把 preview 能力收敛为可长期运行的服务边界：

- 已完成 data owner：网页 active row 按 `updated_at` 恢复并受启动时间上限保护，不把停机空白计入 duration
- 已完成 web activity engine：30 秒扩展心跳使用 75 秒 connected 宽限；15 秒 watchdog 在过期后按最后成功上报时间封口，并在 data owner 中防止并发新上报被旧检查误封
- 已完成 verification first：跨夜崩溃恢复、心跳抖动、扩展消失和数据库观测边界已有测试，再进入 transport 与 owner 结构修改
- 已完成 API transport：Axum + Tower 替换通用 API 的自写 HTTP parser、server loop 和 SSE transport；普通 API 和 SSE 分别使用 32/8 的 fail-fast 并发预算
- 已完成 platform transport：browser bridge 使用 Axum 并设置独立 8 请求并发上限
- 已完成 API boundary：API 使用严格 origin/CORS 与 loopback Host 校验；无 Origin 的 Bearer 客户端保持兼容
- 已完成 browser boundary：浏览器扩展保留独立 listener 和 Token，只回显 Firefox/Chromium 扩展 Origin，不复用通用 API 的 origin policy
- 已完成 transport health：通用 API listener/task readiness 已联动，意外退出会使 desktop 诊断降级或触发 daemon 受控停机；browser bridge 正常退出、panic 或 abort 均立即把 readiness 降级
- 已完成 daemon ownership：tracking、power、audio、media 和 web activity 生命周期已移入对应 owner 模块，`app/daemon/runtime.rs` 只保留编排和关闭顺序
- workspace boundary：首个 daemon-backed 里程碑保持当前 Rust package，不把 Cargo workspace 重排混入 owner 迁移
- verification complete：继续覆盖连接饱和、listener 意外退出和有序 shutdown

验收：daemon 停机不增长 session 或网页活动；短暂心跳抖动不误报断开；扩展消失后网页段不会无限增长；连接压力不会产生无界任务；readiness 与实际服务状态一致。

### 阶段 3：Linux 服务化与桌面客户端切换

- 已完成第一批写侧基础：capabilities 暴露服务版本、协议上下限和 write scopes；tracking owner daemon 开放事务化 app mapping、classification、AFK threshold 与 tracking pause，默认 daemon 仍严格只读
- 已完成 runtime settings owner：audio participation 可热切换；browser bridge 端口、Token、启停和 URL 隐私以完整配置原子应用，换端口失败时保留旧 listener 与旧存储
- 已完成：Tools runtime tick、启动恢复、Linux 通知、SSE 事件和 HTTP/MCP 写侧由 daemon owner 接管
- 已完成 Stage 2H.1：API listener 换端口使用预绑定/提交/切换，Token 文件原子轮换并撤销旧 bearer/SSE，HTTP/MCP 响应不返回密钥
- 已完成 Stage 2H.2：DEB 构建输入包含 daemon 与默认禁用的 user unit；受控 restart 使用跨实例持久化 ticket，手工 preview 不可误触发
- 已完成 Stage 2H.3a：通过用户会话 D-Bus 查询 systemd user service 真实状态，识别旧 desktop autostart 的迁移条件，并在默认 owner 切换前保持服务启用动作关闭
- 已完成 Stage 2H.3b.1：typed loopback client、Bearer 认证、runtime host 与协议协商，以及真实 API transport 回归测试
- 已完成 Stage 2H.3b.2：`/current`、active session 与标准 SSE parser 已接入只读 runtime adapter，具有 subscribe-before-read、cursor replay、resync 全量重读、有限重连和显式 shutdown
- 已完成 Stage 2H.3b.3：显式 preview 模式不运行 embedded tracker/API/browser/Tools，并复用现有 tracking commands 和前端事件；自动化与真实 GNOME 会话已验证唯一 daemon lease、完整状态读取，以及 desktop 退出后 daemon 持续追踪
- 已完成 Stage 2H.3c.1：受管 daemon client state 承接 AFK threshold、tracking pause、audio participation、classification 和全部 Tools command；Tools SSE 失效通知会重读完整 snapshot，embedded 模式保持原行为
- 已完成 Stage 2H.3c.2 核心设置迁移：browser/local API 配置与普通 app settings 通过 daemon API 写入；运行中换端口或轮换 Token 会通过共享 revision 主动重建 Desktop client、SSE、Tools 刷新和诊断请求。Desktop 会在发起首个资源写入前校验完整设置 patch；多个专用 runtime endpoint 之间不承诺跨资源事务，后续若开放非 UI 调用方，需升级为 daemon 侧统一批量命令或提供明确补偿语义
- 已完成 Stage 2H.3c.3：活动导入的文件选择/暂存归 Desktop，文件复核、数据库提交和批次删除归 daemon；暂存票据一次性消费并受路径、权限、大小和指纹约束
- 已完成 Stage 2H.3c.4：定时备份配置、调度 tick、运行状态、安全文件发布和保留策略归 daemon；Desktop 通过 typed client 读写，SSE 只发送失效通知，客户端重读完整 snapshot
- 已完成 Stage 2H.3c.5：按应用删除要求显式确认和有界 executable/range，请求通过 typed client 交给 daemon，在同一事务维护原生、导入事实和批次计数
- 已完成 Stage 2H.3c.6：受控恢复使用 owner-only 暂存、systemd restart、启动维护事务和 durable receipt
- 已完成 Stage 2H.3c.7a：WebDAV 非密钥设置走 daemon app-settings，Linux 密码走 profile-scoped Secret Service，显式上传由 daemon 从自己的 SQLite snapshot 执行
- 已实现 Stage 2H.3c.7b：remote backup 列表、ID 派生路径校验、有界下载与下载归档进入受控恢复状态机的衔接；完成门禁后 Stage 2H.3c 写侧收口
- 已完成 Stage 2H.3d.1：固定 unit 的受限 systemd 控制基础，以及 Production embedded 启动前的单 owner 防护
- 已完成 Stage 2H.3d.2 数据语义：后台追踪与桌面客户端登录偏好分键，旧值只作一次性缺省来源，host-owned 偏好不随备份覆盖目标机器；当前尚未开放日常设置 UI
- 已完成 Stage 2H.3d.3a-d：owner-only reservation、Production embedded 受控重启、旧 lease 释放屏障、managed client 激活、readiness 确认及全部持久状态的中断恢复自动化；真实 unit mutation 尚待 DEB 验收
- 已完成 Stage 2H.3d.4a：交接 reservation 与 fixed unit 状态合并为 Tauri 专属诊断，Settings 能明确显示 pending、failed、managed 和 managed-blocked
- 已完成 Stage 2H.3d.4b：本机确认式重试先验证状态、停止 daemon 并等待 lease，再以新 reservation 受控重启；`rolled-back` embedded 状态可通过同一确认入口预约重新接管，但不会在现有 embedded lease 释放前启动 daemon
- 已完成 Stage 2H.3d.4c 后端：completed reservation 持有后台登录意图，专用 Tauri command 串行应用 systemd 与 SQLite 镜像，启动路径负责中断后对账
- 已完成 Stage 2H.3d.4d 后端：`rolling-back → rolled-back` 保证 daemon 正常封口、lease 释放和 unit 禁用发生在 embedded 恢复之前，中断不会产生双 owner
- 已完成 Stage 2H.3d.4e：Quiet Pro 诊断区按状态开放后台登录偏好、显式重试和安全回滚，危险操作确认且执行期间禁止重复提交
- Stage 2H.3d.5a 成品静态验证和 2H.3d.5b DEB-only beta 发布契约已完成；当前只剩 2H.3d.5c 已安装包实机验收
- Tauri 改为 daemon desktop client，并保留 tray、通知、文件选择和 updater
- 默认切换后 desktop 不启动或自动回退 embedded tracker；daemon 不可用时明确暂停、诊断和重启
- 一个 `patina` 产品包同时交付 Patina Desktop、`patinad` 和 systemd user unit
- DEB 不在 `postinst` 全局 enable；首次桌面启动在用户会话中迁移并启用服务
- “后台追踪随登录启动”与“桌面客户端随登录打开”的持久化语义已拆分；启动时最小化只属于桌面客户端，首次交接会应用服务状态，日常修改待专用设置入口
- 首个 daemon-backed DEB 使用 beta 版本验证且只发布 DEB；AppImage 在解决 daemon 版本化解包与原子更新前不进入该发布
- embedded runtime 至少跨一个稳定版本保留为显式开发回滚路径

验收：登录后 daemon 可靠启动；关闭或退出 Tauri 后继续记录；重开 UI 恢复当前状态；服务崩溃由 systemd 恢复且不产生第二 owner；升级与卸载不误删用户数据。

### 阶段 4：独立仓库身份与稳定发布门槛

- daemon-backed beta 验收后，完整 monorepo 脱离 Windows 上游 fork network
- 保留 Git 历史、MIT 许可和 attribution，不拆分独立 `patinad` 仓库
- daemon-backed 稳定版前，单独验证 AppImage 的版本化 daemon extraction 与原子更新，或完成不破坏既有 updater 的退役迁移

验收：新仓库的 Actions、Release、Secrets 和 updater endpoint 可验证；现有 DEB 与 AppImage 用户都有明确且不会循环更新的迁移路径。

### 阶段 5：只读浏览器 UI

- `patinad` 提供 loopback browser UI、JSON API 和 event stream
- 第一版只覆盖 Dashboard、History、当前会话和诊断
- 复用现有 React feature，通过 transport-neutral browser gateway 获取数据
- 浏览器使用 same-origin HttpOnly session，长期 Bearer Token 不进入 JavaScript、URL 或浏览器存储

验收：浏览器 UI 不依赖 Tauri 即可回看数据；Tauri 与浏览器显示同一运行状态；恶意外部 Origin 无法读取 API；长期 Token 不进入浏览器历史、存储或普通日志。

### 阶段 6：受控写侧与新客户端

- 浏览器只读路径稳定后，再通过 CSRF 防护和操作确认逐步开放写侧
- MCP、CLI 和 Agent 保留 Bearer Token 认证，与浏览器 UI 共用业务契约但不共用凭据模型
- TUI 与可选 CLI

### 阶段 7：平台与客户端实现扩展

- KDE KWin provider
- 按 compositor 评估 wlroots provider
- 根据实测内存、启动、桌面集成和维护收益再独立评估 Tauri / GPUI
- 只有实测构建、二进制、资源或独立打包收益成立时才拆 Cargo workspace
- AppImage 只有在固定 service owner、版本化 daemon extraction 和 updater 原子切换得到独立验证后才恢复 daemon-backed 发布

## 7. 错误与安全策略

- storage anchor 损坏、挂载缺失或 schema 初始化失败时 fail-closed
- API 只监听 loopback；Bearer Token 文件保持 owner-only 权限并只供 MCP、CLI 和 Agent 使用
- browser UI 使用 same-origin HttpOnly session；API 拒绝任意外部 Origin，浏览器扩展使用独立 bridge credential
- daemon 正常停止前封口 active session；异常退出由下次启动检查 active row，但只能按最后可信观测时间封口，不能用下次启动时间填补停机空白
- 默认 owner 切换后，desktop 不自动启动 embedded tracker；服务故障必须可诊断并受控恢复
- desktop 与 daemon 版本不兼容时显示诊断，不静默使用不完整接口
- 迁移、清理、恢复和备份继续由 Rust owner 执行
- API、MCP、TUI 和 CLI 不获得任意路径删除或任意 SQL 能力
- 数据目录继续使用现有 `Patina` profile，不创建 `Patina Linux` 或 `patina_linux` 数据树

## 8. Windows 冻结与删除

`patinad` 稳定前：

- Windows 源码保留但冻结
- 不跟踪 Windows 上游功能
- 不新增 Windows CI、测试、发布或适配
- 不让 Windows 接口继续决定共享 runtime 的形状

`patinad` 稳定后：

- 以独立版本和独立执行计划删除 Windows cfg、依赖、源码和文档
- 先证明 Linux schema upgrade、release、updater 和构建不依赖被删路径
- 不把删除工作混入 tracking 正确性或数据迁移修复

## 9. 验证门槛

每一阶段至少覆盖：

- Production / Local / Dev 路径隔离
- 默认与自定义数据目录
- 缺失挂载、损坏锚点和数据库不可用
- 单实例、owner 竞争和端口冲突
- active session 的启动、切换、AFK、锁屏、睡眠、恢复和异常封口
- daemon 重启后的持续时间、重复记录和自愈
- UI 退出后继续记录及重新连接
- browser UI 与 Tauri desktop 的同源数据、event reconnect 和能力降级
- browser session、origin、CSRF 与长期 token 不落 URL
- API auth、请求限制、schema 与实际路由一致性
- daemon-backed `.deb` 与 systemd user service 的安装、升级、卸载和数据保留
- AppImage 的版本化 daemon extraction、固定 service owner 与 updater 原子切换独立验证
- `npm run check:full` 与 Linux release contract

阶段 0 和阶段 1 应优先使用 TDD 覆盖纯 context、path resolver、lease 和 endpoint registry，再进行真实 GNOME 环境手动验证。

## 10. 文档同步规则

- 能力承诺变化更新 `linux-platform-support.md`
- 实施顺序变化更新 `roadmap-and-prioritization.md`
- owner 或通道变化更新 `architecture.md`
- endpoint 行为变化同步更新 `api-index.md`、OpenAPI 和 `mcp-wrapper.md`
- 本设计完成使命后移入 `docs/archive/`，不长期保留为第二份架构母文档
