# 一套后端与多客户端复用阶段计划

状态：进行中。2026-10-03 用户授权创建专用 worktree／分支，按效果推进后端复用与多客户端同步，必要时重构模块。本文件管理本阶段交付，不把规划项表述为已实现。

当前授权推进边界：用户要求继续完成后端与整体架构基础，到需要讨论新 UI／多客户端产品交互时再讨论。M1b、核心读协议与现有 Desktop 内部适配可以继续实施；M3–M5 的新界面设计与完整客户端开发在进入前提交具体讨论方案，不把本轮基础完成等同于整个四端计划完成。

## 基线与开发边界

- 起点：Linux `main` 的 `f344c8a93b21d6ca8dac89adb56be96e7ce929c7`，公开稳定版 1.9.2；main 工作区干净。
- 分支：`feature/multi-client-platform`；工作目录：`.worktrees/multi-client`。有明确依赖和可独立验收的工作包时再细分，保持一条集成线。
- 授权覆盖计划、实现、重构与隔离验证；当前不安装开发产物到生产 profile，不启动第二个生产追踪 owner，不因开发授权自动推送、合并 main 或发布。
- 默认单 agent。Quiet Pro、本地优先、Linux/GNOME 和单 runtime owner 继续适用。R1 连续观察、Widget 暂停项、上游草稿、KDE/wlroots、Flatpak 和 Windows 删除不随本阶段恢复。

## 用户效果与最终验收

产品由一个 `patinad` 后端拥有业务规则与数据，Tauri、Web、TUI、GPUI 是不同表现形式的客户端。

1. 四端在同一查询范围下取得相同的产品事实和统计；分类、排除、导入优先级、活动时长和隐私裁剪不在各端分别实现。
2. 在任一支持写操作的客户端修改分类，其他在线客户端及时更新；离线重连、后端重启、事件缺口后恢复到一致状态，不重复执行写操作。
3. 关闭全部客户端后仍持续追踪；没有安装图形客户端也能安装、启动和诊断后端。
4. Tauri／Web 共享适用的 React 页面与交互；TUI／GPUI 使用共同 Rust 客户端库。每端保留窗口、导航、密度等本地偏好。
5. 后端新增兼容能力后旧客户端可继续使用已支持能力；不支持的能力明确不可用。新增界面仍需各端实现，不能宣称后端更新自动生成四套 UI。
6. UI 采用真实数据和异常状态验收：无数据、权限失效、连接中断、缓存过期、局部能力缺失均可区分。

完成证据必须包括多个真实客户端同时连接同一隔离 daemon 的录制／自动验收；SDK 测试、mock 服务或一端截图不等于四端交付完成。

## 阶段起点能力与缺口清单

| 场景 | 当前可复用能力 | 需要完成的边界 | 验收 |
| --- | --- | --- | --- |
| 连接和状态 | capabilities、受认证 HTTP/SSE、事件重放、resync-required、Desktop 重连适配 | 现有 `platform/daemon_client.rs` 依赖同一 Tauri 产品 crate 中的 API／domain 类型；提取独立客户端传输与协议，复用而非复制 | 无 Tauri/GTK/SQLite 依赖的客户端构建；Desktop 使用相同传输；断连／超限／协议不匹配回归 |
| 今天与历史 | summary、sessions、daily activity、daily apps；已有共享 Rust 聚合 | Dashboard、History 和详情仍经 `sessionReadRepository.ts` 查询 SQLite；TS 仍有聚合、导入优先级和分类规则 | 同范围一致，跨日／时区／活跃会话／导入重叠夹具；迁移后的调用方无 SQL fallback |
| 应用与网页详情 | 共享 destination owner、apps、web-activity | API 返回字段、分页、时间线和标题隐私尚需逐字段对照桌面需求 | 详情与汇总对应，小时导入不伪造精确活动；有界读取 |
| 分类及设置 | daemon 分类／app-settings 写操作和事务后事件 | 配置读快照、并发编辑策略、跨端缓存失效及客户端本地偏好分离 | A 写 B 更新，旧响应不覆盖新状态，失败不报告已保存 |
| Tools | daemon Tools owner、快照、命令、事件 | 通用客户端可消费类型；通知表现与业务到期行为分离 | 多端观察同一计时器，不重复触发业务操作或回放旧提醒 |
| 导入、备份、恢复 | daemon 预约、staging、快照、恢复状态机 | 文件选择和本机权限是宿主边界；Web 受控上传／下载能力需独立设计 | 沿用安全预约与确认；不开放任意路径或把 Token 交给网页 |
| 浏览器访问 | Axum、严格 Host/Origin、现有浏览器 session 设计规则 | 浏览器会话签发／撤销、CSRF、静态资源入口和 transport-neutral 前端尚未交付 | 同源受控登录，跨站／重绑定拒绝，无长期 Token 泄露 |
| 独立安装 | patinad 二进制、systemd user service、现有产品包 | 后端构建／依赖和安装包仍与 Desktop 产品交织 | 无 Desktop 的环境启动、升级、数据保留和诊断 |

HTTP API 索引和源码以当前实现为准；索引中历史的 unreleased/stage 标签不能单独作为缺少实现的证据。

## 架构决策

- **唯一业务 owner**：domain/data/engine 保有规则与事务；公开接口返回客户端需要的领域数据。按真实用例设计快照，不建立能任意读表／执行 SQL 的接口。
- **协议优先**：保留 HTTP JSON + SSE 和能力协商。优先从 Rust 协议类型／现有 OpenAPI 单一事实生成客户端契约，生成类型仍须有运行时边界校验；不同语言不得手写两套计时规则。
- **窄范围 crate 提取**：客户端必须不链接 Tauri、SQLx、tracking 和桌面库，这构成独立构建的实际需要。先提取传输／协议，Desktop 原有 typed facade 暂保留并调用提取后的实现；逐用例迁移 DTO。暂不整体重排 engine/data/workspace。
- **同步职责**：事务完成后通知，客户端通过快照恢复真相；事件序号不能冒充数据库 revision。明确快照与订阅竞态、daemon 实例更换、重放缺口、配置切换的旧响应屏蔽。写命令不因网络失败自动重试；需要重试的长任务复用 ticket／幂等键。
- **前端边界**：feature 不判断浏览器/Tauri；组合根注入外部能力。Tauri 的凭据留在 Rust 宿主，Web 使用同源 HttpOnly/SameSite session 与 CSRF 防护。
- **客户端状态**：共享产品偏好归后端；窗口位置、当前页面、终端配色等留在客户端。缓存可重建、带新鲜度语义；不得在后端断连时启动本地 tracker。
- **替代模块准入**：现有模块有耦合、正确性或可测性障碍时才替换；保留迁移对照和旧路径退出条件，避免形成长期双实现。

## 工作包与顺序

| 工作包 | 内容 | 可交付效果与退出条件 | 状态 |
| --- | --- | --- | --- |
| M0 | 工作区、缺口表、owner 决策、阶段计划与长期规则 | 新会话能准确继续；稳定 main 不受开发影响 | 已完成 |
| M1 | 独立 Rust 传输／协议基础，Desktop 接入；同步契约和 SDK 回归 | 第二个非 Tauri 进程可使用相同连接基础；请求、错误和 SSE 帧只有一份传输实现；通用重连／快照协调从宿主提取 | M1a、M1b 已实现并验证 |
| M2 | 以“今天 → 应用／网页详情 → 历史”为切片，补最小读 API，迁移 Tauri；统一产品配置读取 | Tauri 作为标准客户端完成核心链路，统计规则由后端负责 | M2a–M2e、M2g 已完成；网页、分类清理枚举与普通设置仍待迁移 |
| M3 | 浏览器会话和适配层；共享 React 核心界面，复用 Quiet Pro | Tauri＋Web 并行读取／修改分类并同步，真实浏览器验收 | 待实施 |
| M4 | Rust SDK typed 能力逐步补全，TUI 接入同一核心链路 | 实际交互式 TUI 可查看／筛选／修改分类，并参与同步；CLI 示例不算完成 | 待实施 |
| M5 | GPUI 客户端，同一能力和同步契约，独立视图 | 可运行 GPUI 核心链路和四端同步验收；评估启动、资源和维护成本 | 待实施 |
| M6 | 后端独立安装、兼容矩阵、异常恢复、发布集成 | 无 Desktop 后端运行、四端适用覆盖矩阵、可重复回归；范围冻结后再准备候选 | 待实施 |

M1 的第二客户端示例用于证明独立依赖和真实连接，不能提前宣称 TUI 或 GPUI 已交付。M3 优先于完整 TUI／GPUI 开发，以先取得共享 UI 的直接收益。只有出现真实并行需要才增加工作分支。

## 验证矩阵

- 协议：错误 envelope、协议版本、capability 缺失、未知事件、错误事件 ID、响应上限、认证失效、禁止重定向、固定 loopback；凭据不得进入日志。
- 同步：先订阅后快照、提交与快照交错、两端写入策略、断网重连、事件缺口、daemon 实例切换、停止后的迟到响应和提醒不重复。
- 读模型：相同范围与时区的一致结果，边界时刻、DST、排除、分类变更、精确／小时导入优先级和隐私。
- UI：正常／空／加载／错误／过期／局部不可用；Tauri 与真实浏览器走实际适配器；TUI／GPUI 分别实跑。
- 工程：前端交付 `npm run check`；架构／Rust 变更 `npm run check:full`，另跑独立客户端 crate 测试与无 Tauri 依赖检查。不把宿主操作放入普通测试。
- 安装与生命周期：只使用隔离 profile／环境；生产安装及发布另按明确授权执行。暂停的长时间观察不自动恢复。

## 执行记录

- 2026-10-03：从 f344c8a9 创建专用 worktree 与分支；核对已实现 API/SSE、Rust 客户端依赖和 Desktop SQLite 读取，记录 M0 与 M1 范围。尚无新客户端可交付版本。

### M1a：独立客户端基础

- `crates/patina-protocol` 为服务端／客户端共享的 capabilities、协议版本和响应／错误 envelope 来源，仅依赖 serde；`crates/patina-client` 拥有固定 loopback HTTP、能力协商、超时／响应上限和 SSE 帧读取，不依赖 Tauri、GTK、SQLx 或追踪模块。
- 原 `platform/daemon_client.rs` 保留业务 typed 方法和领域事件映射，转用独立传输；通用重连、快照协调、全部业务 DTO 和 TS 契约生成尚未迁移，不把传输提取称为完整 SDK。
- 请求不使用环境代理、不跟随重定向、不自动重试写命令。SSE 在完整事件组装前限制未结束帧大小，防止分片流持续增长；领域事件 ID／类型校验仍复用 Desktop 原适配。
- 新增 SDK 11 项测试，另有真实 loopback API＋合成 SQLite 的双客户端集成回归：独立客户端提交分类，Desktop facade 与另一客户端收到同一事件，重读结果一致，重连后可重放遗漏事件。
- `inspect` 是显式连接的 SDK 探针，不是交互式 TUI。`scripts/acceptance/independent-client.py` 已用准确 `/usr/bin/patinad` 1.9.2 二进制和两个独立探针进程通过隔离 Local profile 验收；临时 XDG 路径、不连接宿主 D-Bus、禁用浏览器桥接和音频，完成后子进程退出。该证据不证明硬件追踪或任何新 GUI 已验收。
- CI 新增独立客户端 job，无桌面依赖安装步骤；实际依赖图检查防止 SDK 引入产品、SQLite 或 UI crate。工作流当前仅本地修改，尚未推送或远端执行。
- 初轮 `check:full` 的前端与 SDK 部分通过；Rust 测试发现提取后的测试专用 `ApiResponse` import 缺失，已修复。协议单独提取后复跑最终 Rust 全量门禁，不重复无变化的前端构建。
- 最终门禁组成均通过：56 个 TypeScript 测试文件、38 项浏览器 UI 检查、生产构建和 bundle 检查；独立 SDK 11 项测试、依赖图边界与 Clippy；产品 Rust 721 passed / 21 ignored、cargo check、边界检查及 Clippy。没有新增已交付 UI、包版本或生产安装。日志与不含凭据的进程验收结果在本 worktree 的 `tmp/acceptance/multi-client-m1/`（gitignored）。

下一执行入口：M1b 已完成，继续 M2 的今天／详情／历史读接口缺口与产品配置读取。先以真实客户端调用证明必要抽象，不创建无人使用的通用状态框架。

### M1b 执行设计

- 提取共享运行事件及提醒 wire types，保留 domain／engine 的既有导出入口；SDK 统一校验 SSE 名称、序号与 envelope，服务端复用相同类型。
- 共享 SDK 拥有连接配置版本、取消、退避、先订阅后快照、事件游标与重放缺口处理；Desktop 仅提供具体快照读取和原生输出。每次发布必须同时校验 stop 状态和配置版本，不能只在 select 外层检查。
- SSE 响应增加可选的事件流实例标识，区分 daemon 重启后的新序号空间。新客户端重连遇到实例变化或旧服务端无标识时保守重读，不跨未知实例重放提醒；旧客户端与已有 JSON 字段保持兼容。
- 首个真实调用方是现有 Desktop adapter；独立 SDK 测试验证在途协商／订阅／快照的取消、配置切换、缺口恢复、快照期间的事件与提醒去重，不引入新 UI。

### M1b 核验结果

- 共享 `SnapshotSession` 已成为 Desktop adapter 和独立 `inspect --watch` 的实际连接 owner。宿主只注入快照 reader 和输出；Tools／原生提醒投递保留原 owner。连接与配置 revision 原子替换，重连即清空旧运行状态。
- RuntimeEvent、ToolAlert 与 envelope 共用 `patina-protocol` 定义；SDK 校验 event 名称和序号。服务端 SSE 返回稳定 hub 实例标识，真实 HTTP 双客户端测试确认两端读取一致；新 hub 不复用旧标识。
- 重连／缺口会通知现有 Desktop 的 tracking、Tools、定时备份与设置 owner，并使旧 Dashboard／History／Data 缓存失效。没有伪造 daemon 游标或回放提醒来补状态。
- 完整 `check:full` 通过：56 个 TypeScript 文件、38 项浏览器回归、构建／bundle、24 项 SDK 测试、依赖图／Clippy、722 Rust passed / 21 ignored 及产品 Clippy。后续只追加重跑实际变更的 SDK 探针与真实协议测试，未重复无变化前端检查。
- 两个独立 SDK 进程使用共享协调器连接准确的已安装 1.9.2 二进制，运行于全新隔离 Local profile，共同观察分类事件；旧服务端无实例标识的兼容路径通过。该过程不读取生产数据、不修改生产设置、不安装候选。
- 证据位于 `tmp/acceptance/multi-client-m1b/`（gitignored），包含完整门禁、SDK 最终门禁、真实后端契约及独立进程结果。本批没有新 UI、公开发布、main 合并或推送。

### M2 起点复核

- Rust 已有通用导入优先级解析和有界 daily activity／daily apps 查询；应复用，不再增加第二份统计事实表。
- Dashboard／History／destination 仍读取 `sessionReadRepository.ts`、`webActivityRepository.ts`；`sessionReadCompiler.ts` 仍承担活动归一化、合并、标题和部分显示语义。服务端 `/sessions` 只返回已封口原生记录，不能直接冒充完整客户端历史接口。
- 产品配置读取、业务 DTO 与精确历史／聚合读契约需继续补齐。现有 Summary 的导入来源分类与 Desktop 手动分类、live 截止等语义必须逐项对照，不能仅把 SQL 搬到 HTTP 就宣称四端统计一致。
- 后端独立构建／安装仍未完成；后续基础收口应包含它的依赖核对与适用实现，不以 SDK 独立构建替代。新 UI 实施前再讨论具体客户端功能与交互范围。

### M2a 执行设计：分类配置读取与条件提交

- 配置读取 owner 为 `data/repositories/classification_settings`，提供固定分类命名空间的完整、有界、一致快照。`GET /api/v1/settings/classification` 与 Desktop command 共用此实现，不开放任意 key／SQL 查询，不返回 API／桥接／远端密码等配置。
- 先保留现有分类配置 key/value 的兼容表示，避免同时更改分类规则；这是明确的配置契约，不代表其解析、历史统计与展示业务已经全部统一。规范化产品读模型继续在 M2 后续处理。
- 快照 revision 为按确定顺序、长度分隔的配置字节计算的内容摘要，不是事件序号。`POST /api/v1/settings/classification/conditional` 必须携带 `expected_revision`，在取得 SQLite 写锁后的同一事务中比较，不匹配返回 409；旧写入口仍兼容无条件提交。SDK 先检查 `classification-conditional` capability，再使用独立端点，防止旧 daemon 忽略新字段后无条件覆盖。客户端不得自动重试冲突写入。
- 现有 Desktop 的分类配置读取改走该 command／SDK，失败不回退 SQL；不缓存跨版本快照，不加入新的冲突交互。历史迁移仍通过现有显式写边界，GET 不修复、不迁移数据。
- API、共享 Rust 协议／SDK、现有 Desktop 适配和 OpenAPI 一起交付；用真实 API 验证双客户端冲突、事务不部分写入、订阅通知，以及敏感配置隔离和超限失败。

### M2a 核验结果

- 分类与网页域名配置读取已从前端 SQL 转为 owner 快照；固定命名空间、UTF-8 字节预算、JSON 转义后的响应预算及五秒超时均由后端执行。异常不返回部分配置。历史无效 key 只忽略，不在 GET 中修复或删除。
- 共享协议／SDK 已提供快照和条件提交，旧写接口保留。独立端点拒绝缺少前置 revision 的请求；冲突无写入、无变更事件。现有 Desktop 编辑交互仍使用原无条件写流程，不能宣称它已有冲突处理 UI。
- 真实 API＋SQLite 契约验证独立 SDK 与 Desktop 读到相同配置、成功提交后的事件、陈旧 revision 的 409，以及敏感配置不泄露。文件 WAL 数据库的双连接并发测试证明同一 revision 的不同编辑最多一个提交成功；结果超限则回滚。
- `check:full` 的前端／SDK 部分通过：57 个 TypeScript 测试文件、38 项浏览器回归、构建／bundle、27 项 SDK 测试及依赖图／Clippy。产品 Rust 初轮仅旧路由数量断言失败；补上新增 GET／条件 POST 断言后，完整 `check:rust` 通过：727 passed / 21 ignored、cargo check、边界检查和 Clippy。未重复已通过且未变化的前端门禁。
- 证据保存于本 worktree `tmp/acceptance/multi-client-m2a/`。本批未安装、打包、推送、合并 main 或发布。正在使用的 1.9.2 daemon 不具备新快照接口；开发验收使用新源码的隔离服务测试。

### 下一读取切片的约束

- 先统一后端产品分类语义：沿用 Desktop 的手动分类、禁用 override、已删除分类回落、别名与排除；旧 Summary 的导入来源分类不是 Desktop 已确认分类。需要共享夹具对照，不直接更改旧 API 的兼容语义后宣称迁移完成。
- 活跃会话的可信截止由后端给出。现有 TS 健康状态裁剪与 API sampled time 要对齐，验证断连／挂起／陈旧 heartbeat，不能将客户端墙钟当作持续记录证据。
- 新读接口应返回有界产品快照，事实、分类与排除从同一事务读取；复用已有 Rust 原生／精确导入／小时汇总优先级和 daily 聚合。History／详情只使用精确事实，标题和隐私单独预算。
- Desktop 接入每个切片后退出对应 SQL 与重复业务规则；未迁移的路径继续具名列出。独立后端构建／安装、普通设置同步和客户端本地偏好分离仍是基础阶段未完成项。

### M2b 执行设计：产品每日应用快照

- 新增独立的产品每日应用读契约，复用 `daily_activity` 的查询预算和原生／导入优先级；保留旧 daily-apps、Summary 端点的兼容语义。新快照在同一只读事务读取分类配置、排除、事实和应用身份，返回配置 revision 与最终分类。
- `domain` 拥有分类及排除规则，保留手动分类、禁用 override、内置 system 排除和删除分类回落顺序。用共享 JSON 夹具与现有 TS mapper 对照；名称本地化与颜色呈现仍由现有客户端负责，快照提供该事务中的名称 override。
- 现有 Data 趋势页消费新快照，退出该链路的前端重新分类／排除；缺少新接口明确报错，不降级为旧接口与本地配置拼接。无新界面或冲突交互。精确历史、健康截止、普通设置和独立后台安装继续保留为未完成项。

### M2b 核验结果

- 新增 `GET /api/v1/activity/daily-product` 与独立 SDK typed 读取；Desktop command 改用同一边界。配置 revision、最终分类、名称 override 与活动事实由同一 SQLite snapshot 产生。继续使用既有 daily 查询与导入优先级，不复制事实表或重新实现区间分配。
- `ProductClassification` 与旧 TS mapper 共享夹具，对照别名、手动分类、禁用／无效配置、系统进程、删除分类回落、百分号转义和 UTF-16 截断。`language` 只为保留旧空白自定义标签的归一化行为，不改变时间计算；分类写标准化和完整产品显示契约仍待统一。
- Data 应用／分类趋势直接消费最终分类和排除；新本地 mapper 不再改变旧快照的应用集合与分组。显示名称 override 同样来自快照，内置本地化、标签和颜色仍走表现层。持久化 `appReadVersion` 升为 2，避免旧图表绕过新读取。
- 实际 HTTP 测试中，独立 SDK 与 Desktop facade 返回相同每日数据与配置 revision。仓库测试验证排除的原生记录仍压制重叠导入，导入来源分类不冒充手动确认分类，响应不包含标题或凭据。旧 daily-apps 继续保持兼容。
- 最终 `npm run check:full` 全部通过：58 个 TypeScript 测试文件、38 项浏览器回归、生产构建／bundle、28 项 SDK 测试及依赖图／Clippy、729 Rust passed / 21 ignored、Rust 边界和 Clippy。初轮新测试夹具缺少生产索引及合法导入指纹，补齐后通过；未降低约束。
- 证据位于 `tmp/acceptance/multi-client-m2b/`。仅本地开发；未安装、推送、合并 main 或发布。该结果不是整个多客户端基础完成：新 UI、精确历史／网页读取、可信 live 截止、普通设置同步与独立后端构建／安装尚未完成。

### M2c 执行设计：后端活跃读取截止

- 先给产品每日快照增加显式健康与 cutoff 契约，读取 owner 持久化的 heartbeat，和事实处于同一 SQLite 事务。沿用现有 Desktop 的八秒 stale 边界；健康时允许读取到 sampled time，陈旧时最多计到最后 heartbeat，缺失／无效时不为开放会话推算时长。已封口和导入事实不受该 cutoff 影响。
- 该策略属于 domain，数据库获取属于 data；SDK／Desktop 只校验并消费，不按客户端墙钟重新推算。不把读取健康等同于采样来源可靠性或运行时 watchdog 修复。
- 用恢复 heartbeat、陈旧 heartbeat、无 heartbeat、未来时间及跨日夹具验证，不改变生产 runtime。随后复用此策略迁移 Dashboard／History，旧兼容 API 明确保留边界。

### M2c 核验结果

- 产品每日快照新增 `tracking_health`：健康状态、最后 heartbeat、live cutoff 与 stale 阈值来自后端。固定 heartbeat 与活动／分类共用读取事务；陈旧时长不随后续请求时钟增长。轻微未来 heartbeat 在八秒窗口内钳制到本次 sampled time，超过窗口视作无效，避免读取期间刚提交的时间戳被误判。
- SQL 只读取固定 heartbeat key，按字节限制持久值；无效／超长值不成为可信时间。`domain/activity_read_health` 独立拥有读取政策，SDK／JS 校验状态与数值关系，不自行延长时长。
- 新增跨日数据库回归和纯策略回归，覆盖停滞、重复读取、恢复、缺失／非法／过远未来值与已封口记录保留；实际双客户端契约继续通过。现有 Data 缓存版本升为 3，版本 2 缓存自动重读。
- 完整门禁通过：58 个 TypeScript 测试文件、38 项浏览器回归、28 项 SDK 测试及 Clippy、731 Rust passed / 21 ignored、产品 Clippy。缓存版本补充变更另完成针对性回归、TypeScript／生产构建和 bundle 检查；未因文档变更重复全量构建。
- 证据位于 `tmp/acceptance/multi-client-m2c/`。没有安装、推送、合并或发布。读取健康不保证前台 provider 质量；旧兼容 daily-apps／heatmap／Summary、Dashboard、精确 History 和网页仍有待迁移，不把该切片计作整体后端基础完成。

### M2d 执行设计：Dashboard 产品快照

- Dashboard 需要今天／昨天总量、应用分组和逐小时分类，不能用 daily totals 冒充完整替代。先统一领域层的分区分配：小时桶在完整查询范围内分配后，再按剩余容量分摊至显示小时，保留整数余数，避免各小时独立查询导致数量丢失。
- 后端复用有界产品读取与分类／健康 owner，在一个事务内取得本地日数据并生成 Dashboard 契约。小时汇总导入只提供数量，不伪造精确活动区间；原生和精确导入保留优先级及原生重叠规则。
- 日历边界归后端宿主时区，重复本地小时汇入同一显示小时，缺失小时为零；实际日长不得假定固定 24 小时。随后接入现有 Dashboard hook，退出对应 SQL、客户端 live 推算和重复分类，保留既有布局与交互。精确历史另用相同分类／健康策略。

### M2d 后端检查点（页面接入仍待完成）

- 新增 `/api/v1/activity/dashboard`、OpenAPI 与 SDK `dashboard(date, language)`。选定日和前一日、应用身份、配置 revision、可信 live 截止、24 个显示小时的分类数量来自同一次有界产品读取；复用原有日事实查询，不新增存储表。
- `summarize_activity_range` 与分区查询共用同一领域实现。先在父范围按优先级／容量分配，再把桶数量及整数余数分配到小时；一毫秒桶拆分不会消失。穷举小数量、竞争容量和切点证明各记录守恒且不超剩余容量，原有跨运行时夹具继续通过。
- 日历 owner 按 offset 转换切割实际时间，重复小时合并到同一个显示槽。三个独立进程分别使用 `America/New_York`、`Australia/Lord_Howe` 和 `Asia/Kolkata` 验证整小时 DST、半小时 DST、半小时时区的接口数量守恒；不修改宿主时区。
- 真实 HTTP 契约验证新 SDK Dashboard 总量与同日产品快照一致。SDK 拒绝小时遗漏、重复小时和分类错配；返回的小时数量不是精确活动时间线。
- `check:full` 通过：58 个 TypeScript 文件、38 项浏览器回归、736 Rust passed / 21 ignored 及 Clippy。新增 Dashboard 负向 SDK 测试后单独完成最终 `check:client`，共 29 项 SDK 测试及 Clippy 通过；未重复未变化的前端门禁。证据位于 `tmp/acceptance/multi-client-m2d-backend/`。
- **M2d 未完成**：现有 Desktop Dashboard hook 仍走旧 SQL／TS 编译链，下一执行项是使用新快照并删除该链路重复计算。随后继续精确历史／网页、普通设置同步、客户端本地偏好和独立后端构建／安装。多客户端高频读取还需复核共享查询 admission 的 busy 行为，不能仅以单请求测试替代并发使用验收。
- 本检查点仅本地提交，不安装、不推送、不合并 main、不发布，不启动新客户端 UI 工作。

### M2d Desktop 接入核验结果

- 现有 Dashboard 已使用 `cmd_get_dashboard_product` → SDK → 后端快照；生产路径删除会话汇总 SQL 与前端会话编译，不再按本地时钟增加活跃时长。名称本地化、颜色、百分比和图表显示取整仍是表现层；图标只读缓存保留为明确例外，失败不阻止活动数据。
- 前景按配置间隔重读后端；普通轮询不堆积请求，数据失效会丢弃旧结果后补读。读取 scope 含缓存代数／日期／语言，停止后不发布。预热与运行时协调均防止旧请求重新填充已失效缓存。
- 保留原布局与导航。首读失败显示不可用状态；已有快照刷新失败会明确标记旧数据并自动重试。真实浏览器自动检查验证了失败提示、保留原数据和恢复，未新增 TUI／GPUI／Web 客户端。
- 旧 Dashboard 编译器、候选映射和格式化只保留在 `tests/helpers/legacy*`，作为历史 replay／迁移对照，不作为当前 Dashboard 运行代码。新增适配器测试验证无活动 SQL fallback、契约拒绝、与旧稳定场景的呈现对照，以及新本地 mapper 不改变已确认数量。历史 replay 测试不等同于当前端到端安装验收。
- 完整门禁通过：59 个 TypeScript 测试文件、39 项浏览器检查、29 项 SDK 测试、736 Rust passed / 21 ignored，以及边界检查和 Clippy。首轮发现测试 oracle 引用路径遗漏；随后首屏预算检查发现验证器进入主包，改为读取时按需加载后恢复预算，未提高阈值。展示 benchmark 已改测后端数量的格式化，命令通过；该微基准不是整机性能验收。
- 证据位于 `tmp/acceptance/multi-client-m2d-desktop/`。没有安装、推送、合并或发布。下一入口为精确 History／详情及网页读取的后端归属；查询 admission 并发、设置同步／本地偏好、类型生成和独立后端构建／安装仍未完成，整体 goal 保持进行中。

### M2e 执行设计：精确历史与详情

- data owner 在同一事务读取分类、heartbeat、原生／精确导入事实与标题样本；复用已有优先级编译，不将小时桶放进时间线。开放记录止于可信 cutoff；事实与样本裁剪到请求区间，保留来源和可追溯 ID。
- 先查询紧凑候选，再批量读取实际贡献记录的元数据，避免标题进入全范围 UNION 排序。固定范围／记录／标题字节与时间预算，超限拒绝整个快照，不截断为成功或退回前端 SQL。
- 新读契约返回规范应用 key、最终分类、名称 override、配置 revision 和读取健康。标题仅为已存储的历史数据；record caption 不是带时间的标题采样。受认证本地客户端可读，不新增 MCP 暴露或浏览器授权。
- 后端与独立 SDK 先验证优先级、裁剪、开放记录、敏感配置隔离和预算，再迁移现有 History／应用详情入口，退出对应 SQL 和重复业务规则。前端标题显示／时间线合并的迁移须区分真实样本和显示用 caption，不把旧 fallback 当作精确事实。

### M2e 后端检查点（页面／详情接入待完成）

- 新增 `/api/v1/activity/history`、共享精确历史 DTO、SDK 与 OpenAPI。配置／健康／事实／标题在同一事务读取；后端输出源 ID、origin、规范应用 key、最终分类、名称 override、裁剪后的区间和真实标题样本。小时桶不会出现，caption 不变成假采样。
- 32 天范围、20,000 输入事实、40,000 输出片段、50,000 标题样本及 8 MiB 编码响应均有边界。字段限制使用 UTF-8 字节，响应计入 JSON 转义扩张。候选 UNION 只包含整数事实，标题按实际贡献 ID 分批读取；预算不足整批失败，GET 不改数据库。
- SQLite 测试覆盖原生压制精确导入、排除后仍压制、裁剪父记录与标题、开放记录冻结／恢复、无 heartbeat 不推算、原生／导入同 ID、敏感配置隔离，以及输入／标题数／Unicode 字段／转义响应预算。真实 HTTP 双客户端读取结果一致，重复查询参数被拒绝。
- 完整 `check:full` 通过：59 个 TypeScript 测试文件、39 项浏览器检查、30 项 SDK 测试及 Clippy、740 Rust passed / 21 ignored、产品边界和 Clippy。证据位于 `tmp/acceptance/multi-client-m2e-backend/`。没有安装、推送、合并或发布。
- **M2e 未完成**：现有 History／应用详情仍使用旧 SQL／TS 链。下一执行项是薄 Tauri command、严格前端 adapter 与已有视图接入；要处理旧 caption fallback、周视图读取量和刷新／缓存失效，不能把新 API 通过等同于页面已迁移。其后继续网页／图标、设置、并发 admission、契约生成和独立后端基础。


### M2e Desktop 接入设计补充

- History／应用详情改用严格精确历史 adapter，薄 command 经 SDK 访问既有 owner；不保留读取失败后的 SQL fallback。前端显示编译器显式区分后端确认事实与旧 replay 输入，名称 override、分类、排除和 live 边界以快照为准。
- History 的周趋势已不在页面上，删除其整周会话／标题读取及无用汇总计算；页面只取所选日。原生重叠不在详情归一化时被裁掉；caption 不转为采样，同标题采样空档保留。时钟回退也不能截短应用详情已确认的闭合记录。
- 将 Dashboard 已验证的请求协调器移到稳定 shared owner，History 和详情共同使用单请求／失效丢弃／停止保护。当前日轮询 owner 而非本地增长，历史日不持续轮询；读取失败会重试。History 缓存包含语言和失效代数，运行时等待初始化期间发生失效也不能重新填入旧缓存。
- 精确历史迁移使生产端 `nativeSessionPrecedence` 不再有消费者；移至 tests 作为旧契约 oracle，移除过时手工 bundle 分块，并增加生产代码不能引用 tests 的架构门禁。保留 bundle 原预算，禁止旧优先级 chunk 回归。
- 验证进行中，首轮前端 60 个测试文件／39 项浏览器检查通过；首次完整门禁停在已退出 chunk 的旧要求。最终门禁包含新增 History 失败恢复浏览器测试、真实 API 与 Desktop facade 对照，以及标题／重叠／失效缓存回归。后续状态以核验结果为准。


### M2e Desktop 接入核验结果（2026-10-04）

- History 和应用详情实际读取已切到 `cmd_get_exact_history` → 共享 SDK → 精确历史 owner；对应会话／标题 SQL 和生产端导入优先级计算已退出。范围／响应／标题预算、来源 ID、排序、健康和开放区间由 adapter 严格校验，无 SQL fallback。网页、图标与最早记录时间仍列为独立迁移，未宣称 Desktop 全部退出数据库读取。
- 已确认分类、名称 override 和过滤结果不随较新的本地 mapper 改变；开放事实断连后不增长。caption 只作标签，缺少样本时标题详情为空；同标题样本之间的真实空档保留。应用详情保留原生重叠的各自时长及独立标题，不因客户端时钟回退剪短确认事实。旧无 confirmed 的 replay 分支尚保留，待后续移出生产编译器。
- History／详情共用快照控制器；变更通知失效复用现存控制器，不随每次事件重建读取队列。浏览器刻意阻塞一次读取并连续发送三次 tracking 通知，证明只补读一次；停止前尚未执行的微任务也不产生新后端请求。当前日轮询、失败重试、旧响应丢弃和语言／代数缓存保护均已自动验证。
- 完整 `check:full` 通过：60 个 TypeScript 文件、40 项浏览器检查、30 项 SDK 测试及 Clippy、740 Rust passed / 21 ignored、产品边界与 Clippy。随后仅前端刷新并发发生变化，最终 `npm run check` 再通过全部 60 个文件／41 项浏览器检查，生产构建及 bundle 原预算通过（总 JS gzip 364.54 KiB）；Rust 未变化，未重复其门禁。
- 当前 History 表现层微基准以一个高量日的 700 条确认记录／2,800 个真实采样为输入，平均约 3.9 ms，保留原预算。该结果不代表后端查询、网络或整机性能。证据位于 `tmp/acceptance/multi-client-m2e-desktop/`，未安装、打包、推送、合并 main 或发布。
- 基础阶段继续进行：网页活动／详情、图标／最早时间、普通设置同步与客户端本地偏好、读取 admission／超时统一、契约类型生成和后端独立构建／安装尚未完成。还应将旧 replay 分支移出生产、核对 History 日历／小时展示在 DST 日的范围，避免把可复用后端已正确的日边界再次剪坏。新 TUI／GPUI／Web UI 仍在讨论边界之外，本次未开发。


### M2f 执行设计：网页产品读取

- 新建有界 `data/repositories/web_product` 读 owner；分类配置、URL 隐私模式、heartbeat、网页记录及其原生会话关联来自同一个只读事务。复用既有持久表，不增加第二份后端事实。先交付精确网页接口／独立 SDK，再做域名每日汇总和现有 Desktop 迁移。
- 当前 Data 的重复记录处理按 browser client／kind／exe／domain 去重；后端将身份元组显式建模，保留不同浏览器来源的独立贡献。先查紧凑 ID／时间，再按实际贡献候选读取有字节限制的元数据，范围、输入记录、单字段、整体内存、JSON 转义和查询时长均有预算。
- 开放网页记录必须同时受 owner heartbeat、最后网页上报和关联原生会话约束；沿用既有 75 秒浏览器宽限，超过后止于最后上报。无原生关联的旧开放记录最多读到最后持久观察，不因 GET 修复、重新打开或延长。已关闭记录保留事实边界，并服从已知父会话关闭边界。
- URL 过滤继续由后端执行。隐私枚举移到 serde-only 协议，现有 settings 重导出；旧 API 与新接口共用 domain 过滤函数，避免新 SDK 绕过 strip-query／domain-only。新精确 API 仍限已认证本地读取，不新增 MCP 工具或浏览器授权。
- 两项产品差异已向用户询问，答复前不切 Desktop：停止记录域名是否隐藏旧历史（现有 Data／History 不一致）；URL 设置是否统一限制所有客户端（现有说明只限制 API／AI，Desktop SQL 可见原文）。后端独立工作先继续，快照明确携带 recording_enabled、classification_revision 与 url_privacy，不以隐式默认决定页面行为。


### M2f 网页精确后端检查点

- 已交付 `/api/v1/activity/web-history`、共享 DTO／隐私枚举、独立 SDK、OpenAPI 和薄 Tauri command。三个认证表面使用相同 owner；真实 loopback 测试对照独立 SDK、第二个客户端和 Desktop facade 的记录与隐私模式切换，重复参数拒绝。
- 一个只读事务取得分类、隐私、heartbeat、紧凑网页事实和原生关联，再批量加载有限元数据。32 天范围、20,000 事实、8 MiB 原始元数据／编码输出和单字段上限均由后端执行，12 秒读取期限低于 HTTP 总期限；超限整批报错。domain-only 不读取原始 URL；旧 API 与新接口共用过滤政策。
- 测试覆盖同来源同域名重叠、独立来源和含分隔字符的身份元组、查询裁剪、上报过期／owner 停滞／恢复、无心跳不推算、GET 不封口，以及 Unicode 字段、数量与 JSON 转义预算。域名 recording_enabled 独立于显示元数据解析，错误的颜色或名称不会把已停止记录误报为启用。
- 首轮 `check:full` 通过：60 个 TypeScript 文件、41 项浏览器检查、31 项 SDK 测试／Clippy、744 Rust passed / 21 ignored 及 Clippy。补充 metadata／来源边界后，最终 SDK 门禁与产品 Rust 门禁通过（746 passed / 21 ignored）；新增测试夹具的原始字符串分隔符曾导致编译失败，修正后通过。最后 recording_enabled 防回归另通过全部六项 web_product 测试；对应最终源码 Clippy 已通过。前端未改动，未重复其门禁。
- 证据位于 `tmp/acceptance/multi-client-m2f-web-backend/`。**M2f 仍未完成**：域名每日聚合、候选统计和现有 Desktop 的 SQL／重复规则退出尚未实施；精确接口通过不能替代这些验收。两项产品讨论仍待答复，Desktop 网页行为保持原状。
- 仅本地开发，无安装、推送、合并 main 或发布。普通设置同步、图标／最早时间、全局读取 admission／超时统一、契约生成、旧 replay 退出及后端独立构建／安装继续在当前 goal 范围中；新多客户端 UI 不在此检查点交付范围。


### M2g 执行设计：图标读取边界

- 复核确认最早记录时间已由 heatmap 同事务返回，旧 `getEarliestSessionStartTime` 没有生产调用方；删除无用 SQL，不创建重复 endpoint。保留 Data 对原生／精确导入／小时桶最早时间的既有语义。
- 图标 owner 为现有 `data/repositories/icon_cache` 的有界读取子模块；只读持久缓存，不接受路径读取／平台图标提取。API 提供按原始 key 的 keyset 分页及单图标查找，canonical／原始／小写别名由后端统一提供。SDK、Desktop Dashboard 与 Widget 共同消费，断连不退回客户端 SQL。
- 图标是可重建表现缓存，分页不冒充跨页数据库原子快照；并发插入在下次刷新取得，不能与活动数量共享虚假的 revision。每页、单图标、客户端总页数／总字节均有限制，cursor 必须前进。Widget 查找只扫描有界名称并读取匹配图标，不把全量图片传给小窗口。
- 页面仍可在图标读取失败时显示无图标的已确认活动数据，这不构成活动数据 fallback。Widget 已暂停的产品扩展继续暂停，仅迁移原有读取边界。


### M2g 核验结果

- 图标 API／协议／SDK 及 Desktop／Widget 原有入口已接到同一后端缓存 owner。分页按原始 key 的 UTF-8 binary 顺序推进，正确处理特殊字符、Unicode、字节提前分页与 canonical 别名；单项查找只读有界名称和选定图片，不执行平台提取。删除未使用的全量 Widget icon-map command 及旧未限定仓库 getter。
- 最早记录时间旧 SQL getter 无生产调用方，已删除；现有 heatmap 返回仍是 Data 年份范围依据。`sessionReadRepository` 完全退出 SQL，只做精确历史适配。审计还发现分类清理的 `loadDistinctSessionExeNames` 是独立剩余读取，已在长期架构文档具名，不能宣称所有原生表读取已退出。
- 为避免图标分页拖慢业务，图标从 DashboardSnapshot 分离到真实的跨页面 app owner。共享图标前景每 30 秒重读，restore／resync 补读；后台停止和卸载会中止后续翻页，迟到响应不更新视图。异常不发布半份 map，不阻塞活动数据。原有 Widget 产品范围未扩展。
- `check:full` 通过：61 个 TypeScript 文件、41 项浏览器检查、32 项 SDK 测试／Clippy、748 Rust passed / 21 ignored 及 Clippy。初期类型检查发现 `.at` 超出当前 TS target，改成索引访问；只在测试使用的 Row import 已加 cfg。图标与活动解耦后最终 `npm run check` 再通过全部 61 个文件、42 项浏览器检查及构建／bundle 原预算（365.71 KiB 总 JS gzip）。Rust／SDK 未再改变，未重复门禁。
- 浏览器主动阻塞图标读取，证明已确认活动仍能显示；适配器测试验证无 SQL fallback、游标卡住／重复／乱序拒绝、后续页失败不返回部分结果、取消后不继续读取、128 页上限及 `__proto__` key 不污染对象。真实 HTTP 合约对照 SDK／Desktop 图标与 URL 编码。
- 证据位于 `tmp/acceptance/multi-client-m2g-icons/`。本批只做本地提交，不安装、推送、合并或发布。网页两项产品问题仍待答复；可独立推进分类清理枚举、普通设置与本地偏好、查询 admission、契约生成和独立后端构建，不把此切片等同于整体基础完成。
