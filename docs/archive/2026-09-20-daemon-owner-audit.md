# Desktop / daemon owner 审计与本批修复

范围：2026-09-20 至 09-21 的 `feature/patinad-daemon` 工作，基于 `2193acc6` 的未提交工作树。`main` 为 `a13a64a6`，是 daemon 的祖先。当前推进状态以 [工作清单](../working/2026-07-10-patinad-runtime-design.md) 为准，本文保留本批访问盘点、修复及验证证据，不管理下一阶段优先级。

## 修复与明确限制

| 原问题 | 当前实现与 owner | 验证 |
| --- | --- | --- |
| 分类默认色、单项分类修改和读取时清理遗留分类直接写 SQLite | 全部通过 `cmd_commit_classification_settings`，由明确 embedded owner 或 daemon 原子提交；空后缀/超长损坏键只忽略，不绕过协议校验 | `desktopPersistenceOwnership.test.ts`、`classificationDraftState.test.ts` |
| settings command 缺失时根据错误文本回退 SQL | 删除两个 gateway 的 SQL fallback；命令缺失、daemon 未配置、capability 失败与锁错误保留失败 | 同上 owner 测试，覆盖失败时无本地写入 |
| 按域名删除网页历史绕过 daemon | Desktop command → typed client → 仅 tracking daemon 提供的 `POST /api/v1/data/web-domains/delete`；embedded 复用同一 data owner | 确认、认证、只读宿主拒绝、事件及删除语义回归 |
| 客户端 SQLite 重连执行迁移、repair、登录偏好回填 | 根据 `DesktopRuntimeMode` 选路；daemon-client 初始化与重连共用 existing-only 打开/校验路径；失败关闭新连接 | `existing_only_pool_*`：缺库不创建，坏 schema 不修复，元数据/偏好不回填 |
| SQL 插件仍授予不使用的 `load` 创建数据库入口 | main/widget capability 删除 `sql:allow-load`；前端仅 `Database.get` 使用 Rust 已注册池 | 全树调用搜索、capability JSON 与完整编译；`execute` 因下表具名例外仍保留 |
| runtime adapter 丢弃 Tools、提醒和定时备份事件 | 四类 runtime 事件均转发；只有 tracking 变更需要先刷新窗口/session。新 Desktop 实例从实时流开始，普通重连保留 cursor；Tools 快照串行刷新，配置切换/停止撤销旧请求 | 四类事件与旧提醒回归通过；3 条真实 HTTP 延迟响应测试覆盖顺序、配置替换及停止 |
| 客户端预约存储维护，但重启不会执行；pending 会阻止 daemon 后续启动 | 受管 Desktop 使用宿主离线维护协调器，数据迁移前停止 daemon 并取得 maintenance lease；WebView/cache 单独维护不停止 tracking；手动 daemon preview 明确不支持 | Settings、只读备份、跨进程资源屏障、lease 与持久迁移回归；整体候选门禁见文末 |

按域名删除仅匹配规范化后的完整域名；保留其他域名、原生活动与分类设置，关联记录由外键级联删除。删除当前活动段后下一次采样从新的观察时刻开始，不恢复被删除的时长。新接口缺失的旧 daemon 返回明确失败，Desktop 不回退 SQL。它是本批源码能力，已发布 beta.19 不包含这个接口。

## 前端 SQLite 完整访问清单

以下 `P/` 为 `src/platform/persistence/`。共 16 个模块；纯解析/类型出口合并列出。保留读取意味着 Desktop 仍依赖本地 schema，不等于任意新客户端已能完全通过 HTTP 替换。

| 模块 | 当前访问与 owner | 剩余边界 |
| --- | --- | --- |
| `P/sqlite.ts` | `getDB` 取得注册池；串行写适配器当前仅服务 Desktop bootstrap 缓存；重连调用模式感知 command | 批量 helper 并非 DB 事务，不能接入 runtime 业务写 |
| `P/sqliteTransactions.ts` | 注入 executor 的串行任务、失败停止；不独立打开 DB | 真实业务事务归 Rust owner |
| `P/settingsPersistence.ts` | settings/health SELECT；过期记录删除、标题清空调用维护 command | 受控本地读保留；任意 upsert 已删除 |
| `P/appSettingsStore.ts` | 设置与健康时间戳读；设置保存统一 command，host 按字段 owner 分流 | 登录启动是专用 host 操作；未使用的前端 heartbeat 写入口删除 |
| `P/classificationPersistence.ts` | 分类键、定义及 native/import executable 集合 SELECT；按 app 删除用 command | canonical 历史别名与导入语义仍依赖本地读 |
| `P/classificationSettingsGateway.ts` | 分类 mutation batch 全部提交 owner | 无错误文本触发的本地 fallback |
| `P/webActivityRepository.ts` | 网页活动、观察域名及覆盖规则 SELECT；删除走新 command | 本地时间区间、active segment 与分类读语义仍保留；无调用的 delete-before 删除 |
| `P/sessionReadRepository.ts` | icon、sessions/title samples/import exact/import buckets SELECT | 多次查询不承诺同一 snapshot；未来协议需维持 native 优先与导入合并语义 |
| `P/nativeSessionPrecedence.ts` | 纯内存区间归并 | 无 IO；不是额外 writer |
| `P/dataWebActivityTrendRepository.ts` | 网页趋势精简字段 SELECT | 受控本地读保留 |
| `P/dailyActivityRepository.ts` | `cmd_get_daily_activity` 查询 owner | 范围/响应校验，无 SQLite 回退 |
| `P/dailyAppsRepository.ts` | `cmd_get_daily_apps` 查询 owner | identity/响应预算校验，无 SQLite 回退 |
| `P/observedAppsRepository.ts` | 最近及全历史迁移观察量分别用 command | 不用最近列表替代完整迁移语义 |
| `P/dataBootstrapSnapshotStore.ts` | `data.bootstrap_snapshot` 本地读写 | **Desktop 私有可重建渲染缓存例外**，非 tracking 真值，不驱动 daemon |
| `P/remoteBackupSettingsStore.ts` | 配置 SELECT；保存/清理及路径规范化调用 app settings owner | Secret Service 凭据由本机 host 管理，不进入普通配置协议 |
| `P/activityImportRuntimeGateway.ts` | 文件选择/预览、导入提交/列表/删除均调用 Rust command | TS 无 SQL；文件选择/staging 属于 Desktop 能力，runtime 写归 owner |

生产前端原始 `executeWrite` 调用仅剩 `data.bootstrap_snapshot`。这不代表整个产品物理上只有一个 SQLite writer：下列 Rust Desktop 私有状态和 host 例外也共享该库。

## Rust 宿主访问与例外

| 范围 | 当前保护 | 结论 / 保留项 |
| --- | --- | --- |
| `command_client`、bootstrap、runtime lease | 仅 embedded 模式返回本地 owner；客户端缺配置/不可达报错；managed 启动不接管 embedded tracker | 主路径成立；不能以网络失败决定 owner |
| settings、tracking、tray | runtime 配置、分类、暂停经 typed client；登录偏好专用 host 命令 | 浏览器配置合并仍 SELECT 本地库；跨 runtime 设置端点尚非统一事务 |
| persistence / web activity | 聚合读取与维护命令先选 owner；本批修复重连与网页删除 | 保留部分本地只读模型 |
| activity import | commit/list/delete 经 daemon；preview 读 fingerprint，文件 staging 留客户端 | staging 是客户端文件能力，不拥有 runtime DB 写入 |
| backup / restore | staging ticket + daemon schedule；远端上传/恢复/计划配置经 daemon | 本机导出、用户显式测试远端及凭据管理仍由 Desktop host 执行 |
| tools | reminder/timer/pomodoro/规则写入经 daemon | alert 查看/关闭是 Desktop 展示内存；本批补事件转发 |
| widget | side/anchor 本地持久化 | **Desktop 私有偏好例外**，仍在共用物理 DB |
| updater / desktop behavior | update-check day、安装后 reopen intent 本地保存/消费 | **Desktop 生命周期状态例外**，不拥有 tracking |
| daemon service | login preference/cutover/rollback 由 host mutation mutex 串行控制 | **host 交接例外**；reservation 与系统状态对账不能强制依赖运行中的 daemon |
| storage maintenance | **本机 host 离线维护例外**：只在所有 Desktop 退出存储访问后执行，涉及 data 时先停止 daemon 并取得 runtime lease；同一 UI 流程支持取消与重启 | managed 预约只读备份、不 checkpoint；手动 daemon preview 无受管 service 控制合同。不能把普通维护入口扩成在线 runtime SQL 写入 |

后续需要独立设计的协议缺口：适合其他客户端的目录维护请求边界、等价的完整只读模型、客户端私有数据物理分离，以及有真实需求时的跨端点设置原子提交。本机目录迁移保留 Desktop 宿主确认，不作为通用 HTTP 文件系统接口。第一阶段不以“客户端零 SQLite”或增加新客户端作为完成条件，但必须明确这些限制。

## 目录维护的补齐范围

用户在本轮明确要求合并前保留 main 的目录迁移与清缓存能力，因此临时安全拒绝没有被当作任务终点。最终实现沿用既有 Settings 预览、确认、预约和稍后重启流程：

- 正常 Desktop 持有 shared storage lease；维护启动先拿 exclusive lease，资源仍被使用则有界等待并失败，不强杀旧实例。exclusive → shared 降级使用非阻塞锁，另一维护进程抢到转换窗口时明确要求重试，不让启动线程无限等待。Linux 兼容检查只检查同 UID 的相关进程/句柄/映射元数据，不读取活动库、标题或凭据内容；无关进程不可读的元数据不导致全局阻断。
- 数据目录迁移由宿主先停受管 daemon，取得临时 `Maintenance` runtime lease 后复制/校验，再释放 lease 并恢复服务。WebView-only/cache-only 不停止 daemon。客户端始终不会切回 embedded tracking。
- 普通 `pending-restart` 预约不会阻止 daemon 继续使用原路径；实际执行的 journal 存在时，新 daemon 必须拒绝启动直到恢复完成。
- 私有 journal 在文件替换和 anchor 提交之前落盘；原目录保留。默认目录的旧 WAL/SHM 等受管条目同样移入保留区，不能残留在新数据库旁。回滚中再次中断也保留恢复原件；只有确定 committed/rolled-back 后才记录结果、移除 pending，再清理 receipt。
- 手动 daemon preview 拒绝新增维护；恢复 journal 存在时拒绝覆盖或取消请求。缓存单独预约也有重启按钮。代码与隔离回归不等于已验证生产安装。

## 发布和安装事实

2026-09-20 查询确认：[beta.19](https://github.com/Asanilo/patina-Linux/releases/tag/v1.9.0-beta.19) 已公开为 prerelease，[工作流 35449507248](https://github.com/Asanilo/patina-Linux/actions/runs/35449507248) 成功。annotated tag peel 到 `fbdad8ebf169ed1235649a2c9aab3872be67ddf1`，与构建 SHA 一致。Release 的 `target_commitish=main` 不代表真实 tag 指向 main，也不证明 daemon 已合并。稳定通道仍为 `v1.8.4`。

公开资产为 DEB、`latest.json`、GNOME zip、Chromium zip 与 Firefox XPI；无 AppImage。下载的 DEB 和 manifest 摘要与 GitHub digest 一致：

- DEB SHA256：`9a8e2706451f4728ce0204286e7942410c662441206da55abe70ffb64f5850ce`。
- manifest SHA256：`c87bd1682b1cf384c607fc438b609f94704e1ae63efee678993d1f1624a6dfb7`。
- 用发布 commit 的 updater 公钥、缓存的 `minisign-verify 0.2.5` 独立验签：原件通过，修改一字节被拒绝。签名内嵌在 manifest，未假定存在独立 `.sig` 资产。
- `release:verify-daemon-deb` 对公开 DEB 通过；未安装/执行包内程序。三个扩展只核对公共存在性及 workflow 成功，本轮未再次下载验内容。

本机只读 baseline（2026-09-20 23:46:22 +08）：`dpkg` 为 `1.9.0-beta.18 amd64`；生产 unit loaded/enabled/active/running，PID 1464、NRestarts 0，运行 executable 与 `/usr/bin/patinad` 同 inode，无 deleted 标记。未读真实 DB/token，未运行会读取它们的 installed collector，也未启动程序、重启服务或安装。此快照不证明 tracking readiness 或活动连续性。

从 beta.18 `3105fc4d` 到 beta.19 `fbdad8eb`，已比对的 lease、daemon runtime、tracking engine、Linux power 与 packaging 无生产实现变化，web activity 的差异为回归测试；可保留同环境旧行为证据，不能更名为 beta.19 已安装验收。同期 AppImage、Desktop bootstrap、聚合读取和备份处理有变动，不能整体复用。`fbdad8eb` → `2193acc6` 的提交差异只有文档，本批工作树则改变运行行为，必须重新验证。

## 本批验证与交付状态

2026-09-21，本批代码冻结后的 `npm run check:full` 退出码为 0，覆盖基于 `2193acc6` 的当前未提交工作树，包括本批修复和此前协作门禁改动：

| 验证 | 结果与范围 |
| --- | --- |
| 前端与构建 | 55 个 TypeScript 测试文件通过，包含 38 项真实浏览器回归；TypeScript/Vite 构建、bundle budget、命名与架构边界通过 |
| Rust | 670 passed、0 failed、14 ignored；`cargo check`、Rust 边界检查、`cargo clippy -- -D warnings` 通过。跳过的具名环境测试继续按开发文档单独路由 |
| owner 与事件专项 | 前端 17 项持久化 owner 回归、existing-only 重连、网页删除的数据/HTTP 权限/事件验证，以及 runtime adapter 与 Tools 顺序/取消回归包含在完整门禁中 |
| main schema 升级 | 使用 `a13a64a` 的实际 v1–6 SQL、固定 SQLx 校验元数据和 16 表合成记录；通过真实 prepared open 升级到本分支，重复打开仍保留字段/关联和迁移记录，且新增触发器与 receipt 可用 |
| 目录维护专项 | `storage_` 冻结前专项 57 项通过，后补占用屏障/降级争抢专项 9 项通过；最终全部纳入上面的 670 项。覆盖只读备份、无 checkpoint 预约、lease 释放、旧 WAL/SHM、anchor 中断、回滚再次中断及恢复记录保留 |

主日志为 `/tmp/patina-daemon-closure-check.log`（临时文件，不承诺长期保留）。首次受限执行被跨时区 Node 子进程的沙箱 EPERM 中止，获准重跑后完整通过；没有删减测试规避问题。真实浏览器用例全部通过，但临时 profile 清理输出 ENOTEMPTY 非阻断警告，因此不是零警告运行。14 个 Markdown 文件的 UTF-8、相对链接和最终 diff 空白检查也通过。

迁移断点使用合成目录、数据库和故障注入来模拟状态丢失；资源锁另有真实子进程退出/争抢测试。没有执行硬断电，不能据此声称所有文件系统和任意断电时刻都已实测；外部挂载缺失时保留恢复信息并要求恢复路径。

隔离验证与生产验收分开：默认 `check:full` 覆盖本批代码；main schema 升级使用合成数据。真实候选安装、Desktop 关闭/重开、登录/挂起、生产故障/回退需要适用的动作前后证据。AppImage 首次接管、DEB 共存、登录和正式更新仍未完成，不解除该格式与 daemon 稳定发布门槛。

本批不修改版本，不提交/推送、合并 main、构建新安装包或变更生产服务。贡献草稿 `feat/linux-desktop` 继续冻结，不能整体合并其迁移或上游历史。

## 原生续验（2026-09-21）

用户随后授权继续推进。本轮增加 `npm run test:storage-native`，以私有 HOME/XDG/D-Bus、合成数据库和独立 daemon 验证真实 Desktop bootstrap、React、IPC 与文件维护。服务管理器是仅控制自家子进程的 D-Bus fixture；它不连接生产 user manager，不能替代真实 systemd/安装验收。测试宿主和 daemon 使用不同 executable 路径，以免旧客户端兼容屏障把测试宿主当作第二个 Desktop。

首先完成既有 `npm run perf:heatmap-desktop`：`/tmp/patina-heatmap-test-guymQ5/result.json` 为 exit 0、passed true；两轮真实 UI 均无页面错误或溢出。正常关闭、等待 310 秒销毁 Main、重新打开后，50,000 条合成 session、3,000,000,000 ms 总时长、quick_check 和外键完整性保持正确。test binary SHA256 为 `139a69920b9a6c8d89d6a8a0d5b339e139829029e56cf25d5f87d11ff53176b4`。这证明私有 preview 环境下的真实客户端重开，不证明持续采集真实活动；fixture 的 tracking 保持暂停。

首轮存储原生验收 `/tmp/patina-storage-test-G71J6w` 发现“数据迁出后无法恢复默认目录”：默认位置仍是活动 WebView 根，被通用重叠校验误拒绝。`plan::validate_target_relationships` 现只对目标、稳定默认根和另一类活动根三者精确相同放行；控制目录、本类源目录、自定义重叠、父子重叠、叶符号链接及 canonical 源/控制路径冲突仍拒绝。预览、预约和执行复用这一规则。4 个新增回归覆盖双向恢复、拒绝边界、路径别名以及恢复时保留另一类文件/API token/原目录；迁移专项最终 34 项通过。

该修复后的完整 `npm run check:full` 再次退出 0：55 个 TypeScript 文件、38 项浏览器回归、674 passed / 15 ignored 的 Rust 测试，构建、预算、边界与 Clippy 通过。新增第 15 项 ignored 是具名的原生存储 worker，需由 runner 单独调用。日志 `/tmp/patina-daemon-native-check.log`；浏览器临时 profile 清理仍有 ENOTEMPTY 非阻断警告。原生 GTK/portal 的隔离环境诊断消息不作为零警告或正式桌面集成验收的证据。

修复后的第二轮 `/tmp/patina-storage-test-DJVI5c` 五阶段均通过，但最终断言错误地省略了规范化 WebView 目标的 `webview` 子目录，故不计作整轮成功。修正测试，使最终路径与预约实际返回的目标一致；生产实现未再改变。最终 `npm run test:storage-native` 在 `/tmp/patina-storage-test-aoEjAu` 退出 0，`result.json` 与 `evidence.json` 均为 passed true，收尾验证通过，日志为 `/tmp/patina-storage-native-acceptance-complete.log`。

| 阶段 | 真实动作和证据 |
| --- | --- |
| 迁出预约与取消 | 真实 Settings 渲染，IPC 预览/预约、取消再预约；当前数据根仍为默认目录，daemon PID 361146 保持不变 |
| 执行迁出并预约恢复 | 新 Desktop bootstrap 执行迁移，daemon 受控停止/启动，PID 361146 → 361789；IPC 预约恢复默认目录 |
| 执行恢复并预约 WebView 迁移 | 数据恢复默认位置，daemon PID 361789 → 362150；IPC 预约 WebView 迁移 |
| 执行 WebView 迁移并预约清缓存 | WebView 路径与预约相同，localStorage 保留；操作真实缓存开关，确认重启按钮可见且可用，daemon PID 362150 不变 |
| 执行缓存清理及最终复核 | 合成缓存 marker 消失，持久偏好保留，清理标志归 false 且时间戳存在，daemon PID 362150 不变；最终 fixture 清理为 pid null、start_count 3、stop_count 3 |

每一阶段均无页面错误，无 embedded tracking lease；认证后的 capability 确认 daemon 为 tracking owner 且 ready。50,000 条合成 session、3,000,000,000 ms 总时长、quick_check 与外键校验保持正确。最终数据根恢复默认位置、原迁出库及备份保留、pending 与 journal 均清除，关闭 fixture 后 maintenance lease 可取得。

最终构建 SHA256：libtest `7966ee071644b7e52758073547ad77e64e51999430b3a1a794ea10a7c1124ff4`；独立 daemon `6b2da6a92cae5ec735872ea389abab759edafc966120c1b0ee116aff643f44c0`；frontend index `b80917d1b2f70e6b4f32801ecff6088035f01c8c7ce152e612f546749743b608`。`/tmp` 路径仅用于本次证据定位，不承诺长期保留。

迁移预约直接调用真实 IPC，未操作原生目录选择器或迁移确认弹窗；退出/重开由 supervisor 执行，未点击应用的 `app.restart` 动作。WebView 恢复默认目录、旧客户端占用与故障恢复不在这五阶段的原生覆盖中，不能把对应专项回归称为原生验收。服务管理器为私有 fixture，tracking 暂停；这些结果不证明真实 systemd、生产活动连续性、安装/升级、登录/挂起或用户观察到的视觉表现。当前 Todo 中安装候选验收与 main 合并仍未勾选，生产服务和安装版本未变更。

收尾检查：14 个改动 Markdown 文件的 UTF-8 与本地链接目标、T1–T6 唯一性、存储/API 文档契约、两份原生 runner JS 语法及 diff 空白检查通过。31 个新增或改动 Rust 文件的格式检查通过；整库 `cargo fmt --check` 另提示 `app/daemon_service.rs`、`commands/backup.rs`、`platform/linux/mod.rs` 的既有格式差异，三文件均与 HEAD 字节一致，本批未顺带改写。完整门禁后只有测试收尾断言、模块声明排序与文档调整，最终 native runner 已重新编译并执行修正后的测试。
