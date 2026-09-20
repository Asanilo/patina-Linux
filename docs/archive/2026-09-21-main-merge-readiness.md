# daemon 合并前候选验收（2026-09-21）

用户要求继续完成合入 main 前的工作。本批仍在 `feature/patinad-daemon`，基准为 `2193acc6`；当前推进顺序与完成状态只由 [工作清单](../working/2026-07-10-patinad-runtime-design.md) 管理。main `a13a64a6` 是该基准的祖先，`HEAD..main` 无新增提交，无需反向同步。上游贡献草稿保持冻结。

## 双目录恢复缺陷

复核发现，同一预约可以把 data 和 WebView 从各自自定义目录一起恢复到默认共享目录。旧 executor 根据目标路径判断数据种类，第二次 WebView promotion 会被误判成 data，把刚迁入的数据库隔离走，并可能复用数据隔离目录。

`PromotionKind` 现在随 journal 显式保存，提升、文件白名单、隔离目录与恢复验证均按 Data/Webview 分类；共享默认目标合法，但同类重复 promotion 仍拒绝。该 journal 尚未发布，缺少种类的旧实验记录明确拒绝，不推测恢复。

新增三项回归验证双侧累加预约成功、第二 anchor 失败后回滚并重试，以及六个中断位置。核对数据库行数/完整性、两侧备份与 WebView、token、源目录和保留原件。迁移专项 37 项通过。

## 候选身份与完整门禁

本批 `npm run check:full` 退出 0：55 个 TypeScript 文件、38 项浏览器回归、677 passed / 15 ignored 的 Rust 测试；构建、bundle budget、命名/架构/Rust 边界与 Clippy 通过。日志 `/tmp/patina-main-readiness-check.log`。15 项环境专项默认跳过，按各自 opt-in 路由单独执行；本批执行项见下文，不宣称 15 项全部重跑。

为本次用户明确要求的合并前安装验收，构建了本地未签名 DEB，未改变版本文件或公开发布状态。该包保持源码版本 `1.9.0-beta.19`，但**不是已公开的 beta.19 发布资产**，身份以以下 hash 和源码清单区分：

- 包：`/tmp/patina-merge-candidate-z2kcgjds/Patina_1.9.0-beta.19_amd64.deb`。
- SHA256：`40f65c6398f992ca38eaf4f55fcdef7df33b0712cdb107ffc7642967a977da77`。
- 基准与文件 SHA256 清单：同目录 `source-manifest.json`；是基于 `2193acc6` 的本批工作树，不能把基准 commit 单独当成候选内容。
- 构建使用 `createUpdaterArtifacts=false`，未签名，不得上传为 updater 资产；包结构、默认未启用的 unit、扩展 UUID 与元数据检查通过。

隔离升级基线选用本地 `Patina_1.9.0-beta.18_amd64.deb`。只读核对证明其 Desktop、daemon 和 unit 与当时实际安装的三个文件 SHA256 一致：

| 文件 | SHA256 |
| --- | --- |
| `/usr/bin/Patina` | `eb0d6fec97255ef693f9e15c2aca637e4937de747a1f7d8a2794c87969edf1c7` |
| `/usr/bin/patinad` | `a4ac4057a08329e3c04206a74aa898d4ca04cd30e099a9ff92da94bf627022c7` |
| `/usr/lib/systemd/user/patinad.service` | `789a578e3829dcf7d76d6b6dfee3347201e3007860393776ce3f97f9a4ef37a4` |

这证明本次基线与当时已安装软件文件一致，不自动等同于上游或 main 的正式发布原包。该基线文件核对阶段未读真实数据库、token 或活动内容；后续授权实装的数据校验另见下文。

## 验证方法与边界

`test:deb-isolated` 使用用户、挂载、PID、网络命名空间和私有 dpkg root/database/log。两包 control 仅允许 control/md5sums，拒绝维护脚本、触发器、非预期载荷与自动启用服务。依赖使用宿主已安装版本的元数据副本执行正常 dpkg 检查，不能声称已验证干净发行版的依赖安装。

`test:storage-native -- --systemd --daemon <私有安装路径>` 使用候选包的实际 daemon 和同源码 Desktop libtest 宿主。私有 D-Bus 只把固定产品名映射到本轮唯一随机 unit；真实 manager 执行启动和停止，`systemd-run --wait` 保存退出码，外层再次核对该 unit 已回收。它不控制正式 `patinad.service`。四次后继 Desktop 必须来自真实 Settings 重启按钮 → IPC → `app.restart`，runner 只启动第一轮。

真实 systemd 测试 unit 使用 `Type=exec`、`Restart=no`、`PrivateTmp=no`；包内完整 hardening、实际登录/挂起、持续真实活动采集与签名升级仍不是此测试的结论。Desktop 是测试宿主，目录预约通过真实 IPC，未自动操作原生目录选择器或确认弹窗。源码合并与生产安装、正式发布分别记录，不将 AppImage 发布矩阵扩大为本次源码合并门槛。

## 实际执行结果

隔离 dpkg：`/tmp/patina-deb-acceptance-zwcwa_sp/evidence.json` passed true，四阶段全部通过。升级基线包 SHA256 为 `1e20c36c22ca8850c73294fc1d15cb4787200a4287d4a0ba7d863654b32a4e6e`，候选 hash 与上文一致。正常依赖检查使用 3,987 个宿主已安装包元数据副本，没有使用 force-depends；不复制或执行宿主维护脚本/trigger。最终保留的候选 daemon 位于该目录 `root/usr/bin/patinad`。

原生串联：`/tmp/patina-storage-test-BKWirs/result.json` code 0、passed true，无超时/中断；五阶段均通过，`cleanup.json` 无错误。独立临时 unit 为 `patina-storage-test-BKWirs.service`，最终 LoadState=not-found。三个 `systemd-run --wait` 观察到的退出码均为 0；start_count=3、stop_count=3、最终 pid=null。

| 阶段 | Desktop PID | daemon PID 与效果 |
| --- | --- | --- |
| 预约迁出 | 417549 | 417507，取消并重新预约，原路径继续有效 |
| 执行迁出并预约恢复 | 418411 | 418501，第一次受控重启 |
| 执行恢复并预约 WebView 迁移 | 419132 | 419258，第二次受控重启 |
| 执行 WebView 迁移并预约缓存 | 419847 | 419258，服务保持运行 |
| 清理缓存后复核 | 420297 | 419258，服务保持运行 |

后四个 Desktop 均经页面按钮调用真实 `app.restart` 创建，留有 Tauri restart exit、前一 PID、启动时间与进程组证据；Node 没有代替启动后继进程。最终 50,000 条合成 session、3,000,000,000 ms、quick_check 和外键校验正确；localStorage 保留，缓存 marker 删除，预约及 journal 清理，原目录/备份保留。

构建摘要：Desktop libtest `5b519ca3adcae09ad0d4e88bc82aefe36c9541490b12357f53ace3829a92cc8a`；候选 daemon `e6d63ae4382ad002bcd62328ac457fa03463a5bbb94a7fad59590fcb7a74251d`；frontend index `b80917d1b2f70e6b4f32801ecff6088035f01c8c7ce152e612f546749743b608`。daemon hash 与私有 dpkg 安装清单相同。原生日志含隔离 GTK/portal 诊断，不能宣称零警告或视觉验收；日志 `/tmp/patina-candidate-native-systemd.log`。

包 runner 自身用 true-only 合成包通过正常四阶段、拒绝维护脚本和缺依赖失败传播三项回归；测试过程不执行真实 Patina。源码完整门禁已覆盖本批生产修复；新增 opt-in 入口另行按上述证据执行。

## 授权后的本机实装

用户对上文 SHA256 对应候选明确选择“授权本机实装验收”。所有备份、生产快照与辅助脚本保存在 `/home/arinp22/.local/state/patina/acceptance/20260921-merge-f8bplgw1`，目录权限 0700、文件 0600；它们包含私有数据，不进入 Git。`acceptance-summary.json` 是该次实装汇总。

- 安装前：beta.18 managed 基线通过；确认真实 anchor、无未完成的迁移/缓存/恢复预约。SQLite backup API 建立在线备份，并归档控制文件与持久 WebView 文件；不复制可再生缓存或既有备份目录。
- 安装：通过系统管理员授权执行 `dpkg --install`，退出 0。安装期间旧 daemon 继续运行；候选两二进制及 unit 的 SHA256 与包清单一致后，再受控停止正式 service。确认旧 PID 1464 消失、MainPID=0，并获得 owner 文件排他锁后，建立停写 SQLite 备份；备份完整性、外键及 SQLx 记录均正常，才启动新 daemon。
- 服务：新 PID 441183，运行文件 inode 对应已安装 `/usr/bin/patinad`；unit 为包内正式配置，`PrivateTmp=yes`、`ProtectSystem=strict`、`NoNewPrivileges=yes`，`NeedDaemonReload=no`。整个 Desktop 验收中 PID 不变、NRestarts=0，lease 与 service PID 一致，认证 API、tracking 和 managed service capability 就绪。
- 正式 Desktop：首实例 PID 442677，真实设置页存储内容已加载、缓存控件可见且可用；关闭窗口后隐藏，再执行已安装二进制唤回同一实例。通过归属该 PID 的托盘正常退出动作退出，进程码 0；无 Desktop 的 15 秒中，后台 heartbeat 与 successful-sample 时间戳均前进 15,358 ms。随后新 Desktop PID 480385 重开，存储页仍正常；第二次正常退出码 0，结束时恢复为仅 daemon 在后台运行。未切换用户设置。
- 数据：升级后及最终各复核一次固定历史范围；52,248 条已闭合 session、74,556 条已闭合网页 segment、119,055 条关联标题样本及导入事实摘要均与停写备份一致。活动行和后来新增记录单独允许变化；SQLx v1–8 的成功状态/校验和保持一致，quick_check 与外键检查通过。未恢复、降级数据库，也未在真实数据上执行目录迁移或清缓存。

正式 Desktop SHA256 为 `655d9f04c0f9fd43c1ce3d03fa90087ef1073f88fa130d7ee306d55e82298097`；daemon 为上文 `e6d63ae4…251d`；unit 内容与 beta.18 相同。最终 `final-managed.json` 的 15 项检查全部通过，`final-comparison.json` 保留历史摘要。首次最终快照 `after-ui.json` 在与历史摘要读取并行时收到 SQLite `database is locked (5)`，quickCheck 未取得结果；历史检查本身通过，随后串行快照通过。失败快照保留，不记为零重试。

AT-SPI 辅助脚本曾因过期节点、WebKit 的 `press` 动作名与文本层级识别失败；修正探测后七个正式 UI 阶段通过。托盘退出按 D-Bus 连接 PID 归属定位菜单；未改变全局无障碍设置。此验收证明正式包生命周期、生产 unit 和短期采样，不等同于人工视觉、长期运行、登录/挂起或生产目录迁移动作。

## 合入准备状态

本批代码、完整门禁、隔离候选和授权实装已收口；Unreleased 已记录用户可感知修复，保持原版本与旧发布记录。生产源码逐文件仍与候选 `source-manifest.json` 相同，构建后的新增差异只涉及验收脚本、文档和脚本入口，不需要为记录更新重建安装包。

main 仍为 `a13a64a6`，无冲突或新增主线提交；固定当前批次后可快进整合并保留既有开发与 beta 历史。实际合入时需同步长期文档的主线归属及 T5；当前尚未合并、推送、打 tag 或公开发布。AppImage 实机及稳定发布门槛保留，后续平台回流由 T6 管理。`/tmp` 中隔离测试证据与候选不承诺长期保留；实装备份位于上述持久私有目录。
