# `patinad` 当前实施与验收

> 更新：2026-09-23。daemon 分离已通过第一阶段验收并经用户明确授权合入本地 `main`；本文继续管理平台成果评估、候选证据与剩余发布门槛。历史阶段与实验详见 [归档](../archive/2026-09-19-patinad-runtime-history.md)。
> 方向以 [路线](../roadmap-and-prioritization.md#linux-main-and-daemon-experiment) 为准，协议与 owner 以 [架构](../architecture.md) 为准。当前执行见下方 Todo 与 [beta.21 候选记录](2026-09-21-linux-platform-reuse.md)；历史验收不自动覆盖后续候选。隔离安装、真实临时 systemd、界面重启及本机实装验收均已通过；源码合入不改变公开发布状态。

## 推进 Todo（2026-09-20）

用户已确认按“daemon 收口 → 验收与合并 → 选择性回流 Linux 平台成果”的顺序推进。本清单记录执行状态；分支职责与改动流向由路线文档拥有。`[x]` 仅表示有证据完成，不能用已安排或历史测试总数代替完成。

- [x] **T1：固定分支和范围。** 本批起始基准：`main` 为 `a13a64a`，daemon 为 `2193acc`，main 是 daemon 的祖先。`feat/linux-desktop` 已从上游 `80204c73` 创建，未完成草稿保留在独立 worktree，暂停扩大实现。两个工作树原有改动均保留，daemon 本批改动随当前验收记录固定提交。
- [x] **T2：完成差距审计。** 已清点前端全部 16 个 persistence 模块、Rust command 路由、runtime owner、候选发布和安装事实；读写例外、功能缺口与验证范围见 [本批审计](../archive/2026-09-20-daemon-owner-audit.md)。完成盘点不等于全部缺口已实现。
- [x] **T3：修复第一阶段阻塞项。** 分类写入和 missing-command SQL fallback、网页历史删除、客户端重连 schema 维护、无用 SQL load 权限、SSE 漏事件与 Tools 并发覆盖已修复。目录迁移/清缓存已接通受管模式离线维护，补齐 Desktop 跨进程屏障、旧客户端占用检测和迁移中断恢复；恢复默认目录误拒绝、双目录同时恢复默认时混淆文件种类的缺陷均已修复。不以删减 main 功能收口。专项与本批完整门禁通过，候选验收归 T4。
- [x] **T4：验证候选。** 最新 `check:full` 通过：55 个 TypeScript 测试文件、38 项浏览器回归、677 Rust passed / 15 ignored，以及构建/预算/边界/Clippy。main 实际 v1–6 schema 的合成 16 表升级回归通过。私有 dpkg root 完成 beta.18 安装→候选升级→卸载→重装；候选 daemon 配合真实临时 systemd、四次界面 `app.restart` 完成五阶段存储验收，50,000 条记录与总时长正确。用户随后明确授权本机实装；备份校验后完成 beta.18→本地候选升级，正式 Desktop 的存储页、关闭隐藏、单实例唤回、正常退出和新进程重开通过，退出后真实采样继续，daemon PID 不变。最终 15 项托管检查、历史记录摘要、SQLx 校验和及数据库完整性通过。候选身份、一次短暂数据库锁重试及证明范围见 [合并前证据](../archive/2026-09-21-main-merge-readiness.md)。
- [x] **T5：达到合入条件并收敛到 main。** 用户明确要求合并后，本地 main 从 `a13a64a669849234df5575994673cb7a90cc3003` 快进至已验收提交 `bc5c2e9c7f56251857add5b91dd527cb1340288f`，无冲突，保留全部开发及 beta 历史。主线文档同步提交为 `5eefcf4e`，用户随后同意交付收尾，已将该提交普通快进推送至 `origin/main`。后续产品开发在 main，原 daemon 分支保留；未打 tag 或改变公开发布状态。
- [ ] **T6：按模块评估并选择性回流平台成果。** [当前执行记录](2026-09-21-linux-platform-reuse.md)中的采样中断、idle 可信性、GNOME 双协议及会话重绑已实现并通过完整门禁；version 4 扩展已在本机激活，用户已完成锁屏/挂起。beta.20 本机 DEB 升级、实际 AppImage/DEB 共存、无界面持续采样、重新登录后的会话恢复及真实系统重启后的后台自动启动均通过。随后独立真实 systemd 验收发现并修复首次凭据竞态，新候选通过两秒延迟接管、界面重开、崩溃恢复、容器重启启动和六条隔离路由；714 Rust / 17 ignored 的完整门禁通过。新候选未替换本机安装。剩余为纯 AppImage 独立 GNOME 登录/FUSE、正式签名升级及后续 ESM/更多 Shell 验证；上游草稿仍保留，不整支合并。
- [x] **T7：准备合流后的下一版 beta。** 按公开 beta.19 `fbdad8eb` 之后的完整范围整理 beta.20 版本与 changelog，源码发布门禁通过；beta.20 已于 2026-09-22 公开预发布，后续审核修复已提交 main（`fa1cec27`），当前准备 beta.21，本项不代表 beta.21 已发布；2026-09-22 另完成未签名 AppImage 本地候选与隔离验收，不等于恢复该格式发布。AppImage 实装/正式升级门槛保持不变。

第一阶段不要求实现新客户端、补齐所有桌面或清零客户端私有持久化。受控读取可以保留明确例外；runtime 写入必须由当前 owner 执行。AppImage 实机验收仍约束该格式及 daemon 稳定发布，不以源码合并代替。本机现运行 2026-09-22 实装验收的本地未签名 beta.20 候选，生产 service 已受控重启；真实数据库继续记录，既有历史及 schema 校验通过。当前最新公开预发布为 beta.20；beta.21 的源码、包和本机安装状态由候选执行记录管理。

## 当前状态

| 项目 | 已有证据 | 尚不能宣称 |
| --- | --- | --- |
| daemon 与 Desktop 分离 | 独立二进制、profile/lease、HTTP/SSE、追踪与平台信号、主要写侧、服务交接和客户端适配已实现；既有 DEB 候选有实机验收 | 所有读取均已脱离 Desktop SQLite，或任意客户端均可直接替换 |
| 后台生命周期 | 已验证关闭/重开、故障恢复、回退/接管、特定登录环境、锁屏/挂起与 Zen 网页边界 | 所有桌面、硬件、登录配置长期稳定 |
| 数据与备份 | 有界趋势/分类读取、流式导出与预览、多表恢复回滚及隔离 systemd/WebDAV 恢复已有验证 | 完整恢复恒定内存、任意第三方 WebDAV 兼容 |
| AppImage | 旧 beta.20 候选具备宿主 DEB 共存证据；修复凭据竞态的新候选通过无 DEB 容器真实 systemd 首次接管、故障恢复及容器重启、六条隔离路由，见 [最新证据](2026-09-21-linux-platform-reuse.md) | 新候选已实装宿主、纯 AppImage GNOME 登录/FUSE 或正式签名升级已通过；允许公开发布 |
| 新客户端 | Tauri 保留；TUI、GPUI、本机浏览器 UI 是后续方向 | 已实现，或已决定三者开发顺序 |

## 版本与发布证据

- 2026-09-20 只读确认当时安装 beta.18，生产 service active/running 且使用当时已安装的 binary；未读真实 DB/token，不能据此宣称 tracking-ready 或升级通过。
- beta.19 发布准备提交 `fbdad8e` 已推送 `feature/patinad-daemon`，标签 `v1.9.0-beta.19` 已推送。
- 已确认 [发布工作流](https://github.com/Asanilo/patina-Linux/actions/runs/35449507248) 成功，[beta.19](https://github.com/Asanilo/patina-Linux/releases/tag/v1.9.0-beta.19) 已公开；tag 和构建 SHA 均对应 `fbdad8e`。DEB/manifest 摘要、公开签名和 DEB 成品检查通过，详见 [本批审计](../archive/2026-09-20-daemon-owner-audit.md#发布和安装事实)。
- beta.19 `fbdad8e` 的 `release:check` 通过：637 Rust 测试、38 浏览器回归、Clippy、版本/changelog、扩展与签名 XPI 检查。14 项 opt-in 测试默认跳过；[专项路由](../linux-development-setup.md#opt-in-validation-routing) 定义何时单独运行，下表关联既有证据。该数字不代表后续工作树已验证。
- 浏览器临时 profile 清理出现过非致命 ENOTEMPTY 警告，不记为零警告运行。
- 2026-09-20 的发布资产复核针对已公开 beta.19 成品，不覆盖随后本地候选；两者身份分开记录。AppImage 发布门禁未解除，稳定通道不变。
- 后续工作树验证：2026-09-20 基于 `2193acc` 的协作/门禁与 Settings owner 修复已通过 `check:full`，见 [本批证据](../archive/2026-09-20-agent-workflow-hardening.md)。当时未提交、未打包或安装；该结果不替代 beta.19 发布资产检查或随后的实装验收。
- 2026-09-21 本批 daemon owner、事件和目录维护修复的完整 `check:full` 通过，结果与限制见 [本批验证](../archive/2026-09-20-daemon-owner-audit.md#本批验证与交付状态)。这是同一基准上当时未提交的工作树，不是 beta.19 的安装包；浏览器临时 profile 清理有非阻断 ENOTEMPTY 警告。
- 2026-09-21 原生续验修复恢复默认目录问题后，完整门禁更新为 674 Rust / 15 ignored；`perf:heatmap-desktop` 与 `test:storage-native` 均通过。该次原生存储报告为 `/tmp/patina-storage-test-aoEjAu`，具体动作、构建摘要与证据限制见 [原生续验](../archive/2026-09-20-daemon-owner-audit.md#原生续验2026-09-21)。均为 debug 源码验收，当时未构建新安装包。
- 随后的本次合并前验证达到 677 Rust / 15 ignored；新候选包 SHA256 为 `40f65c6398f992ca38eaf4f55fcdef7df33b0712cdb107ffc7642967a977da77`，保留版本号但不是公开 beta.19 原包。隔离 dpkg、真实临时 systemd/界面重启和授权后的宿主实装全部通过，详见 [本次证据](../archive/2026-09-21-main-merge-readiness.md)。本机 daemon 从 PID 1464 受控切换至 441183；备份与实装证据保存在用户私有目录 `/home/arinp22/.local/state/patina/acceptance/20260921-merge-f8bplgw1`。
- 本地主线合入复用 `bc5c2e9c` 的代码与验收证据，后续仅同步 13 份文档；存储/Agent/API 文档契约、UTF-8、相对链接和 diff 检查通过。发布契约在宿主环境通过 25 项 policy、3 项 DEB 与 11 项安装检查脚本测试；沙箱内两次未捕获预期子进程错误输出，保留为环境限制，不记为零重试。合并没有重新构建、安装或操作生产服务。
- 用户确认后，`5eefcf4e` 已推送 main，对应 [Verify 35529435704](https://github.com/Asanilo/patina-Linux/actions/runs/35529435704) 成功；该结果只覆盖合流基线，不覆盖随后本地 beta.20 准备改动。
- beta.20 源码准备已完成，新增采样中断及旧协议扩展修复后 `release:check` 通过：56 个 TypeScript 测试文件、38 项浏览器回归、683 Rust passed / 15 ignored、Clippy、扩展签名和版本/changelog 检查。浏览器临时 profile 清理仍有非阻断警告；详细范围与限制见 [平台回流记录](2026-09-21-linux-platform-reuse.md#本批验证与限制)。本批没有新安装包、生产重启、tag 或公开资产。
- 首片提交 `e42ba3c9` 已推送 main，其 [Verify 35530683384](https://github.com/Asanilo/patina-Linux/actions/runs/35530683384) 成功。再次只读确认 beta.20 尚无远端标签或 Release，继续使用该未发布版本。
- 第二片 idle/协议消费端修复后完整门禁更新为 705 Rust passed / 15 ignored；56 个 TypeScript 文件、38 项浏览器回归、Clippy、扩展签名及版本/changelog 均通过，日志 `/tmp/patina-beta20-provider-release-check.log`。证明范围及后续扩展实机门槛见 [第二片记录](2026-09-21-linux-platform-reuse.md#第二片idle-可信性与-gnome-消费端兼容)，没有新安装、生产操作或公开资产。

## 合流后的验收基线与后续范围

以下矩阵记录 T4/T5 的验收依据；阻塞修复、完整门禁、隔离候选安装/升级、真实临时服务/应用重启、当前系统实装确认及本地主线合入均已完成。下一项为 T6 的平台成果评估；AppImage 和稳定发布门槛继续保留，不因合流增加新客户端或额外性能优化。

| 验收项与通过标准 | 当前状态、版本与证据 | 下一动作 / 重跑条件 |
| --- | --- | --- |
| 唯一 owner：同 profile 不出现第二 tracker，异常不隐式回退 | 旧 DEB 候选已通过单 owner 与回滚/接管；本批实装确认旧 PID 退出和 owner 文件锁释放后才启动候选，lease 与生产 MainPID 一致，tracking/service capability 就绪 | lease、reservation、服务启动/关闭变化时补动作前后证据；不把一次升级作为任意异常证明 |
| 客户端生命周期：退出后持续记录，重开恢复，旧 UI 不终止 daemon | 本批 libtest 验证低耗窗口销毁与四次真实 Settings 重启；正式包内 Desktop 另完成隐藏、单实例唤回、两次退出码 0 和新进程重开，退出后 15 秒心跳与成功采样均前进，daemon PID 保持 441183、NRestarts=0 | 生产短期采样通过；不替代长期使用、真实登录/挂起或视觉闪烁验收，电源处理变化另补验证 |
| 协议边界：版本/capability、认证、SSE 与降级可测，新客户端不直连 SQLite | 本轮 Desktop DB 盘点和完整门禁通过；实装认证 API、协议/版本与 capability 检查通过。修复与具名读取例外见 [本批审计](../archive/2026-09-20-daemon-owner-audit.md) | 未来新客户端另做等价协议验收，不宣称 Desktop 已完全脱离 SQLite |
| DEB 安装/升级：配套版本、受控重启、失败可恢复、卸载保留数据 | 私有 dpkg 安装→升级→卸载→重装通过；获准后实际 beta.18→候选升级、停写备份、生产 unit 重启、正式 Desktop 与真实采样均通过，安装文件 hash 和运行 binary inode 一致，见 [候选证据](../archive/2026-09-21-main-merge-readiness.md) | 实装未卸载或降级；卸载保留数据由隔离测试覆盖。依赖不代表干净发行版装依赖，不覆盖 purge 或正式 updater；旧公共签名不属于本地候选 |
| AppImage：首次接管、共存、登录及正式升级 | `14e37d9`、`2bc2865` 已有 [隔离 AppDir、预检、unit 解析与验签验证](../archive/2026-09-19-patinad-runtime-history.md#appimage-持久运行时与原子更新2026-09-19)；实机门槛仍未通过 | 持久运行时/updater/unit 变化后重跑具名隔离验证；经授权补实机矩阵后才评估恢复格式发布，不解除 DEB-only gate |
| 数据安全：统计一致，恢复事务、receipt 与失败回滚有效 | beta.19 自动门禁及 `65c1440` [多表/写入故障](../archive/2026-09-19-patinad-runtime-history.md#多表备份与写入故障补验2026-09-19未出包) 通过；本批实装前在线/停写 SQLite 备份通过校验，升级后固定历史记录摘要和 SQLx 1–8 校验和保持一致，quick_check/外键通过 | schema/备份/恢复/凭据/重启链改变时补具名专项；本轮未恢复或迁移生产数据，WebDAV 外部兼容和任意崩溃时刻不在现有证明范围 |
| 本机目录维护：保留 main 的迁移、恢复默认目录和重启清缓存 | 候选 daemon + 真实临时 systemd + 四次界面 app.restart 串联通过；数据迁出/恢复各重启一次 daemon，WebView/缓存维护保持 PID，3 次服务退出均为 0，50,000 条记录与总时长、持久偏好、备份/源目录和 journal 收尾正确；双侧恢复另有 37 项迁移专项；正式包存储页加载及控件可用另有实装证据 | 生产数据未做目录迁移或清缓存；迁移动作仍由隔离 unit/libtest 覆盖。目录选择器/确认弹窗、双侧恢复和故障中断未全部执行原生动作，不扩大既有专项证明 |

证据复用时记录比较的两个 commit、相关行为是否变化和实际环境。上表中的历史通过不自动升级成当前候选通过；归档中的 `/tmp` 文件是历史定位，未检查存在性时不得声称原始文件仍可读取。证据缺失写“待补”，不能靠旧测试总数填为完成。

AppImage 实机接管/共存/正式升级仍是恢复该格式发布和 daemon 稳定版的门槛，不是开始讨论或验证新客户端协议的无限前置任务。通用图表、分类体验、备份性能、Widget 独立入口不再扩大本阶段范围；数据丢失、安全和阻塞核心流程的故障例外。

## 隔离与人工验收规则

本批候选的存储验收需保留动作前后证据，不能仅凭测试总数勾选 T4/T5：

`test:storage-native -- --systemd --daemon <私有安装路径>` 已为下表正常迁移、数据恢复默认、取消和缓存场景提供隔离原生证据：迁移预约通过真实 IPC，缓存开关和四次重启由真实 React 按钮执行，实际服务由唯一临时 systemd unit 管理。它未操作原生目录选择器/确认弹窗，未验证生产持续采集；占用、挂载故障、双侧恢复和中断恢复另有专项回归。不能仅凭该测试宣称系统实装完成；本批实装另有上文正式包与生产 service 证据。

| 场景 | 需要核对的结果 |
| --- | --- |
| 数据目录迁移与恢复默认目录 | 从设置页预览、确认、预约并重启；daemon 受控停止和恢复，始终只有一个 tracking owner；数据库行数、关联、备份和当前路径正确，原目录保留；重开 Desktop 不重复迁移 |
| WebView 目录迁移与单独清缓存 | daemon PID/运行状态不因 WebView-only 或 cache-only 维护改变；持久偏好保留，缓存清理只影响 `WebKitCache`；只预约缓存时也能从设置页重启 |
| 预约后继续使用与取消 | 未重启前继续在原数据路径记录，daemon 正常重启仍使用原路径；取消后不迁移，不留下无法执行的预约 |
| 旧客户端占用与恢复失败 | 其他 Desktop 或残留 WebKit 占用时有界失败、不强杀；挂载不可用时不给默认目录建立替代数据库；保留需要恢复的 journal，并在错误解除后恢复，不能恢复时明确报错 |

- 自动验证使用独立 HOME/XDG/profile、端口与临时 service；先记录 baseline，再执行操作，最后复核 owner、数据完整性和清理。
- 已安装检查脚本的 `baseline / installed / managed / rolled-back / uninstalled` 是各阶段快照，不是仅跑一遍即可代替整套验收。
- UI 退出、服务崩溃、升级、回退和卸载均需操作前后证据；不能用模拟进程或临时 service 证明生产环境已通过。
- 真实登录、挂起及长期使用由用户操作；不自动挂起电脑，不因开发授权重启生产 service。
- 恢复测试使用合成数据或明确授权副本；先验证备份，再预约恢复，复核新实例与 receipt、行数/关联和失败回滚。不得自动回退迁移后的数据库版本。
- 包构建、安装、推送和公开发布是不同状态；公共发布遵循 [发布规范](../versioning-and-release-policy.md)。

## 并行方向与暂停项

- `feat/linux-desktop` 已从上游 `80204c73` 创建；平台边界、GNOME provider 与扩展仍是未完成草稿，尚未完成 Rust 集成和实机验收。当前暂停扩张，后续按 T6 评估可回流模块；本次合并未改该 worktree，不上推当前 daemon 整包差异。
- 当前不脱离 fork、不拆仓库、不重命名；上游是否合并不阻塞 daemon 产品。
- 悬浮窗闪烁/吸附保持暂停；KDE/wlroots、Flatpak、Windows 删除另行评估，不混入分离验收。
- 此文后续只更新当前状态，完成批次的长篇证据移入归档，不再累积相互覆盖的“下一步”。
