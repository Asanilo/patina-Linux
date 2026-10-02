# 一套后端与多客户端复用阶段计划

状态：进行中。2026-10-03 用户授权创建专用 worktree／分支，按效果推进后端复用与多客户端同步，必要时重构模块。本文件管理本阶段交付，不把规划项表述为已实现。

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

## 已有能力与缺口清单

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
| M1 | 独立 Rust 传输／协议基础，Desktop 接入；同步契约和 SDK 回归 | 第二个非 Tauri 进程可使用相同连接基础；请求、错误和 SSE 帧只有一份传输实现；通用重连／快照协调逐步从宿主提取 | M1a 已实现并验证；M1b 同步协调待实施 |
| M2 | 以“今天 → 应用／网页详情 → 历史”为切片，补最小读 API，迁移 Tauri；统一产品配置读取 | Tauri 作为标准客户端完成核心链路，统计规则由后端负责 | 待实施 |
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

下一执行入口：完成 M1b 的客户端事件／快照协调契约和现有适配复用，然后推进 M2 的今天／详情／历史读接口缺口。先以真实客户端调用证明必要抽象，不创建无人使用的通用状态框架。
