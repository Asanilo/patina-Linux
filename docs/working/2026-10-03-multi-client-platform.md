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
| M2 | 以“今天 → 应用／网页详情 → 历史”为切片，补最小读 API，迁移 Tauri；统一产品配置读取 | Tauri 作为标准客户端完成核心链路，统计规则由后端负责 | 应用核心读取、设置／资源条件保存、读取隔离／期限与首批类型生成已完成；网页迁移、客户端偏好存储、剩余业务与契约退出待完成，按下方最新记录推进 |
| M3 | 浏览器会话和适配层；共享 React 核心界面，复用 Quiet Pro | Tauri＋Web 并行读取／修改分类并同步，真实浏览器验收 | 待实施 |
| M4 | Rust SDK typed 能力逐步补全，TUI 接入同一核心链路 | 实际交互式 TUI 可查看／筛选／修改分类，并参与同步；CLI 示例不算完成 | 待实施 |
| M5 | GPUI 客户端，同一能力和同步契约，独立视图 | 可运行 GPUI 核心链路和四端同步验收；评估启动、资源和维护成本 | 待实施 |
| M6 | 后端独立安装、兼容矩阵、异常恢复、发布集成 | 无 Desktop 后端运行、四端适用覆盖矩阵、可重复回归；范围冻结后再准备候选 | M6a–M6h 已有分层验证；激活、已知旧服务迁移和 Desktop 独立目标重载已有私有总线证据，独立版本／正式分包、旧包卸载归属、真实 systemd 登录及发布集成待完成 |

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


### M2h 执行设计：应用清理归属后端

- 审计显示 `loadDistinctSessionExeNames` 仅服务于分类页删除：前端先读原始名称，再做 canonical 分组并提交 raw-name 删除。此处迁移目标改为 owner 完整操作，而非新增与 `/apps` 重叠的全量读取端点。
- 新增明确 capability 的 canonical 清理契约，要求 app key、显式 all／today scope 和 confirmed=true；保留现有 UI 确认、错误反馈及旧 raw-name API。SDK 不自动重试写操作，旧后端不得回退为本地枚举／SQL 删除。
- 在 BEGIN IMMEDIATE 后有界读取三类事实的原始 executable 名称，用既有后端 canonical 政策匹配，并在同一事务内复用删除实现。today 使用后端本地日界，沿用“记录开始于当日”的现有语义，不改成跨日片段裁剪。名称与关联导入批次预算不足则整体回滚。
- 抽取共用事务内删除 helper，仅重算受此次删除影响的导入批次，避免顺便删除无关空批次；原生标题／网页关系沿既有外键策略清理。测试必须证明确认拒绝、别名集合、时间范围、三类事实、无关记录保留和中途错误回滚。


### M2h 核验结果

- 分类页删除改为显式确认的 canonical 后端命令，前端已删除原始名称 SQL、别名枚举和本地 today 日界计算。后端在 BEGIN IMMEDIATE 后识别并删除同一应用的三类事实；薄 command、认证 tracking API、协议及 SDK 共用这份 owner。旧 raw-name API 保留兼容，新客户端必须先识别 `canonical-app-cleanup` capability，失败不回退或重试。
- 共用事务删除 helper 只重算被触及的导入批次，修复删除某应用时顺带清理无关空批次的问题。测试验证大小写／helper／含空白别名、原生／精确导入／小时桶、关联 title／web link 的外键行为、独立网页事实与设置保留；中途 SQL trigger 失败会恢复此前所有删除。超过名称、字段或批次数量预算也不产生部分写入。
- 真实 API 验证 confirmed=false、未知字段、无效 scope 拒绝；Desktop facade 提交后，独立客户端通过 SSE 获知变更并读到清理后的事实，分类设置保持相同。SDK 对缺失 capability 只发协商请求，不尝试旧删除入口；前端验证一次性调用、today scope 转交和无 SQL fallback。
- `check:full` 的前端／SDK 部分通过：62 个 TypeScript 文件、42 项浏览器检查、34 项 SDK 测试及 Clippy、构建与 bundle 原预算通过。Rust 初轮唯一失败是新增测试错误假定 SSE envelope 的 payload 层级；改为反序列化共享 RuntimeEventEnvelope 后，最终完整 `check:rust` 通过（752 passed / 21 ignored）及 Clippy，未重复未变化的前端／SDK。
- 在独立进程分别设置 TZ=America/New_York 与 Australia/Lord_Howe，验证实际 25 小时／24.5 小时日的 today 清理边界；未改宿主时区。证据位于 `tmp/acceptance/multi-client-m2h-cleanup/`。所有删除测试仅用隔离 SQLite，没有操作生产历史、安装应用或远端仓库。
- 整体基础阶段仍未完成：网页两项产品决定、域名汇总／Desktop 网页迁移、普通设置与客户端偏好分离、查询 admission、契约生成、旧 replay 退出和独立后端构建仍在目标范围内。后续优先推进不依赖网页答复的设置／运行基础。


### M6a 执行设计：同源独立 daemon 构建

- daemon 已拥有 RuntimeContext、独立 tracker／Tools／备份和 API owner，但当前 Cargo 默认库仍强制编译 Tauri／GTK。优先保留同一个产品 crate 和六层源码，通过明确 desktop feature 隔离宿主包装，避免路径复用 shell 或第二份后端实现；不做全量 workspace 重排。
- SQL migration 定义应是框架无关的版本／描述／SQL 事实，Desktop plugin 与 SQLx 使用同一份原文，版本、checksum 和数据库兼容性不得因拆分变化。时钟归 runtime context；异步 core 任务使用 owner 的 Tokio runtime，桌面事件／窗口包装只在 desktop 编译。
- 默认 Desktop 构建和完整门禁保持可用；无桌面产物必须通过 normal/build 依赖图与 ELF 动态依赖核验，不能以没有创建窗口冒充无 GTK 依赖。验收只启动隔离 profile，验证存储、认证 API、运行／关闭与核心任务能力。独立安装与打包仍需后续实际证据，不把一次 cargo check 宣称为 M6 完成。
- 网页产品问题仍待讨论；本构建工作不切换网页行为，不安装或重启本机正式后台，不发布新客户端 UI。

### 暂停检查点（2026-10-04，用户准备关机）

- 按用户要求停止推进，保留可恢复现场。开发 worktree 为 `.worktrees/multi-client`，分支为 `feature/multi-client-platform`；最近已提交并完成相应门禁的检查点为 `526210b9`（M2h）。M6a 改动保留在工作区，尚未提交，不代表完整验收通过。
- M6a 已加入默认 desktop feature 与宿主适配隔离、框架无关 migration 定义、runtime context 时钟和 Tokio 调度调整；测试辅助 runtime 与 desktop-tests feature 也已调整，但其测试路径尚未验证。无桌面投影暂时允许共享定义的 dead_code，默认 Desktop 仍保留原有 lint；此例外尚需完整审查。
- 已通过 `CARGO_NET_OFFLINE=true CARGO_TARGET_DIR=/home/arinp22/code/patina/src-tauri/target cargo check --manifest-path src-tauri/Cargo.toml --no-default-features --bin patinad`，最终日志为 `tmp/acceptance/headless-check-7.log`，无编译警告；`git diff --check` 通过。此前 normal/build 依赖图检查未发现 Tauri／GTK 相关依赖，但还没有独立成品及 ELF 证据。
- 尚未完成：默认 Desktop 编译与完整门禁、无桌面测试编译／执行及 Clippy、旧数据库 migration 兼容回归、独立 binary／ELF 依赖核验和隔离 profile 启停验收。恢复时先审查 feature guards、时钟重导出与测试辅助路径，再完成上述验证；不要重复已完成的 M2h 工作或将本次 cargo check 当成可发布证明。
- 暂停前已检查宿主进程，无遗留 cargo check／test／build／clippy、rustc 或 npm run check 任务。未安装、重启生产服务、推送、合并或发布。网页两项产品决定仍待答复，恢复后也不据此默认改变行为。

### M6a 恢复与核验结果（2026-10-04）

- 用户返回并明确恢复开发。本检查点取代上面的暂停状态；M6a 已通过实现和自动验收。`desktop` 为默认 feature，纯后端关闭该 feature；Tauri mock 支持放入显式 `desktop-tests`。同一 crate／同一份后端源码，无新 UI、无另一套运行时业务实现。
- 迁移定义脱离 Tauri plugin 类型，SQL 原文、版本及描述与 `526210b9` 逐字节对照一致；SQLx 仍使用原有 migration 类型和 checksum 算法。时钟移动到 runtime context，清除了生产和专项验收中的旧路径；后台 icon／备份 worker 使用 Tokio，daemon 启动时进入自身 runtime 以支持同步 session probe。
- `check:full` 的前端与 SDK 部分通过：62 个 TypeScript 文件、42 项浏览器检查、34 项 SDK 测试／Clippy、构建与 bundle 门禁。初轮 Desktop 测试编译暴露专项验收中残留的时钟引用；修正后最终完整 `check:rust` 通过，752 passed / 21 ignored，默认构建、边界和 Clippy 通过。未重复未变化的前端／SDK 门禁。
- 新 `check:daemon` 已纳入 `check:full`，验证 normal/build 依赖图、无桌面测试及 all-targets Clippy；最终 595 passed / 10 ignored，Clippy 无警告。首轮找出了一个需要 Desktop 的 AppImage 专项测试依赖及七条测试 Clippy 告警，均已修正；仅为真实 Desktop 专项测试加 feature 条件，没有隐藏核心后端测试。
- 实际构建的独立 `patinad` 已保存并核对 ELF 和动态依赖，未链接 GTK／WebKit／GLib；仍需要 XCB、X11、PulseAudio 等 Linux 系统库，不能称为静态单文件。二进制 SHA256 为 `20c604654e2596ee1de404aa47ecf0e2833607c7b2d9992395e81c095f39135a`。在无宿主显示／D-Bus／音频连接的新 Local profile 下，两个独立 SDK 进程观察同一提交事件；未认证读取拒绝，SIGINT 正常退出，再次启动取得 lease，分类设置和 migration checksum 保留，SQLite integrity 为 ok。
- 证据：`tmp/acceptance/multi-client-m6a-headless/` 保存二进制与依赖／迁移对照；各项门禁见 `tmp/acceptance/m6a-*.log`；隔离运行证据在 `/tmp/patina-independent-client-2td9q_7y/`。没有触碰生产 profile、安装、推送、合并或发布。图形硬件追踪、独立安装包及跨客户端 UI 不属于此次自动验收结论。
- 整体阶段继续：普通共享设置／客户端偏好分离、网页产品决定及迁移、查询 admission／超时、契约生成和旧 replay 退出仍待完成。M6 的独立安装、兼容和发布集成也未完成；不得把本切片标成整个多客户端基础已交付。

### M2i 执行设计：共享设置读取与事件

- 新建 product settings 有界快照：同一只读事务返回生效的追踪／时间连续性规则、最短展示片段、暂停、audio／browser 配置及 owner 健康时间。只返回 browser token 是否存在，不返回凭据、界面偏好、远程备份账号或任意 settings key。revision 只描述设置内容，心跳变化不使其失效。
- 默认 idle／continuity 值以追踪 owner 的 900／180 秒为准，消除旧 API 的 180／30 秒默认值差异。现有用户存储的数字仍按 owner 解析，Desktop 不再用滑块范围重新裁剪后端的实际值；最短展示片段继续沿用既有 60–600 秒、60 秒步长策略。
- Desktop 设置合成改为共享快照加明确白名单的客户端／本机配置；共享读取失败不能退回 SQL 或默认值。删除整表 settings 读取与 tracker timestamp SQL；远程备份暂保留具名四键读取，但读取不再自动写入路径规范化。物理客户端偏好存储及旧 app-settings 写接口兼容仍是后续工作，不能把本切片称为完全分离。
- 普通设置 API 在提交后发出刷新事件，Desktop 将明确的设置原因转成本地 app-settings 通知，不在每次采样时重读设置。后续需验证已有页面异步读取的失效顺序、跨端通知和断连恢复，避免较旧读取覆盖新的已提交状态。新 UI 与网页两项待讨论行为均不在此变更范围。

### M2i 核验结果

- 共享设置 snapshot／协议／SDK／OpenAPI／Desktop command 已落地。固定键及单值字节预算、只读事务和超时共同限制查询；修复超长 Unicode 被截断后先解码的问题，超限在返回字段内容前失败。测试证明无凭据泄露、GET 不修复数据、健康／主题／仅凭据轮换不改变配置 revision，以及暂停和策略变化改变 revision。
- Desktop 共享设置与健康时间不再读 SQL，整表 settings getter 已删除。只保留明确列出的 Desktop／本机凭据键及 WebDAV 四键；远端备份读取不再偷偷保存路径规范化。普通 app-settings 写接口仍有兼容用途，物理客户端偏好存储尚未迁移，不能据此宣称 Desktop 全部退出 SQLite。
- 追踪默认值收敛到 owner 的 900／180 秒；Desktop 不再在启动时把读取的 idle 阈值写回后端。已有设置控件显示后端实际阈值（非整分钟显示秒），操作范围保持原状，外部设置的 60 秒不会被展示成 5 分钟或在无关保存时改写。
- 普通 API 提交后通知其他客户端，空批次和触发器导致的事务失败不发变更事件。真实 HTTP 测试对照独立 SDK 与 Desktop facade 的 revision／内容及新旧 tracker endpoint，验证写后 SSE 和回滚后的状态。Desktop 转发设置原因，主窗口与设置页共用已有读取协调器，丢弃过期响应；设置页用上一快照区分编辑字段，刷新未编辑值并保留草稿，取消后返回最新后端值。同字段普通写仍无 CAS 保证，后续必须单独完成。
- 最终前端门禁通过 63 个 TypeScript 文件、43 项浏览器检查和原 bundle 预算；浏览器实际验证远端阈值更新、保留本地编辑及取消后的最新值。SDK 35 项测试与 Clippy 通过；最终 `check:rust` 为 756 passed / 21 ignored，`check:daemon` 为 599 passed / 10 ignored，均含对应依赖／边界和 Clippy。
- 初轮门禁发现 SDK 的 Clippy 风格告警、OpenAPI 大 JSON 宏的递归限制、新测试类型路径、Unicode 超限处理及旧默认值断言，均已修复。前端／SDK 的最终证据为 `tmp/acceptance/m2i-complete-gate.log`；随后仅 Rust 修正，最终证据为 `tmp/acceptance/m2i-rust-verified.log` 与 `tmp/acceptance/m2i-daemon-verified.log`，未重复未变化的前端／SDK。
- 本切片只做本地源码与提交，没有打包、安装、推送或合并 main。下一执行范围仍为客户端偏好物理归属、普通配置条件写入、网页决定与迁移、读取 admission、契约生成及旧业务／replay 分支退出；独立安装与新客户端产品阶段仍不能视为完成。

### M2j 执行设计：普通追踪策略条件提交

- 普通策略（idle／continuity／最短展示时长／暂停）在 owner 取得 transition lock 和 SQLite writer 后检查读取时的 product revision，并原子提交。暂停沿用最后可信 probe 边界封口，不能绕开现有追踪恢复保护；idle 平台阈值只在事务成功后更新。独立 typed SDK 通过新 capability／endpoint 提交，不回退旧无条件写入，不自动重试冲突。
- 设置页显式携带读取基线，不能在保存前偷偷读取一个新 revision 来绕过冲突。普通策略混合其他设置时先检查／提交条件策略，再执行已验证的其余操作；不能把数据库事务宣称为系统资源的全局原子事务。资源操作失败要保留真实错误和草稿，后续根据新快照恢复。
- 复核发现设置保存 helper 已返回 save-failed，但页面将非 runtime-warning 全部提示为 saved；先修复此错误反馈并增加浏览器失败场景。其他客户端本地偏好的物理迁移保持后续范围，不因 CAS 交付宣称已完成。

### M2j 核验结果

- 新 capability／typed request／`POST /api/v1/settings/product/conditional`／SDK 已实现。普通策略先锁 tracking transition，再采样 owner 时间，在 SQLite writer 内比较最初读取的 revision 并原子写入。暂停封口复用既有事务 owner，失败保留 pending probe boundary；平台阈值及 pending seal 的确认只在提交成功后发生。不同 SQLite 连接同时提交同一旧 revision 时，只允许一个有实际变更的写者成功。
- Desktop 设置编排从 command 移到 `app/settings_commit`。设置页携带明确读取基线；新接口缺失、冲突或网络错误不回退旧写入口，也不重新 GET 后偷偷换 revision 重试。混合保存先验证配置并提交条件策略，冲突时主题／音频等其余操作完全不执行。后续资源操作失败仍可能已有策略提交，返回真实错误并重新读取；不能宣称存在跨资源全局事务。
- 草稿刷新不自动接受同字段冲突的新版本；不同字段正常合并。用户取消冲突编辑或将策略改回当前值后，即使仍有外观草稿，也会恢复最新策略基线。修复失败结果误提示 saved；确认后的 idle／audio 不再由前端重复写入，避免稍后的补写覆盖其他客户端的新值。保存期间的新编辑也按提交时草稿与当前草稿对照保留。
- 自动验收覆盖：无效字段／旧 revision 拒绝、两连接竞争、SQL seal 失败整体回滚、最后可信 probe 边界、等待 transition lock 后再取时间、SDK 缺少 capability 不 fallback／冲突只提交一次，以及真实 MockRuntime Desktop → HTTP owner 的混合保存拒绝、成功和后续资源失败。浏览器验证失败 toast、同字段冲突保留草稿、取消后重新编辑成功；这些不是新 Web 客户端或本机安装验收。
- 顺带修复 OpenAPI 写能力枚举遗漏既有 canonical／web cleanup 的问题，枚举现在从真实 API surface 合并生成；契约测试覆盖这些能力和新的条件写能力。
- 最终通过：63 个 TypeScript 文件、44 项浏览器检查与原 bundle 预算；36 项 SDK 测试及 Clippy；Desktop 762 passed / 21 ignored，独立后端 604 passed / 10 ignored，边界／依赖图及各自 Clippy 均通过。证据为 `tmp/acceptance/m2j-frontend-final.log`、`m2j-full-2.log` 的 SDK 部分、`m2j-rust-verified.log`、`m2j-daemon-verified.log`；原生混合场景另有 `m2j-native-mixed.log`。初轮修正了旧测试对可选元数据形状的要求、测试读取的 owner 边界和缺失 event hub 的原生夹具。
- 没有安装、打包、推送、合并或发布。后续仍需客户端偏好物理迁移、运行资源的并发保护、其余无条件便捷写入口的退出审计、网页产品决定／迁移、查询 admission／失败重读、契约生成和旧 replay／客户端业务退出。不能把 M2j 视为整个后端基础或 M2 已完成。

### M2k 执行设计：设置读取失败后的恢复

- 复核发现设置的事件触发读取若单次失败，当前控制器不会自动再读；需要另一条事件才能恢复。复用既有 `SnapshotReadController`，增加显式启用的有限退避（1／3／10 秒），成功后重置，耗尽后等待新失效或前台补读。默认不改变已有自带轮询的 Dashboard／History 读取行为。
- 主窗口设置和设置页启用该能力，并在窗口恢复焦点／可见时失效补读。新失效取消旧计时器，旧响应不发布；卸载移除监听与计时器。重试只用于只读快照，不用于任何保存／资源命令。
- 通过虚拟调度器验证重试次数上限、恢复、重新失效、scope 改变及迟到计时器；浏览器故障注入验证没有新设置事件时也能恢复，以及事件缺失后前台补读仍保留用户草稿和条件写入基线。

### M2k 核验结果

- 设置的有限读取恢复与前台补读已接入主窗口和设置页。控制器在成功、新失效和 scope 切换时正确处理预算；取消后的旧计时器即使迟到也不再启动请求。默认无重试的调用方继续使用既有轮询／事件策略，写入口未变化。
- 删除 runtime bootstrap 中已经没有消费者的 settings 读取，设置继续由独立 owner 加载；单次设置接口失败不再阻断后续 mapper／tracking bootstrap，也减少重复冷启动读取。原有 mapper 失败仍保留 tracking 观察的回归测试已更新。
- 最终 `npm run check` 通过：64 个 TypeScript 文件、45 项浏览器检查、类型／架构与构建／原 bundle 预算。虚拟计时器覆盖耗尽不自旋、恢复后重置、失效抢占、动态 scope 和卸载。浏览器真实触发失败后，仅解除故障、不发送新事件也能恢复；缺少事件时恢复焦点能取得新值。首轮唯一失败为故障注入产生的预期 console.error 尚未登记；最终只断言并移除该场景的精确合成错误前缀，其他 console.error 仍使验收失败。
- 证据：`tmp/acceptance/m2k-frontend-final.log`。本切片没有 Rust／协议／安装包变化，未重复已通过的 Rust 门禁，也没有安装、推送、合并或发布。整体基础阶段仍继续，客户端偏好物理存储、运行资源并发保护、后端读取 admission、网页迁移及契约生成等范围没有被缩减。

### M2l 执行设计：分析读取期限一致性

- 客户端偏好迁移发现产品选择：现有恢复会覆盖主题／语言等界面设置。已询问用户迁移到客户端本地后应保留当前偏好还是继续随数据备份恢复，答复前不改变恢复行为。
- 独立审计确认多条分析 repository 允许 30 秒，而统一 HTTP handler 15 秒先超时；原生 heatmap SDK 也只有 18 秒。将分析读取 query／HTTP／SDK 的 30／32／35 秒预算放在无运行时依赖的协议模块，普通 observed-apps 用 15／17／18 秒，精确网页保持 12／15／20 秒。通用 SDK GET 同样识别这些已知端点；显式调用者 timeout 覆盖仍由调用者负责，写操作期限不变。
- 保留范围／数量／字节限制，不通过缩短允许查询范围规避期限问题。增加真实 HTTP＋SDK 的 16 秒隔离连接阻塞验收，证明旧 15 秒边界不再提前终止有效查询；该慢测明确忽略于普通套件，在本切片单独执行。
- 本切片不等同于 admission 完成。单连接 SQLite pool 与不同分析族 semaphore 的相互影响、取消后的实际 SQLite 工作、写侧响应和跨查询共享容量仍需专项验证；不把 async timeout 宣称为 SQLite CPU 或总内存上限。

### M2l 核验结果

- 查询、HTTP 和 SDK 已使用同一份有界分析读取预算。daily／Dashboard／History／trend／heatmap 及 legacy migration 使用 30／32／35 秒；普通 observed-apps 和精确网页使用各自预算。通用 SDK GET 会按已知路径和解码后的 legacy scope 选择期限；显式 timeout 覆盖仍保留。普通元数据与写操作的既有期限未改变。
- `npm run check:full` 完整通过：64 个 TypeScript 文件、45 项浏览器检查、36 项 SDK 测试及 Clippy；Desktop 763 passed / 22 ignored，独立后端 605 passed / 11 ignored，均包含对应依赖／边界和 Clippy。新增加的 ignored 项是明确需要 16 秒的传输验收，不是略过失败测试。
- 随后单独执行该慢测并通过：在内存 SQLite 中持有唯一连接 16 秒，通用独立 SDK 经真实 HTTP 读取 exact history，在释放连接后成功取得结果，再由 typed SDK 正常读取。它会发现原来的 3 秒通用 SDK 或 15 秒 HTTP 早退，实际用时 16.01 秒。证据：`tmp/acceptance/m2l-full.log`、`tmp/acceptance/m2l-delayed-http.log`。没有读取用户 profile 或启动生产追踪。
- API 文档已移除“HTTP 在 15 秒先超时”等过时说明，开发文档记录慢测入口。本批没有安装、打包、推送、合并或发布。客户端偏好的备份选择尚待答复；后续可独立推进 SQLite 分析读取与写侧的隔离及 admission 验证，整体目标保持未完成。

### M2m 执行设计：daemon 分析读与写侧隔离

- 核实 SQLx 0.8.6 不默认启用 WAL，当前 daemon API 分析与追踪共享一个连接；持有该连接会阻止后续写入取得连接。由 `data/analytical_reads` 管理独立只读池，daemon 在拥有 runtime lease 的启动准备阶段确认 WAL，使用同一已存在的数据库路径；只读连接不能创建数据库或运行 migration。
- 每个 daemon read owner 有两条只读连接和共享的两项 admission；不同分析接口／listener clone 共用容量，不建立无界等待队列，容量不足返回 503。已有 repository 各自更严格的单查询限制暂保留。普通设置、追踪写入和短控制接口继续使用写侧连接，不被分析连接 checkout 占用。
- 为只读 SQLite VM 安装 progress callback：每次 checkout 重置 30 秒执行上限，关闭时中断活跃原生查询；调用方取消后仍最多占用既有两条连接，不靠 Rust future Drop 假定 SQL 已经停止。该机制不宣称能抢占任意 Rust CPU 工作或阻塞文件系统 IO。
- primary daemon 必须接入该 owner，并在停止 API 后、释放写池与 runtime lease 前关闭读池。embedded 迁移宿主和内存测试 context 仍保留具名的共享池兼容路径，不扩展第二套 backend。备份偏好选择未答复，相关恢复语义不变。

### M2m 核验结果

- daemon 启动、API context、listener clone 和关闭已接入独立分析读 owner。WAL 由 writer 确认；readonly／query-only 池不创建数据库、不执行迁移。新旧事实读取（包括 sessions／summary／apps／web-activity）均经分析池，AI 聚合的子读取继承限额。配置／控制接口保持 writer pool，原来的 repository 单查询限制仍保留。
- 隔离 SQLite 测试先复现共享唯一连接时写入无法取得连接，再证明持有只读事务时 writer 能在一秒门槛内提交，旧快照保持旧值、后续读取看到新值。验证只读写入／临时表拒绝、clone 共用两项容量、关闭 reader 不关闭 writer，以及原生 VM 截止、调用方取消后的连接重用和关闭中断。
- 实际 HTTP 验证两个分析 slot 满载时不同事实接口返回 503，同时配置读取与条件写入仍在一秒门槛内完成；释放后恢复读取。容量恢复使用无额外全局 family semaphore 的 apps 端点，避免测试彼此争抢 History 的单查询限制。
- `check:full` 通过：64 个 TypeScript 文件、45 项浏览器检查、36 项 SDK 测试及 Clippy；Desktop 768 passed / 22 ignored、独立后端 610 passed / 11 ignored，以及边界／依赖图和 Clippy。随后只调整上述测试的恢复端点，相关 analytical 测试再次通过（6 passed / 1 ignored），没有重复未变化的前端门禁。初轮修正了 SQLx hook 名称为 before_acquire。证据为 `tmp/acceptance/m2m-full.log` 和 `m2m-analytical-final.log`。
- 新 headless binary 经临时 Local profile 的双 SDK 同步、认证拒绝、正常关闭／重启、lease 重获、分类及 migration checksum 保留验收通过；实际数据库 journal mode 为 WAL。证据在 `tmp/acceptance/multi-client-m2m-read-isolation/`、`tmp/acceptance/m2m-independent-client.log` 和 `/tmp/patina-independent-client-889sov_t/`；二进制 SHA256 为 `af7806183ec1bfa7cb9b6a96ecb14505715dfd750e7a6a5bc72cb909856f7567`。
- 没有操作生产 profile、安装、推送、合并或发布。本切片不宣称 embedded 或所有后台备份任务都已经使用独立读池，也不代表实际硬件追踪／整机性能验收。客户端偏好备份选择及网页两项产品问题仍待答复；运行资源并发保护、契约生成和其余已列出的基础阶段工作继续保留。

### M2n 执行设计：从 Rust 生成客户端契约

- 先覆盖已有 Desktop 消费者的 product settings／classification／cached icons，共享 Rust DTO 为事实来源。`patina-protocol` 用显式 `typegen` feature 启用 ts-rs，普通 SDK／daemon 构建不启用生成依赖；不解析 Rust 源码文本猜测类型，不生成 UI。
- 生成结果放在 `src/platform/protocol`，仍由 persistence adapter 验证未知输入、构造符合生成类型的 wire 对象，再映射为前端模型。保留字节预算、安全整数、revision、枚举与游标校验；JSON 的 64 位整数映射为 number 不代表精度验证已经完成。
- 固定生成配置与锁文件，提供显式写入命令和只读过期检查，纳入完整门禁。普通前端构建使用已提交产物，不额外要求 Rust。此批不宣称全部协议或手写 OpenAPI 已迁移；后续契约按实际消费者扩展。

### M2n 核验结果

- 已生成 13 项 wire 类型，设置／分类／图标三个现有 adapter 实际消费生成类型，保留原有 unknown 输入验证和前端模型。可选字段和 nullable 字段依照 serde 属性生成；JSON number 映射由生成器显式配置，环境变量不能改变输出。源码与生成结果、生成器及独立 Cargo 锁文件一同保存。
- 新增 `generate:protocol`、只读 `check:protocol`，后者加入 `check:full`。正常 SDK／daemon 的已解析依赖图确认不包含 ts-rs，并设门禁防止后续误启用。该批没有改变 endpoint、serde 输出或数据库格式。
- 故障注入验证：临时增加一个必填 wire 字段后，只读检查拒绝过期产物且不修改文件；TypeScript 同时指出真实设置 adapter 漏填该字段。恢复原始文件后，即使设置冲突的 TS_RS 环境变量，生成比较仍通过。证据为 `tmp/acceptance/m2n-contract-negative.log`，临时字段已移除。
- 最终 `npm run check:full` 完整通过：64 个 TypeScript 文件、45 项浏览器检查、36 项 SDK 测试及 Clippy；Desktop 768 passed / 22 ignored，独立后端 610 passed / 11 ignored，以及生成器／边界／依赖图／Clippy 和原 bundle 预算。证据为 `tmp/acceptance/m2n-full.log`。本批未增加重复的运行时行为测试，也未重跑未变化的实机／打包验收。
- 本切片仅本地开发与提交；未安装、推送、合并或发布。剩余协议的生成覆盖、运行资源并发保护、客户端偏好归属及网页迁移等仍在基础阶段范围；待答复的产品选择和新客户端 UI 讨论边界保持不变。

### M2o 执行设计：资源变更生命周期与浏览器停止事务

- 当前资源变更直接受 HTTP future 生命周期影响；浏览器端口设置提交后仍会等待旧 listener 关闭，取消会留下已提交但尚未发布的资源状态。由 daemon 自己持有已接受的音频／浏览器配置／Local API 端口与凭据变更，一次只接受一项，忙时返回明确 conflict，不建立等待队列。调用方失联不终止已接受的变更，也不自动重试；daemon 关闭时先拒绝新资源变更并等待已接受操作完成，再关闭后台、listener 和 SQLite。
- 浏览器 disabled 设置与活动网页段封口放入同一事务；封口失败保留原配置和 listener，不再出现“已关闭但封口失败”后又被下一次启用覆盖的中间状态。浏览器请求的策略读取及事实写入与配置事务共用 ingress transition，避免已读旧 enabled 的请求在关闭事务之后重新写入；等待结束后才取配置变更时间。
- 本切片保护资源状态一致性与生命周期，不把串行化当成旧草稿 CAS。Desktop 当前浏览器完整配置提交及资源 revision／条件 patch 仍需继续收口；embedded 兼容路径也不在新的 daemon 操作 owner 内。新客户端 UI 与尚待答复的产品语义不变。

### M2o 核验结果

- 四类 daemon 资源变更已接入独立生命周期 owner；同一实例只接受一项在途操作，重叠或关闭状态返回 conflict。已接受操作由 daemon task 持有，调用方取消只丢弃等待结果；启动工厂将真实 control 交给 DaemonRuntime，关闭先 drain 再停止后台、listener 和数据库。测试验证取消后操作继续、并发拒绝、失败释放名额及关闭等待／拒绝新写入。
- 浏览器关闭设置及 active segment 封口同事务完成，配置事务与 ingress 策略读取／事实写入共用 transition，锁在提交后释放而不跨 listener shutdown。原先的第二次封口事务已移除；时间由 owner 在取得 transition 后采样。事务失败不改 listener、不发布设置或网页事件。
- 原生测试实际持有 SQLite 连接、提交后丢弃 caller future，再释放连接并 drain，确认存储配置与真实新 listener 均完成切换。SQL trigger 注入封口失败后，原设置、原 listener、未封口段及事件数保持原状；解除故障后关闭与封口一起成功。另验证配置提交期间 ingress 等待，随后读取已提交的 disabled 状态，不沿用旧策略。
- 完整门禁各项通过：64 个 TypeScript 文件、45 项浏览器检查、36 项 SDK 测试；Desktop 773 passed / 22 ignored，独立后端 615 passed / 11 ignored，以及生成器、架构／依赖图、Clippy 与 bundle 预算。初轮发现测试 schema 未带网页表、测试读 SQL 越过 owner 边界及旧便利 helper 仅剩测试消费者，均已修正；helper 明确限定为测试，生产传入 owner 时间。前端／SDK 未变化，没有重复它们已通过的检查。证据为 `tmp/acceptance/m2o-full.log` 的前端／SDK部分、`m2o-rust.log` 的 Desktop 测试、`m2o-clippy-final.log`、`m2o-daemon.log` 和 `m2o-runtime.log`。
- 新 headless 成品及两独立 SDK 进程通过临时 Local profile 验收：认证拒绝、共享分类事件、正常关闭、lease 重获、分类与 migration checksum 保留。成品 SHA256 为 `9cd5347062473b9b7801680463b18127163bfd5091d6b1a82bebfb207a911e26`，证据为 `tmp/acceptance/multi-client-m2o-resources/`、`tmp/acceptance/m2o-independent-client.log` 和 `/tmp/patina-independent-client-0st1vlou/`。未连接生产 profile 或实际桌面／音频采样环境。
- 未安装、推送、合并或发布。资源条件 patch／revision、Desktop 浏览器全量配置与 SQL 读取退出仍需继续；此批没有宣称旧草稿防覆盖、跨端点全局事务、崩溃／断电恢复或 embedded 全路径已经完成。

### M2p 执行设计：音频与浏览器资源条件写入

- 新增共享协议与独立 SDK 的 resource snapshot／conditional patch，局部浏览器字段由 daemon 合并，省略 token 不读取或回传凭据、不覆盖现有凭据。使用独立资源 revision；普通追踪策略／健康时间／客户端偏好不使它失效。运行资源操作继续使用 M2o 的 admission、取消后持有及关闭 drain。
- revision 由公开资源状态和持久化 generation 计算，不散列凭据。所有经 app settings data owner 的资源写入（包括旧完整替换和音频接口）在原事务中推进 generation，因而非空 Token 轮换也使旧 baseline 失效。GET 不初始化 metadata；空 patch 只检查 baseline，不推进版本或发布事件。
- daemon 先对照读取版本并校验合并后的配置；需要换端口时先预留 listener，再在 `BEGIN IMMEDIATE` 内复查 revision，并原子提交音频／浏览器设置、generation 与必要的网页封口。成功后完成运行资源发布与事件；绑定／比较／SQL 失败不部分写入。该原子性只覆盖此资源请求，不延伸到普通策略或系统服务命令。
- 此批先完成后端契约、SDK、生成类型、真实传输及事务验收。现有 Desktop 设置页仍需携带资源基线、处理草稿冲突并退出完整配置聚合／SQL 读取，不能把新 endpoint 存在当作 Desktop 已获得资源 CAS。旧接口保留兼容，不用于新 SDK fallback。

### M2p 核验结果

- 新增资源 GET／条件 POST、`runtime-settings-conditional` capability、共享 Rust DTO、独立 SDK 和生成 TypeScript；条件 POST 只向 tracking daemon 开放。服务端合并省略字段，并在 listener 预留后于 SQLite writer transaction 再次校验版本。音频／浏览器配置、generation 与必要封口同事务提交，成功后发布 live resource 与事件；空 patch 不写、不发事件。条件请求继承 M2o 的取消与 drain 语义。
- 旧资源接口及其他经 app settings owner 的资源写入同步推进 generation，非空凭据轮换也会使旧 revision 失效，返回值不含凭据或凭据哈希。复核发现备份保留当前凭据却可能导入旧 generation，已将 generation 归为同一组本机保留元数据；测试确认恢复旧 archive 后当前凭据和版本保留，旧 revision 仍被拒绝。此改动不涉及待讨论的界面偏好恢复语义。
- 验证包括：两独立 SQLite writer 竞争只允许一个版本提交；封口 trigger 失败整体回滚音频／浏览器／generation；实际独立 SDK → HTTP → daemon 合并端口／隐私 patch 时保留凭据；旧版本、旧接口凭据轮换后的旧版本及端口占用均拒绝且无部分写入；空 patch 无事件；SDK 缺少 capability 不 fallback、409 不重试、非法／超限响应拒绝。初轮修正测试模块路径，专项最终 4 项通过，证据 `tmp/acceptance/m2p-resource-final.log`。
- 首轮 `check:full` 全部通过，包含 64 个 TypeScript 文件、45 项浏览器检查、38 项 SDK 测试、生成器与全部边界／Clippy／bundle 门禁。随后仅补充恢复归属及其测试，相关 Rust 门禁再次通过：Desktop 777 passed / 22 ignored，独立后端 619 passed / 11 ignored。证据为 `tmp/acceptance/m2p-full.log`、`m2p-rust-final.log`、`m2p-daemon-final.log`；未重复未变化的前端与 SDK 检查。
- 隔离进程脚本已补资源 patch、仅凭据轮换后的旧版本拒绝及 generation 重启保留；浏览器／音频采集保持关闭。最终 headless 成品通过双 SDK 同步、认证拒绝、资源条件写入、正常关闭与重启／分类和 checksum 保留。SHA256 为 `a6a0c8a4cc7d9d1c62bed8a6a085f3b1cc6fd7ea411c0e460b5a5f1d537fc4b0`，证据为 `tmp/acceptance/multi-client-m2p-resources-final/`、`tmp/acceptance/m2p-independent-client-final.log` 及 `/tmp/patina-independent-client-kmr2o05t/`。中间构建证据保留，不能代替最终成品。
- 本批只在开发分支本地实现与提交，没有安装、推送、合并或发布。下一项是现有 Desktop 资源保存的实际迁移：读取／保存携带资源 baseline、保留同字段冲突、取消恢复最新值，以及退出浏览器完整配置／SQL 聚合。普通策略与资源之间仍不构成跨请求全局事务；基础阶段及新客户端讨论边界不变。

### M2q 执行设计：Desktop 设置页资源基线

- 设置页与全局读取取得资源 snapshot，由严格 adapter 验证后映射；资源 revision 随 bootstrap／草稿保存传递。同字段冲突保留原 baseline，不被刷新自动覆盖；取消或改回当前值后使用最新版本。缺少 baseline 或 daemon 读取错误不回退无条件写入。
- Desktop 的新保存编排按资源、普通策略、客户端偏好分流。纯读取 preflight 验证资源能力和原版本，普通策略继续按原始 product revision 提交，再用原始 resource revision 提交稀疏 patch，最后提交客户端偏好。preflight 或普通策略冲突不执行后续写入；后段失败如实报告并重读，不能宣称跨 endpoint 全局原子性。资源请求不经 Desktop SQL 聚合完整浏览器配置，确认后不再前端补写音频。
- 明确的 embedded 迁移宿主由 host 返回 null 标记，保留旧兼容保存路径；这个标记不能由命令缺失或读取失败推断。浏览器凭据的既有本机显示／复制读取例外仍保留，不把此切片称为全部 Desktop 退出 SQLite；不创建新客户端或改变 Quiet Pro 视觉。

### M2q 核验结果

- 全局设置与设置页已读取、验证 resource snapshot；bootstrap、保存 adapter 与 hook 显式传递资源版本。稀疏资源保存走新增薄 Tauri command 和 `app/settings_commit/resources`，省略浏览器 token 时不从 Desktop SQL 读取整份配置。preflight 检查资源 capability／原版本／参数和偏好限制，随后普通策略、资源、偏好按既定顺序执行；后端资源 CAS 仍使用最初版本。确认后的音频不由前端补写。
- 资源草稿与普通策略分别跟踪冲突。刷新未编辑字段并保留编辑内容，同字段冲突不接受新 baseline；取消后采用最新值与资源版本。缺少资源 baseline 时保存拒绝，daemon 读失败不会回退；host 明确的 null 才表示 embedded 兼容路径。凭据显示仍是具名的本机读取例外，旧兼容接口未删除。
- 新增严格解析／预算／安全整数／非法 enabled-token 组合／冲突草稿／不 fallback 检查。原生测试在不安装 Desktop SQLite pool 的 MockRuntime 中，通过真实 daemon HTTP 完成普通策略＋资源＋主题混合保存，确认只改端口时凭据保留；旧资源版本的 preflight 不改普通策略和主题。证据为 `tmp/acceptance/m2q-native.log`。
- `check:full` 通过：65 个 TypeScript 文件、原 45 项浏览器检查、38 项 SDK 测试、Desktop 778 passed / 22 ignored、独立后端 619 passed / 11 ignored，以及生成器／边界／Clippy／bundle 门禁。新增的浏览器资源场景随后单独执行，最终 46 项通过，验证冲突保留端口草稿、取消采用远端值、再次保存只包含端口 mutation。证据为 `tmp/acceptance/m2q-full.log`、`m2q-browser-final.log` 和 `m2q-settings-test.log`。首轮修正新单测的 window mock，以及浏览器场景插入定位；这不是实装或新客户端验收。
- 没有安装、打包、推送、合并或发布。普通策略与资源之间仍没有跨 endpoint 原子事务，mixed save 后段失败可能已有前段提交，页面会失败提示并重读。下一独立工作仍包括剩余客户端契约／读取与便捷写入口审计、网页产品决定和迁移、客户端偏好归属；新 TUI／GPUI／Web UI 继续在讨论边界之外。

### M2r 执行设计：History 日历显示边界

- 审计发现原生精确 History 读取已使用真实本地日界，但时间线 `getFullDayRange` 和页面传给网页显示的 selectedDayRange 仍将午夜加 24 小时。在 23／25 小时或半小时切换日会延伸到次日或漏掉当天末尾。复用既有 `getDayRange` 的 calendar-next-day 算法，保持 24 zoom 选项代表全天、其他 zoom 保持既有实际时长。
- 用 Node 独立测试进程中的十个时区／日期夹具，先复现旧实现失败，再验证实际日长、最后半小时保留、排除次日、末端 zoom／focus 和轴终点。包含 New York、Lord Howe、Troll、Chatham 的偏移切换与 Kathmandu／Singapore 基线，不改变主机时区。
- 另发现旧 History 小时投影仍用本地 setHours 分段，Troll 两小时回拨时会把重复小时错误归到相邻列；这应通过后端现有 `activity_calendar` 能力退出旧客户端计算，不在 TS 再复制一份后端时钟算法。`AppShell` 的最短展示时长快捷入口也仍先乐观更新再无条件写，后续需携带读取版本及失败恢复。此切片不把这些残留问题视为完成。

### M2r 核验结果

- History 时间线与页面使用同一真实本地日界，全天不再固定为 24 小时。十个时区／日期夹具通过，覆盖 22／23／23.5／24／24.5／25／26 小时日长，确认最后半小时显示完整、次日记录排除、末端缩放／focus 可到达真正午夜及轴末标签不变。旧实现首先在 New York 春季切换日失败，多出了一个小时，证据为 `tmp/acceptance/m2r-before.log`；修正后 `m2r-calendar.log` 通过。
- `npm run check` 通过：66 个 TypeScript 文件、46 项浏览器检查、类型／架构、构建及原 bundle 预算，证据 `tmp/acceptance/m2r-frontend.log`。没有 Rust／协议变化，未重复 Rust 或实装验收。没有安装、打包、推送、合并或发布。
- 独立 Node 子进程以 `TZ=Antarctica/Troll` 复核旧小时投影：2026-10-25 实际 26 小时；按真实分钟参考，01 和 02 点应各 120 分钟，现实现分别给出 60 和 180 分钟。此剩余差异明确保留，下一项应让 History 消费后端统一的小时投影，并同时核对详情读模型的相同调用方。最短展示时长快捷写入口与其他已列基础范围仍未完成。

### M2s 执行设计：History 小时投影归后端

- 保留旧精确历史协议，新增产品快照将同一批确认后的 precise records 与 24 个本地钟点的分类时长一起返回。小时投影只来自精确记录，不把小时导入量伪造成记录或混入 History；复用 `domain/activity_calendar` 的实际偏移边界，不在 TS 复制日历分段算法。
- 延续已有 32 天／事实／响应预算，小时分段另有步骤与分类数量上限；从记录的相同 revision／可信 cutoff 派生，不再补查配置或用客户端 now 延伸会话。SDK／Desktop 验证小时分类量与精确记录总量守恒，不能靠客户端重算时钟位置掩盖后端问题。
- History 的小时图消费新快照，只保留颜色、标签和分钟舍入；旧 TS 小时统计退出生产调用链，历史 replay 对照仅留在测试范围。查询错误或旧服务端缺少新接口时沿用已确认快照／重试状态，不回退旧统计。另核对详情的全天 viewport 是否也把 24 小时误作实际日长。

### M2s 核验结果

- 已实现 `/activity/history-product`、共享 DTO／独立 SDK、薄 Desktop command 与 History 接入。记录、配置和健康仍是一次精确读取，小时量由该批 records 派生；旧精确接口与详情读取继续保留。复用同一 `activity_calendar` 边界实现，保留 32 天／8 MiB 等限制，新增 4096 分类／100 万分段步骤限制。最终 History family admission 保持到投影和响应预算检查完成，不能在 SQL 结束后提前释放。异步 timeout 不宣称能抢占任意同步 CPU。
- 小时协议数量归为 `ActivityHour`／`ActivityCategoryTotal`，Dashboard 原 Rust 名称仅 re-export；TS 生成面扩展至 27 项类型。Dashboard 与 History 共用严格小时数量校验及纯显示格式化；History 不再在 TS 用 setHours 分段。旧统计仅保留为 tests/helpers 的历史 replay 对照，已确认 src 下无引用。未知服务能力、非法小时分配及数量不守恒均不 fallback。
- 详情默认全天 viewport 同样改用真实日长；十组前端日历夹具覆盖它与 History。共享小时图原先固定 60 分钟纵轴，会裁剪正确的重复小时数量，现保留 60 分钟基线并按确认最大量扩展。新增真实浏览器场景用 130 分钟确认量验证柱形在绘图区内，恢复普通数据后刻度回到 60；视觉 tokens、切换与布局未扩展为新设计。
- 后端测试覆盖部分小时／跨日守恒、分类和工作步骤上限，以及小时导入量不进入精确投影。六个独立 TZ 测试进程（New York、Lord Howe、Troll、Chatham、Kathmandu、Singapore）用实际分钟参考验证偏移边界；Troll 回拨日的 01／02 点均为 120 分钟。真实 HTTP 验证独立 SDK 与 Desktop facade 记录／revision／小时量一致，分析容量用尽时新端点同样拒绝而不挤占配置写。证据为 `tmp/acceptance/m2s-hourly-tests.log`、`m2s-timezones.log` 与完整门禁内的契约测试。
- 完整门禁各项通过：66 个 TypeScript 文件、最终 47 项浏览器检查、40 项 SDK 测试、Desktop 781 passed / 22 ignored、独立后端 622 passed / 11 ignored，以及生成器、边界、Clippy 和原 bundle 预算。先完成 `m2s-full.log`，随后仅因纵轴修复重跑前端 `m2s-frontend-final.log`，因并发占用收口重跑 `m2s-rust-final.log`／`m2s-daemon-final.log`。初轮修正了 replay helper 的旧导入和共享查询 helper 的补丁接入；未把旧 TS 算法保留为运行时兼容路径。
- 最终 headless 成品在新 Local profile 中通过新 History 产品接口与一条合成会话的数量一致性、两 SDK 分类同步、认证拒绝、资源 CAS、正常关闭及重启数据保留。SHA256 为 `a335cfe67fc7b33f9256c56bb27ee7e9b248a1fc7431d8d068961b99ab4f2305`；证据为 `tmp/acceptance/multi-client-m2s-history-hours-final/`、`m2s-independent-client-final.log` 和 `/tmp/patina-independent-client-hbw46d1t/`。未连接生产 profile、真实音频／桌面采样，也未安装、推送、合并或发布。
- 整体基础阶段继续；最短展示时长的无条件快捷写、其他剩余客户端业务／契约、网页产品决定及迁移、客户端偏好归属与独立安装仍未完成。此批不宣称四客户端已交付或全部客户端业务计算已经退出。

### M2t 执行设计：快捷设置采用显示时的版本

- History 最短展示时长快捷入口沿用原显示快照的 policy revision，不在保存前重新读取并覆盖基线；未取得有效基线或已有请求时禁用操作。等待确认期间保留已确认数值，冲突／失败提示并重新读取，不重试写操作。
- 全局设置快照与其条件写入基线由独立 `useAppSettingsRuntime` 持有；追踪 hook 只组合此 owner。订阅、前台刷新、有限读重试及卸载处理沿用已有读取控制器；迟到写确认不能覆盖在途取得的新快照。设置页或追踪状态同步改变共享配置时使快捷基线失效并刷新，客户端纯显示偏好不触发提前重读。
- persistence adapter 拒绝缺少 policy baseline 的普通策略 patch；删除无调用方的无条件 Settings 更新 helper。暂停状态同步仅读取产品快照，不为了一个布尔值聚合偏好／资源／凭据。旧暂停控制命令仍是独立控制接口，不在本切片暗中更改其语义。
- 验证快捷保存只调用一次条件命令、失败无 fallback，以及真实浏览器中的重复点击、未通知外部更新产生冲突和迟到确认不回退显示。该批不涉及新客户端 UI、数据库迁移、生产安装或发布。

### M2t 核验结果

- History 快捷入口已经使用原始 policy revision 保存；等待期间保持确认数值并禁用重复操作。更新的快照优先于迟到保存确认，失败保留显示、使旧基线失效并补读。全局设置生命周期从追踪 effect 移出，设置与基线一起更新；保存请求由单独 ticket 限制，旧生命周期的完成不能清除新请求。共享策略字段清单供 adapter 与设置草稿判断复用，无调用方的无条件更新 helper 已删除。
- 服务测试确认缺少 policy baseline 时不发 IPC；正常提交只发一次条件命令，冲突不补 GET 后重试、不回退旧写入口；暂停状态读取只使用产品快照。浏览器在实际时间线弹窗内验证双击只产生一次写入、静默外部更新使旧版本失败且保留外部值、在途新快照不会被旧确认回退（含 DOM 变更观察）。原设置页的编辑保留、普通策略和资源冲突回归也通过。前两轮修正了测试未打开弹窗及多条提示并存时的定位，未放宽行为断言。
- 完整门禁的全部组成项通过：67 个 TypeScript 测试文件、48 项浏览器检查、40 项 SDK 测试、Desktop 781 passed / 22 ignored、独立后端 622 passed / 11 ignored；协议生成比较、命名／架构／Rust／依赖边界及 Clippy 通过。`m2t-full.log` 保留首轮运行至浏览器定位失败的记录；修正后的浏览器、余下 4 个 TS 文件、构建及后端门禁分别在 `tmp/acceptance/m2t-browser.log`、`m2t-remaining-ts.log`、`m2t-build.log`、`m2t-client.log`、`m2t-rust.log`、`m2t-daemon.log`，没有重跑已通过且未变化的测试。
- 构建初次触发入口预算：新快照／写入生命周期代码使入口 gzip 从 M2s 的 72.94 KiB 增至 73.59 KiB。显式记录并将该项上限从 73.25 调整为 73.75 KiB；没有通过人为拆 chunk 隐藏成本，也没有放宽 370 KiB 总预算，当前总量 369.22 KiB。最终门禁记录为 `tmp/acceptance/m2t-bundle-final.log`。本批未改 Rust 或安装路径，不重复 headless 成品、生产安装及已完成的发行验收。
- 仍只在专用分支本地实现与提交；未安装、合并、推送或发布。基础阶段未完成：其余业务／契约迁移、独立安装以及等待产品选择的网页与客户端偏好边界继续保留；新增 TUI／GPUI／Web UI 仍需先讨论。

### M2u 执行设计：Tools 独立客户端契约

- 将现有 Tools HTTP 请求与快照 DTO 放入 `patina-protocol`，只承载数据；domain 继续拥有存储解析、计时和番茄阶段推进，API 边界显式转换。不把后端业务方法迁进客户端依赖，不改变现有 JSON 或到期语义。
- 独立 SDK 暴露快照、提醒／软件提醒规则创建与取消、计时器／番茄开始及具名动作；写操作校验 daemon 的 Tools ownership 和 write capability，使用现有有界 transport，不重试。Desktop facade 改用 SDK，退出任意路径字符串的 Tools 动作入口。
- 用真实隔离 HTTP＋SQLite owner 验证独立 SDK 与 Desktop 读取同一状态、写后通知与单次动作；协议反序列化／响应预算／能力缺失／失败不重试做自动回归。保留既有提醒实时订阅与重连规则，不因新客户端接入回放到期提醒。本切片不新增界面、安装或发布。

### M2u 核验结果

- 共享协议已包含 Tools 的请求、枚举与完整快照，类型生成从 27 项扩展至 43 项；Desktop 前端原始快照类型消费生成结果，保留既有 unknown 校验及 camelCase 显示模型。domain 的计时／存储／阶段推进实现没有迁入协议 crate；API 边界显式转换，旧 HTTP／IPC 的 JSON 表示保持一致。独立 SDK 暴露全部现有 Tools 操作，Desktop facade 已转发到同一实现；固定动作由 `ToolsAction` 选择。
- SDK 写入检查兼容 daemon、Tools owned／ready 和 `tools` write scope；保留原有三秒／64 KiB 客户端响应限制，失败不重试或 fallback。四项新增 SDK 测试覆盖所有动作路径／请求、缺失能力与错误宿主、无效 ID、409 单次提交、未知枚举／必填结构／超限响应拒绝。它们不宣称旧接口已经获得并发草稿 CAS 或客户端重试幂等性。
- 真实 HTTP＋SQLite 测试运行后台 Tools 循环，固定可推进时钟，两个独立 SDK 与 Desktop facade 对照计时、分段、暂停、提醒规则及番茄快照。验证 JSON 转换一致、写后变更事件、取消提醒不触发、到期提醒只触发一次、新实时订阅不重放旧提醒；使用测试 sink，不发送本机桌面通知。编译初轮修正了测试对私有 tick 的调用，改为运行真实生命周期入口，同时修正迁移后的请求枚举与测试闭包。
- `npm run check:full` 完整通过：67 个 TypeScript 文件、48 项浏览器检查、44 项 SDK 测试、Desktop 782 passed / 22 ignored、无桌面后端 623 passed / 11 ignored；生成器、架构／依赖边界、Clippy 及 bundle 门禁通过。前端仅改变类型来源，总 JS gzip 仍为 369.22 KiB。证据为 `tmp/acceptance/m2u-full.log`；专项记录为 `m2u-tools-sdk.log`、`m2u-tools-native.log`。
- 新无桌面构建经临时 Local profile 和两个 SDK 探针进程验收：Tools 就绪后订阅，HTTP 执行计时控制，两个进程收到相同 Tools 事件序号，暂停快照／分段一致；认证拒绝、分类同步、资源 CAS、History 数量守恒、正常关闭／重启和数据保留也通过。该进程验收的写入由脚本经 HTTP 发起，类型化 SDK 写入由上述真实契约测试验证，不混称为新客户端 UI。二进制 SHA256 为 `78640cbac8de98535db3e6d185eb51cc849a9ce0ed98756a24e64428742afd8a`；证据为 `tmp/acceptance/multi-client-m2u-tools/`、`m2u-independent-client.log` 及 `/tmp/patina-independent-client-iarsfe7x/`。ELF 无 GTK／WebKit 直接依赖，保留现有 X11／Pulse 采样依赖。
- 本切片仅本地开发与隔离验证，未安装、合并、推送或发布。新 TUI／GPUI／Web UI 未开发，Tools 界面投影与其余 backend／宿主契约仍应按实际缺口审计；网页语义与客户端偏好恢复选择仍待讨论，独立安装和整体基础阶段继续。

### M2v 执行设计：Tools 并发状态与事务边界

- 审计发现分段编号、计时暂停／恢复和番茄到期判断在事务外读取，多个调用可基于同一旧状态更新；reset 的状态修改与分段删除也未原子提交。先用并发分段、并发到期和注入删除失败复现。
- `data/repositories/tools` 负责事务入口；写入从 `BEGIN IMMEDIATE` 起覆盖读取、判断、修改与结果，内部 SQL helper 使用同一连接，避免嵌套 pool checkout。完整 Tools 读取使用一个只读事务。现有存储／计时规则保持原 owner，不新增第二份业务实现。
- `ToolsRuntimeOwner` clone 共用操作锁，覆盖控制、启动恢复、后台 tick 和快照发布；取得锁后才取时间，内部已持锁 helper 避免自锁。HTTP tracking owner 的快照经过该 owner，显式只读宿主仍使用 repository 快照。事务失败／取消不留下半次 reset；串行执行不冒充旧客户端草稿 CAS 或网络重试幂等性。
- 完成并发与回滚证据后运行完整门禁、隔离 daemon 回归。不扩展到新 UI、宿主安装或发布。

### M2v 核验结果

- 修复前的三项独立回归均失败：16 次并发分段全部返回编号 1；16 次并发到期检查均返回完成；删除分段的 SQL trigger 失败后计时器已经被 reset。证据为 `tmp/acceptance/m2v-before.log`，不是仅凭代码推断竞态。
- 19 项 Tools repository 写入口统一从 `BEGIN IMMEDIATE` 开始，原 SQL 移到仅接收同一连接的私有 `tools/state`；前置读取不再游离在事务外，已有提醒及番茄计数事务合入调用方事务，避免嵌套获取连接。完整快照在一个读事务内取得；恒定的默认 Tools 配置不再伪装成数据库加载器。业务阶段计算、数据格式与 schema 保持原语义。
- daemon 的控制、恢复、tick 与快照／事件发布共用 owner 锁；锁后才取时间，内部持锁 helper 不再次加锁。embedded 包装原先每次创建 owner，现从受管状态复用 gate；没有复制运行时。HTTP 已有 Tools owner 时通过它读取，没有 owner 的只读宿主继续使用事务快照。
- 并发分段现在得到连续的 1–16 且总时长守恒，到期只返回一次完成并只增加一个番茄计数；删除失败整体回滚，解除故障后 reset 成功。WAL 双连接测试在 reset 已提交后仍从旧读事务取得一致的旧计时器／分段，后续新读看到清理结果。owner 测试确认等待期间不发布快照，释放锁后使用新时钟，并按顺序返回写后读取。真实双 SDK HTTP 测试另加 8 次并发分段，各请求的确认数量依次为 2–9，最终编号无重复。
- 完整门禁全部组成项通过：67 个 TypeScript 文件、48 项浏览器检查、44 项 SDK 测试、Desktop 787 passed / 22 ignored、无桌面后端 628 passed / 11 ignored，以及协议生成、边界、Clippy 与原 bundle 预算。首轮 `m2v-full.log` 在源码结构测试的旧私有方法名处停止，已按持锁 helper 更新；后续证据为 `tmp/acceptance/m2v-remaining-ts.log`、`m2v-build.log`、`m2v-client.log`、`m2v-rust.log`、`m2v-daemon.log`。未重跑已通过且未变化的浏览器等检查。
- 新 headless 成品经独立临时 Local profile 验收，Tools／分类双进程事件、计时控制、认证拒绝、资源 CAS、History 数量、关闭／重启与数据保留通过。复用未变化的 M2u SDK 探针，daemon SHA256 为 `85dd34a2257710e5566e29343609b5266aee42e61dc81d67fd933fce108e31e3`；证据为 `tmp/acceptance/multi-client-m2v-tools-transactions/`、`m2v-independent-client.log` 和 `/tmp/patina-independent-client-2jruo3if/`。
- 原子性限于单项数据库操作；串行 owner 不代表整次 tick 的全局事务、过期草稿 CAS、网络重试幂等或崩溃期间系统通知的恰好一次投递。未安装、合并、推送或发布，未开发新客户端 UI；其余基础阶段缺口及待讨论事项继续保留。

### M2w 执行设计：Tools 客户端快照时序

- 现有 store 的在途读取可覆盖较新的 runtime 事件，页面也会直接发布迟到操作响应。store 将拥有订阅前置、读请求合并、事件失效序号及单个在途写操作；墙钟时间不作为可排序 revision，事件到达后旧读取／旧写确认不能回退显示。
- 写入期间使早先读取失效；无法确认响应顺序或保存失败时重新读取事实，不自动重试动作。读取和预热也先建立共享订阅；订阅拆除后的旧回调无效，临时预热／在途操作完成后释放未使用订阅。
- 现有 Tools hook 改用 store 的动作入口，并用同步 ref 防止同一轮双击及卸载后的错误提示。不新增客户端、视觉或操作语义；用可控异步回归和真实浏览器验证旧响应、事件交错与重复点击，再完成规定门禁。

### M2w 核验结果

- 修复前，延迟读取把后来收到的事件快照覆盖掉：回归刻意让新事件采样时间为 1000、旧响应为 2000，原实现错误返回 2000（`tmp/acceptance/m2w-before.log`）。store 现在使用本地发布／失效序号，而非墙钟排序；新事件使旧读失效，写入开始使更早的读失效，无法确认操作响应顺序时补读。
- 页面不再直接发布操作返回值，统一通过 store 的单在途动作入口；等待操作时不额外读取中间状态。响应丢失后的重读可发现服务端已经提交，操作本身只执行一次。hook 使用同步 ref 阻止同一轮连续点击，并避免卸载后更新 busy 状态或显示错误提示。预热先订阅后读，无使用者时释放监听；订阅 token 使已拆除回调及晚到注册／清理失效。
- Tools 专项最终 27 项通过，新增覆盖读／事件交错、时钟回拨、预热订阅顺序、临时监听释放、重复动作、旧写确认、响应丢失后重读、晚到订阅与新订阅并存。真实浏览器新增连续点击与“暂停事件早于开始确认”的场景，检查只发一次开始命令、补读发生且 DOM 不短暂回到运行中；共 49 项浏览器检查通过。浏览器 mock 现提供有效 Tools 快照，首轮修正了测试文案路径；旧源码结构检查也从直接 publish 更新为 store 动作入口，没有放宽行为断言。
- 完整门禁全部组成项通过：67 个 TypeScript 文件、49 项浏览器检查、44 项 SDK 测试、Desktop 787 passed / 22 ignored、无桌面后端 628 passed / 11 ignored，以及生成器、边界与 Clippy。证据为 `tmp/acceptance/m2w-full.log` 的首轮记录、`m2w-browser.log`、`m2w-remaining-ts.log`、`m2w-tools-final.log`、`m2w-build.log`、`m2w-client.log`、`m2w-rust.log`、`m2w-daemon.log`。本批没有 Rust／协议运行时变更，不重复构建或实装 M2v 已通过的 daemon。
- 新的 store 协调使入口 gzip 从 73.59 增至 73.87 KiB，超过原 73.75 上限；记录成本后将入口上限调至 74 KiB。总 JS 为 369.53 KiB，仍沿用原 370 KiB 总预算，没有人为拆 chunk 隐藏增长。最终记录为 `tmp/acceptance/m2w-bundle-final.log`。
- 仅本地实现与提交，未安装、合并、推送或发布。此批保证已收到的新快照不被旧响应覆盖，不宣称已经解决断连期间的全部计时显示／新鲜度语义；网页与客户端偏好选择、独立安装及其余基础缺口仍需继续，新客户端 UI 仍待讨论。

### M6b 决策与执行设计：独立安装及版本身份

- 用户已选择“后端独立安装和升级；Desktop 按协议兼容连接”。该决定授权实现方向，不授权本机安装或公开发布；不再以 Desktop 与 daemon 版本字符串相等作为长期兼容性标准。
- 安装审计：当前 `tauri.conf.json` 的 DEB 同时拥有 `/usr/bin/patinad` 和 systemd unit，AppImage 在用户目录维护另一种已验证的版本化 runtime；自定义 user unit 会被保留并拒绝自动覆盖。`app/daemon_service/upgrade.rs` 的重载检查目前仍要求新后端版本等于 Desktop。这些边界必须以可验证的独立安装身份替代，不能只删除比较或给同一路径再造一个包。
- 先建立无副作用的 `patinad --build-info`，输出版本、协议兼容范围、构建目标及 desktop/debug 投影信息；不创建 runtime、读取 profile、获取 lease、打开数据库或启动 listener。安装候选可以据此检查真实二进制，再把二进制摘要与安装来源记录在安装 manifest 中，不用 Desktop 版本猜测目标。
- 后续按顺序完成独立安装布局／身份、旧整包迁移和服务 owner 兼容、运行／已安装版本诊断及重载目标验证、无 Desktop 环境的安装升级与数据保留验收。正式 DEB/AppImage 发布流程的变更和公开资产仍在发布授权边界内；现有精确版本保护在替代证据接入前保留。

### M6b 构建身份与候选归档检查点

- 已实现 `patinad --build-info` 的格式 1 JSON，包含 Cargo 产品版本、真实编译目标、服务端协议兼容范围及 desktop/debug feature 信息。实现归 daemon 宿主，入口在准备 runtime 前返回；`--version` 原输出不变，混用 runtime 参数会拒绝。它没有运行实例、凭据或 profile 路径，也不代表运行就绪。
- `scripts/package-daemon.py` 先复制并探测同一候选 ELF，拒绝 desktop 构建、非 Linux 目标、无效元数据／协议范围及未明确允许的 debug 构建。归档固定文件路径、模式、owner 与时间，包含二进制、manifest、systemd 模板、说明和原项目 LICENSE；manifest 绑定各文件 SHA256／长度／模式。旧输出不覆盖，debug 归档有明确后缀。没有添加安装行为或把未完成模板当作已安装服务。
- 三组 Python 回归已纳入默认 TypeScript 门禁，覆盖多种非法元数据、同一 payload 的归档可复现性、内容与权限绑定、许可文件保留、覆盖拒绝和错误 unit 模板。初轮发现本机 Python 3.10 没有 `hashlib.file_digest`，已改为有界分块摘要计算。最终专项证据为 `tmp/acceptance/m6b-package-tests-final.log`。
- `npm run check:full` 通过：68 个 TypeScript 文件、49 项浏览器检查、44 项 SDK 测试、Desktop 788 passed / 22 ignored、无桌面后端 629 passed / 11 ignored，以及生成器、依赖／架构边界、Clippy 与原 bundle 预算。随后补入 LICENSE，仅重跑归档专项；长期发布文档明确独立升级方向后，release-policy 专项也通过。证据为 `tmp/acceptance/m6b-full.log`、`m6b-release-policy.log`。
- 实际 debug 归档为 `tmp/acceptance/multi-client-m6b-bundle/archive/patinad-1.9.2-x86_64-unknown-linux-gnu-debug.tar.gz`，SHA256 `50dac967350028d3929888a58971c73b96e689d80143f7b248dfd89bc6c6d939`。全部成员经摘要／大小／模式核对后提取；二进制 SHA256 `45b8b0c3408062f3637f9ffdd60df18ac1096bb2cb24a63fe0372f9f09762b88`。源二进制及提取后执行文件的元数据查询均在临时 XDG 根下验证，不产生 profile／lease／数据库；错误参数也没有副作用。
- 提取后的执行文件通过新 Local profile 的双 SDK 分类／Tools 事件、控制、认证拒绝、资源 CAS、History 数量、正常关闭／重启与数据保留。复用未改变的 SDK 探针；证据在 `tmp/acceptance/multi-client-m6b-bundle/`、`m6b-metadata-process.log`、`m6b-extracted-metadata.log`、`m6b-independent-client.log` 和 `/tmp/patina-independent-client-dfwjq11a/`。ELF 仍依赖 X11／XCB／Pulse 等宿主库，不是全发行版或静态分发承诺。
- 这是 M6b 的构建与候选归档检查点，不是独立安装／升级完成。manifest 摘要不是发布者签名；实际安装身份、旧包迁移、服务冲突处理和独立版本重载验证仍按上述顺序继续。未改生产 profile、安装、合并、推送或公开发布，仍未开发新客户端 UI。

### M6c 执行设计：独立 runtime 的版本化存放

- 安装文件边界归 Linux platform owner，daemon CLI 只解析显式参数并返回结果。第一步接收已提取、外层来源已由调用方确认的候选目录及预期 manifest SHA256；不自动下载、运行候选或把摘要冒充签名。
- 校验固定文件集合、manifest 格式／构建目标／debug 投影、所有内容摘要与长度，拒绝链接和非普通文件。复制到私有 staging 后再核验，以 manifest 内容摘要命名不可变版本目录；同一版本复用前重新检查已有内容，不覆盖损坏版本。
- runtime 根必须是新／空目录或已有合法独立安装根，独占安装锁覆盖 staging 与发布；通过原子目录发布保留旧版本，失败只清理本次临时目录。返回可供后续诊断使用的版本身份。此步不选择 current、不修改 systemd unit、不启用／重启服务，也不接触 profile 数据；下一步再接显式激活、旧 owner 迁移和目标版本验证。
- 用隔离目录验证损坏、路径／链接、并发安装、重复 staging 与失败不覆盖；真实候选经该边界存放后再做无副作用 metadata 和私有 profile 运行验收。

### M6c 版本化存放检查点与阶段汇报

- 已实现显式 `--stage-runtime` 入口，固定候选清单校验、私有安装根标识、独占锁、原子发布与已有版本重新校验。错误摘要、损坏内容、链接／额外文件、特权模式、错误构建投影／目标、无效协议／长度、无标识用户目录和忙锁均有回归；不执行候选或改动服务。打包脚本的 JSON 输出新增 manifest 摘要，归档格式不变。
- 完整 `npm run check:full` 通过：68 个 TypeScript 文件、49 项浏览器检查、44 项 SDK 测试、Desktop 796 passed / 22 ignored、无桌面后端 637 passed / 11 ignored；类型生成、依赖／架构边界、Clippy 和 bundle 预算通过。证据：`tmp/acceptance/m6c-full.log`；新增 7 项文件存放测试与 1 项 CLI 参数测试。
- 真实验收使用本批新构建作为存放控制程序（SHA256 `e0507c4fe1d24908f9e50df868d72ef5ca2f0ba370e84424484d96293d3358bd`），存放此前 M6b 已验证的候选（SHA256 `45b8b0c3408062f3637f9ffdd60df18ac1096bb2cb24a63fe0372f9f09762b88`）。两次操作返回相同身份，仅产生一个版本，无 current、服务或 profile 文件；随后对存放后的候选执行 metadata 与双 SDK 隔离运行验收，分类／Tools 同步、条件写入、认证、关闭／重启及数据保留通过。此处验证的是 M6c 存放能力，不能声称 M6b 载荷包含本批新 CLI。
- 证据位于 `tmp/acceptance/multi-client-m6c-staging/`、`tmp/acceptance/m6c-independent-client.log`、`/tmp/patina-m6c-stage-qtxtiygj/` 与 `/tmp/patina-independent-client-mcffgxrc/`。候选是本地 debug 成品；manifest 摘要不是发布者签名，隔离运行不等于生产安装或 GNOME 实机验收。
- 阶段状态：独立协议／SDK、核心应用读模型、设置／资源条件写入、Tools 共用契约与同步、防陈旧响应、无桌面后端构建均已有实现和自动／隔离证据。下一工作包应完成独立后端的激活、旧 DEB/AppImage owner 迁移、已安装与运行身份核对及失败恢复，再收口其余 Desktop 业务／契约例外。网页停用后的历史可见性、URL 隐私统一策略、客户端偏好恢复语义仍待产品选择；新增 Web／TUI／GPUI 界面继续留在讨论边界。
- 本检查点只做本地开发、验收与提交；生产 1.9.2、main 和宿主服务保持原状，未合并、推送、安装或公开发布。按用户要求在此集中汇报，不提前启动下一批。

### M6d 执行设计：运行实例的二进制身份与独立诊断契约

- 已核对 `bde592f3` 的干净分支；上轮检查未留下代码修改。激活设计需要先消除一个实际缺口：当前 service snapshot 只有实例和重启 ticket，版本字符串无法区分同版本不同成品，也不能证明启动路径被替换后实际运行的是哪份文件。
- Linux platform 在 daemon 启动时通过 `/proc/self/exe` 打开运行映像并计算有大小上限的 SHA256；不重新解析安装路径、不执行其他二进制。宿主将静态 build-info 与该摘要绑定并随 lifecycle owner 缓存，HTTP 读取不反复读大文件。测量失败明确返回身份不可用及错误，不伪造摘要或使追踪本身不可用。
- service 快照／重启 DTO 归独立协议 crate，SDK 提供类型化诊断和已存在的重启操作，Desktop facade 转发同一 SDK。身份字段可选，旧 daemon 响应仍能读取；新升级验证必须显式要求可用身份，不把旧响应当作已证明目标。
- 本批验证独立客户端读取与 Desktop 契约一致、能力缺失时拒绝重启、失败写入不重试，以及真实运行过程中替换启动路径后仍返回原映像摘要。现有精确版本重载保护保留；安装身份选择、服务迁移及协议兼容重载继续接在该证据之上。

### M6d 运行身份与独立服务诊断检查点

- 已实现运行映像的有界测量与实例内缓存，service API 增加可选 `executable`（build-info＋SHA256）和 `executable_error`。Linux 读取内核的 `/proc/self/exe`，启动路径被替换／删除不把新路径内容误报为正在运行；不暴露本地执行路径。非 Linux 或测量失败明确返回不可用，追踪不因身份测量失败而停止。
- 服务快照与重启 DTO 已迁到 `patina-protocol::service`，SDK 拥有类型化读取、响应边界校验和单次重启 POST；Desktop 原 facade 改为转发。旧响应缺少身份仍可读；畸形摘要、格式／协议区间、矛盾的成功与失败字段及超限响应被拒绝。重启前要求 daemon owner、ready、write scope 和协议兼容；原重载 ticket／新实例／精确版本验证保留，尚未宣称独立升级可用。OpenAPI 与人读文档同步更新。
- 完整 `npm run check:full` 通过：68 个 TypeScript 文件、49 项浏览器检查、48 项 SDK 测试、Desktop 799 passed / 22 ignored、无桌面后端 640 passed / 11 ignored；生成类型、架构／依赖边界、Clippy、bundle 预算通过。新增 4 组 SDK 回归与 3 项原生回归；初次专项在限制环境中无法绑定回环端口，后续完整门禁在允许隔离网络的环境执行通过。证据为 `tmp/acceptance/m6d-full.log`。
- 本批真实 debug 候选 SHA256 `bd5b1d11c1156d1076fdc9ce4aa89a919c52ae4145b902ad29de449f086339b4`，SDK 探针 SHA256 `c4483109ae525659732f7668545b1ffd619b2ae45c68edd0318e899ab9aada4f`。metadata 无副作用检查通过；临时 Local profile 从候选的私有副本启动，运行时将该启动路径替换成不同内容，HTTP 快照及两个独立 SDK 均继续报告原候选摘要，实例未改变。分类／Tools 同步、条件资源写入、认证拒绝、正常关闭、重新打开及数据保留也通过。
- 证据为 `tmp/acceptance/multi-client-m6d-executable-identity/`、`tmp/acceptance/m6d-independent-client.log` 和 `/tmp/patina-independent-client-2ku2didl/`。此批没有增加 systemd 激活操作；下一批仍须完成安装选择与激活状态、旧 DEB/AppImage owner 迁移，再用已安装身份与本批运行身份验证目标。未安装、合并、推送或发布；未开发新客户端 UI，整体基础阶段继续。

### M6e 执行设计：可核对的已选安装版本

- `bdd3382c` 分支已核对。运行身份已经能回答“实际运行哪份二进制”，安装侧仍缺少唯一的期望目标；staged 目录存在不能等同于已选版本。先建立该状态，再由服务 owner 将运行状态收敛到它，避免以 Desktop 版本推断目标。
- Linux standalone owner 复用同一安装锁与完整载荷校验，增加显式 inspect／select。唯一选择记录为 `current -> versions/<manifest SHA256>`，原子更换该相对链接；其他文件、外部路径和异常链接一律拒绝，不能顺手覆盖自定义内容。已有版本目录保持不可变，读取也重新校验其身份与内容。
- select 必须携带操作者观察到的旧 manifest 摘要，首次选择明确传 none；锁内比较后才提交。拒绝从较高 SemVer 选择较低版本；同版本不同成品以摘要区分。当前选择内容损坏时拒绝静默修复；服务就绪不从 current 指针推断。
- CLI 在初始化 profile 前分派，显式选择只改变下一次服务启动应使用的目标，不停止／启动／启用服务，不改数据库。随后服务激活将使用该稳定入口并处理旧 DEB/AppImage owner；本切片不能作为完整安装升级验收。测试覆盖并发旧基线、忙锁、篡改、非法 current、失败保留和真实候选选择后身份相符。

### M6e 安装目标选择检查点

- 已实现 `--inspect-runtime` 和携带 `--expected-current` 的 `--select-runtime`。同一 installation owner 复用原有 staging 校验和锁，inspect 使用共享锁，stage／select 使用独占锁；读取不创建安装根，选择不执行载荷。已选身份、静态构建信息和二进制摘要可直接与 M6d 运行身份核对。
- 7 项新增原生选择回归覆盖并发只允许一个旧基线成功、重复选择不更换链接、较低版本拒绝、同版本不同构建、异常 current 内容保留、目标／当前载荷损坏、缺失目录、忙锁及 debug 明确允许；1 项新增 CLI 回归拒绝缺少基线和混用命令。首次编译修正了测试模块相对导入层级；最终安装模块专项 14 passed。
- 完整 `npm run check:full` 通过：68 个 TypeScript 文件、49 项浏览器检查、48 项 SDK 测试、Desktop 807 passed / 22 ignored、无桌面后端 648 passed / 11 ignored；生成类型、架构／依赖边界、Clippy、bundle 预算通过。证据为 `tmp/acceptance/m6e-full.log`、`m6e-selection-unit.log`。
- 真实验收将 M6d 与本批两份 `1.9.2` debug 构建分别归档、提取并存放到私有根，验证从无选择到旧构建、再到本批构建；重复选择复用原链接，过期基线被拒绝，metadata 与内容摘要相符，不创建 profile 或服务。可重复入口为 `scripts/acceptance/standalone-selection.py`，输入两个已确认来源的候选及其 manifest 摘要；不能把脚本的内容校验当作签名认证。
- 本批二进制 SHA256 `cc7ff6e9e0ff8be35196a52ba70cffcaa1bde99fe9bba8216e25c7694a4594bc`，manifest SHA256 `9a589e3b7a0172117a07804e56d96729d54de633128eee0b922442965f8fefbe`。随后选中的二进制通过双 SDK 隔离运行验收，报告的运行摘要与已选目标一致，启动路径替换、分类／Tools 同步、条件写入、关闭／重新打开和数据保留通过。复用未变化的 M6d SDK 探针。
- 证据位于 `tmp/acceptance/multi-client-m6e-selection/`、`tmp/acceptance/m6e-independent-client.log`、`/tmp/patina-selection-qgxa9v2r/`、`/tmp/patina-independent-client-gfrffmsv/`。较低 SemVer 拒绝仅约束该安装根的已选版本，不代表已经处理旧 DEB/AppImage 数据兼容；同版本摘要切换也不证明数据库可降级。下一工作包必须接入服务激活和旧 owner 迁移，保留运行 lease／交接状态及用户自启动偏好，再替换现有精确版本重载检查。
- 本检查点仅本地实现、隔离验收及提交；main／生产安装保持原状，未推送、合并、安装或发布，未开发新客户端 UI。整体后端基础阶段继续。

### M6f 执行设计：独立服务接入与可恢复激活

- `02c6f843` 干净基线已核对。服务激活不能只写 unit：旧 Desktop 依赖 profile control root 的 cutover reservation 决定 client／embedded 模式；必须保留该唯一决策机制及 RuntimeLease，不能另外建立可同时启动后台的分支。
- 服务文件渲染和可信文件检查归 Linux platform，先从现有 AppImage 实现提取保持字节兼容的窄边界，供 standalone 固定 `current/bin/patinad` 入口复用。systemd manager 的 profile roots、实际 unit 来源及 drop-in 必须核对；自定义 unit／mask 保留，旧 DEB/AppImage 替换须走具名迁移而非按文件名覆盖。
- 安装宿主负责激活顺序：锁定目标、预检候选和服务归属、持久记录激活意图、受控停旧服务、等 lease 释放、准备 unit 和 cutover、启动后用协议／ready／实际二进制摘要核对目标、最后确认迁移完成。重试复核意图与外部状态，不默认降级；用户的后台／Desktop 开机偏好保持原值。
- 首次独立安装与已有 completed cutover 分别覆盖；进行中的其他迁移、数据目录维护和损坏状态不能被该流程擅自修复。失败必须留下可检查的状态，不能误报激活完成。先用隔离状态机与服务环境验证；不在本机生产服务执行新激活命令，不把生成 unit／mock 管理器测试当作实装验收。

### M6f 服务准备前置检查点（激活流程仍在实施范围）

- 已从 AppImage 宿主提取 `platform/linux/patinad_service_unit`，固定共享基础 unit 策略和路径转义；AppImage 继续使用 AppRun＋`--patinad`，standalone 使用已选 `current/bin/patinad`。新增 `--print-runtime-service <manifest>`，要求明确 config／data roots，在共享安装锁内重新核对选中载荷和摘要，返回绑定目标的可审阅 JSON；不会安装或操作已有 unit。
- 运行锁与 cutover 审计确认：独立激活必须复用 `app/runtime_lease` 和 `app/runtime_owner_cutover`。只启动 systemd 服务而不建立对应 cutover 会使 Desktop 仍选择 embedded 启动路径；激活流程还需持久意图和恢复处理。本检查点没有引入第二份后台归属状态，也未实现实际服务启动／迁移。
- 完整 `npm run check:full` 通过：68 个 TypeScript 文件、49 项浏览器检查、48 项 SDK 测试、Desktop 811 passed / 22 ignored、无桌面后端 652 passed / 11 ignored；类型生成、架构／依赖边界、Clippy 和 bundle 预算通过。新增 2 项渲染、1 项目标绑定／既有文件保留和 1 项 CLI 参数回归。证据为 `tmp/acceptance/m6f-unit-full.log`。
- 直接编译 `02c6f843` 中旧 AppImage 渲染函数与新共享函数，在普通路径及含空格、百分号、美元符号、引号／反斜杠的 3 组路径上逐字节输出一致；证据为 `tmp/acceptance/m6f-appimage-unit-compat.json` 与 `/tmp/patina-unit-render-lcjri5wk/`。
- 实际 headless 控制程序 SHA256 `b09a47d6925f6ffe2b6227c0fa46e2a7b46a0b82f69c801372d94cf9c4440f0c` 对 M6e 私有选中版本生成 unit 预览，目标身份相符、重复输出一致、既有自定义 unit 未改变，CLI 未创建默认或显式 profile 根；生成文件通过 `systemd-analyze --user verify`。首次验收发现解析器自身创建 XDG runtime 目录，因此将解析器与 CLI 的运行目录分开后复核；应用代码没有为此改动。
- 原生证据在 `tmp/acceptance/multi-client-m6f-service-unit/`、`/tmp/patina-service-preview-fea6p_yy/`。unit 解析与字节对照不证明实际启动、旧包接管、DB 升级或 GNOME 生命周期；这些仍按 M6f 执行设计继续。此批未改变 runtime 业务代码，不重复上一检查点已通过的双 SDK 数据保留验收。
- main／生产服务未变，仅本地实现、验证及提交，未推送、合并、安装或发布，整体目标仍未完成。

### M6f 首版受控激活与恢复检查点

- 已实现 `--activate-runtime`：锁定已选载荷、核对实际 build-info、manager profile roots、unit 来源／drop-in 和登录偏好，再持久记录激活意图、停服务、等待 RuntimeLease、持有 Maintenance lease 安装 unit，释放 lease 后启动并验证协议、就绪状态和运行映像。读取能力前后核对同一实例，避免重启竞态把旧实例的 ready 与新实例身份拼接。当前支持新 profile 或受认可的 standalone 配置；旧 DEB/AppImage unit、已有 embedded 数据、其他未完成迁移及配置冲突仍拒绝接管。
- `runtime_owner_cutover` 已成为 headless／Desktop 共用模块；安装器先保存将使用的 cutover ID，再预约并推进同一迁移状态。启动成功后中断的操作可直接核对并完成，不重复重启健康后端；失败保留 prepared／starting 和有界错误，不自动回退二进制。managed daemon 使用自身 writer 镜像已有 cutover 的登录偏好，激活不执行 enable／disable；AppImage Desktop 可识别并保留合法 standalone 绑定。
- 状态机回归覆盖首次完成、停止／安装／启动／验证失败后的继续、启动后取消、健康重复执行、保留 completed cutover 偏好，以及拒绝其他迁移／损坏状态。HTTP 回归覆盖独立的后端版本、错误摘要／版本、未就绪、实例更换和身份缺失；文件回归验证 unit 原子发布、mask／自定义文件／硬链接保留。新建控制目录使用明确的 0700，不依赖调用环境的 umask。
- 最终 Rust 复核曾暴露旧安装锁在操作返回后仍被占用。重复文件描述符回归证实仅关闭本 fd 不会结束共享文件描述符的 flock；安装根锁和新 profile 激活锁均改为 RAII 显式 unlock，测试夹具也显式释放自己的原始锁。源码由该回归与完整 Rust 门禁复核，不通过重复运行掩盖失败。
- 完整门禁全部组成项通过：68 个 TypeScript 文件、49 项浏览器检查、48 项 SDK 测试、Desktop 823 passed / 22 ignored、无桌面后端 685 passed / 11 ignored；类型生成、架构／依赖边界、Clippy 和 bundle 预算通过。最初完整执行在 `tmp/acceptance/m6f-activation-full.log`；后续 Rust 修正的最终证据为 `m6f-activation-rust-final.log`、`m6f-activation-daemon-final.log`，未重复未变化的前端／SDK 门禁。
- 可重复的原生适配器验收入口为 `scripts/acceptance/standalone-activation.py` 和 `standalone-systemd-mock.py`：新建私有 D-Bus 总线、明确拒绝宿主总线，使用模拟 manager 和真实 daemon。覆盖已准备但未完成的 cutover 恢复、重复激活不再重启、两份同版本不同构建的受控切换、历史保留与自启动关闭；实际发生两次 start、一次 stop、两次 reload，没有 enable／disable。
- 最终控制程序及首次载荷 SHA256 `7c03e101888f59558311394682226ef475770b9242e2778b0fca6856ff481a03`，manifest SHA256 `54412d576afd7d4db349c0f94456e89ef1ad9d08d160515b121f3f7663458916`；第二次显式切换到已验证的 M6e 构建 `cc7ff6e9e0ff8be35196a52ba70cffcaa1bde99fe9bba8216e25c7694a4594bc`。证据在 `tmp/acceptance/multi-client-m6f-activation/`、`tmp/acceptance/m6f-private-activation-final.log`、`/tmp/patina-activate-private-9_c6dw72/`。这是本地 debug 候选及私有 Production profile 的模拟 manager 验收，不是真实 systemd 登录、PrivateTmp 命名空间或 GNOME 硬件验收，也不是生产安装／公开签名升级。
- 下一工作包应在上述激活流程上完成旧 DEB/AppImage 的具名迁移，再使 Desktop 的重载诊断按已安装目标与协议兼容性验证；保留版本精确比较的旧入口尚未退出。还需真实 systemd／登录环境和正式交付验收，以及此前列出的业务契约与产品讨论项。main、生产服务及自启动配置保持原状；本批只本地实现、验收和提交，未推送、合并或发布。

### M6g 执行设计：已知系统包／AppImage 服务的显式迁移

- 基线 `283b3074` 已核对。迁移沿用 M6f 的锁、持久意图、RuntimeLease 和目标核对；CLI 必须明确源类别、源 unit 摘要及源版本。只接受仓库已知 unit 配方，源文件和 mask／drop-in／自定义配置仍保留，不通过一个“强制”开关接管任意服务。
- 停止前核对 systemd 实际加载的启动命令／环境；运行中的源还须与该 profile 的锁 owner PID 对应，并经 API 确认 daemon 身份与版本。已在本机只读核对 `ExecStart`、`Environment`、`EnvironmentFiles`、`MainPID` 的实际 D-Bus 签名，未改变服务。目标不能低于确认的源版本；源运行版本高于声明源版本时拒绝继续。
- AppImage 迁移持有旧 runtime 安装锁，冻结已发布版本并复用版本预检；系统包 unit 只读取验证，在用户目录安装覆盖项，保留系统包文件。AppImage 的已知用户 unit 在停止和维护锁内按旧内容条件替换；旧 unit 文本随迁移证据保存，不自动降级回滚。
- 迁移证据必须先于 unit 替换持久化，以识别“已换文件但尚未 reload／确认”的重试。普通独立激活不能借历史证据重新接管后来出现的其他配置。客户端读取稳定绑定字段，不因安装器审计字段扩展而失去协议兼容连接能力。
- 验收覆盖源识别拒绝、过期确认、停止前后的源变化、替换后中断恢复、偏好和数据保留；不在本机执行真实迁移。旧 DEB 文件的卸载／分包归属和真实 systemd 登录继续作为交付验收范围。

### M6g 已知旧服务迁移检查点（2026-10-05）

- `--activate-runtime` 新增成组的 `--migrate-from packaged|appimage`、`--source-unit-sha256`、`--source-version`。源确认、实际加载命令／环境、profile 锁 PID、源版本和目标版本通过检查后才持久化迁移意图和停止服务。源未知、drop-in、配置漂移或降级目标均拒绝；系统包文件保持只读，AppImage unit 仅按已知旧内容替换。
- AppImage 已发布 runtime 的检查不初始化旧根，并在整个迁移中持有原安装锁；回归覆盖在途更新被拒绝、current 被外部改动后拒绝继续，以及继承文件描述符不拖延解锁。RuntimeLease 新增只读 owner 检查，未持锁的陈旧 metadata 不算运行进程证据。用户 unit 替换保留未知文本、mask 和硬链接；迁移前保存原文，失败不自动回滚。
- Desktop 读取稳定绑定投影，安装器仍严格验证完整 journal。未知审计字段不阻止客户端读取绑定，非法 roots 仍拒绝。profile 安装锁在读取旧迁移意图前取得，避免从未串行的旧记录构造恢复来源。
- 完整 `npm run check:full` 通过：68 个 TypeScript 文件、49 项浏览器检查、48 项 SDK 测试、Desktop 830 passed / 22 ignored、无桌面后端 691 passed / 11 ignored；类型生成、依赖／架构边界、Clippy、bundle 预算通过。证据为 `tmp/acceptance/m6g-full.log`。之后 Rust 仅有格式调整；补充的验收夹具检查单独执行。
- 候选是本地 `1.9.2` debug 构建，二进制 SHA256 `4e9c6e6ecf03f7f35d36ff4800680cfe1cd5241445773a26db9fbe0b73b730b2`，manifest SHA256 `9cceef1807e755cc30a4937d5a6d2174b9fc4b31cc89efa667a91141acc5d8e7`；归档与逐场景日志位于 `tmp/acceptance/multi-client-m6g-migration/`。旧进程使用只读的已安装 `/usr/bin/patinad`，所有进程连接私有 D-Bus／临时 profile，数据库只有合成验收记录。
- standalone 回归、系统包迁移、合成 AppImage 布局迁移均通过；两个源类别都覆盖登录启用时 unit 已替换但首次 reload 失败，再凭 journal 无源参数重试。确认原 unit 文本有记录、源 PID／环境／drop-in 漂移在 stop 前被拒绝、重复已完成激活不重启、历史数据与后台／Desktop 登录偏好保留。两个恢复场景分别在 `/tmp/patina-activate-private-dftzz22e/` 和 `/tmp/patina-activate-private-mf8vbuud/`。
- 额外的系统包迁移后再次切换已验证 M6f 同版本不同构建通过（`migration-then-switch.log`、`/tmp/patina-activate-private-oolluun_/`），不隐式重做源迁移。AppImage 停服期间的外部 unit 编辑被保留并中止发布；只有测试夹具显式恢复原内容后才能继续（`appimage-external-edit.log`、`/tmp/patina-activate-private-0ruhx1za/`）。首轮恢复验收错误地预期向已停止服务再次发送 StopUnit，已按既有幂等控制行为修正断言并重验；应用没有为迎合断言新增停止操作。
- 离线 `systemctl --root=<temporary> --global is-enabled` 验证用户层等价覆盖加入前后、夹具中移除原系统 unit 后均为 enabled，原链接未改；证据 `/tmp/patina-unit-enable-offline-stqbcylz/result.json`。这是离线查找语义，连同模拟 manager 的启用状态均不能替代真实 systemd 登录验收。AppImage 使用合成 AppDir，不宣称真实 AppImage／FUSE 分发通过。
- 下一项处理 Desktop 的重载／版本诊断：独立后端按已安装目标身份和协议就绪检查，不再要求版本等于 Desktop；旧整包兼容路径保留必要验证。还需核对安装元数据和客户端连接的边界，避免安装器内部状态扩展意外阻止兼容客户端连接。旧包卸载／分包、真实 systemd 登录及正式交付仍未完成；本批不安装、不合并、不推送、不发布，不进入新客户端 UI。

### M6h 执行设计：Desktop 重载核对独立后端目标

- 重载编排继续归 `app/daemon_service/upgrade`，安装来源适配读取稳定绑定和已选目标；独立目标使用版本＋实际构建身份／二进制摘要，旧整包路径暂保留 Desktop 版本保护。诊断返回来源、已安装版本和 current／pending／unverified 状态，现有设置卡不再自行以 Desktop 版本差异判断独立后端异常。
- 用户确认绑定观察到的运行实例、版本及目标身份；执行前重新核对，持有独立安装根锁和 profile 激活锁直至完成，防止另一轮 select／activate 改变目标。API 仍只发送一次 restart，完成须有匹配 ticket、新实例、目标身份、协议兼容和追踪就绪；失败不重试写入、不回滚。
- AppImage 对已登记独立服务的识别不再完整遍历安装载荷，保留已知绑定、profile 和服务配置检查；安装载荷校验仅用于明确的重载／安装操作，不让候选内容或其 manifest 解析成为健康客户端连接的前置条件。现有客户端 UI 只调整诊断和确认数据，不进入新 Web／TUI／GPUI 设计。
- 回归覆盖独立版本与 Desktop 不同但目标已运行、同版本不同构建待重载、确认后目标／实例改变、错误目标和协议、单次重载失败，以及现有 UI 的确认和忙状态。继续隔离测试，不操作本机生产服务。
- 设置诊断每 30 秒刷新，因此周期观察仅核对有界 manifest／文件属性；完整内容摘要和实际 `--build-info` 放在确认后的持锁预检。旧整包重载也取得同一 profile 锁，避免与首次 standalone 迁移交错；预检子进程清除 AppImage 挂载／动态加载覆盖，保持原生后端依赖边界。

### M6h 独立目标重载检查点（2026-10-05）

- Desktop 重载诊断已按来源返回 target version、current／pending／unverified 和确认 revision。standalone 比较构建身份与摘要，版本不同于 Desktop 可为 current，同版本不同构建仍可为 pending；旧整包保留配套版本保护。现有设置卡改用后端状态，确认传回原始 revision，不自行推断版本关系；没有新增客户端或视觉系统。
- 重载前重新核对实例和目标，在安装根锁及 profile 锁内验证全部载荷、实际 build-info、磁盘和已加载 unit。profile 锁也覆盖旧整包重载与首次迁移的互斥。共用命令／环境校验和候选预检供安装宿主及 Desktop 调用；写请求只发送一次，完成核对 ticket、新实例、目标、协议和 tracking ready，旧响应和错误目标不能被拼成成功。
- 周期查询只读有界 manifest 和文件属性。回归明确证明同大小二进制损坏不会被该元数据观察认证，真正持锁重载仍拒绝；manifest 损坏立即拒绝。AppImage 已知 standalone 绑定识别不再扫描载荷，未知安装器审计／阶段字段不影响绑定读取；未完成或未知安装阶段仍不能执行重载。
- HTTP 回归覆盖独立 2.0.0 后端与 Desktop 版本不同的 current 状态、同版本不同构建、错误摘要／版本／协议、确认后实例／目标变化、安装忙、验证中实例更换、延迟 ready 和单次失败。真实浏览器复用现有确认／忙状态场景，使用版本均为 2.0.0 但目标状态 pending 的后端，验证确认 revision、取消、重复点击抑制及成功后不再因 Desktop 1.9.2 而告警。不同产品版本的证据来自契约夹具，实际成品切换仍是两个 1.9.2 debug 构建。
- 完整门禁全部组成项通过：68 个 TypeScript 文件、49 项浏览器检查、48 项 SDK 测试、Desktop 837 passed / 23 ignored、无桌面后端 694 passed / 11 ignored；类型生成、依赖／架构边界、Clippy 和 bundle 预算通过。完整运行在 `tmp/acceptance/m6h-full.log`，最终锁／预检／记录收敛修改的 Rust 证据为 `m6h-rust-adoption.log` 和 `m6h-daemon-adoption.log`，未重复未变化的前端和 SDK 门禁。
- 最终原生宿主验收使用私有 D-Bus＋真实 daemon，从 M6g 构建 `4e9c6e6ecf03f7f35d36ff4800680cfe1cd5241445773a26db9fbe0b73b730b2` 切换到本批 `c46655359ccfa418f79a9a1154b7cb784c2bef4e259a2328f027561642b30b85`；目标 manifest `cb24d078ab5ec8a35cd09d354ec1e211c226039f69cf89eedbbf4d9840757e84`。明确运行默认忽略的 `native_reload_selected_backend`，一次通过；证据 `tmp/acceptance/multi-client-m6h-reload/final/`、`/tmp/patina-activate-private-859mv80k/`。
- 原生验收确认目标选择和 profile 激活均被锁住，单次 API restart 让旧进程以 75 退出、模拟 manager 启动新目标，API 核对实际摘要、实例和就绪。重载后再调用 CLI 激活，安装器更新目标记录而不重复停启；全过程包括首次启动共一次 StartUnit、一次 Reload、零次 StopUnit、一次受控进程重启，历史与登录偏好保留。它不等于真实 systemd 登录、完整 Tauri 窗口或正式安装升级验收。
- 下一批继续独立后端自身版本与正式交付边界、旧整包文件归属及剩余业务契约清单。现有构建仍共享产品版本来源，不能把诊断允许不同版本视为独立发布流程已经完成；M2 网页／偏好边界和 M3–M5 新客户端讨论也仍保留。本批只在功能分支本地实现和验收，main、生产安装和远端保持原状。
