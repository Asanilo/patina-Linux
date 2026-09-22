# Linux 平台草稿评估与回流

状态：T6 首片/第二片、扩展生产端第三片及 daemon 会话识别第四片已实现并通过源码验证；第四片及 AppImage 首次启动修复已通过隔离验收；2026-09-22 本机已安装 beta.20 DEB，并完成真实 AppImage/DEB 共存与后台持续记录检查，新版本实际注销/登录后的会话识别和无界面采样已通过；真实系统重启后的后台自动启动也已验证。version 4 双协议、锁屏/overview 与生命周期已通过本机独立 GNOME 42.9 会话验收；AppImage 本地候选与隔离验收通过，完整实装矩阵仍待补齐。生产会话切换、ESM/更多 Shell 和正式 AppImage 发布门槛仍分开管理。首片 beta.20 准备提交 `e42ba3c9` 的远端 Verify 已通过；beta.20 继续保持未发布，不打 tag 或公开资产。产品主线为 `main`，评估基准为 `5eefcf4e`；贡献草稿位于 `feat/linux-desktop`，基于上游 `80204c73`，另含未提交改动。草稿没有完成整体 Rust 集成或实机验收，不视为可直接合入的实现。

本轮按用户确认的顺序推进主线远端同步、平台模块评估和下一版 beta 准备。分支与产品范围遵循 [路线](../roadmap-and-prioritization.md#linux-main-and-daemon-experiment)，主 Todo 与发布证据由 [当前清单](2026-07-10-patinad-runtime-design.md) 管理。保持贡献 worktree 原状，不整支合并。2026-09-22 用户授权继续扩展生产端及实机/AppImage 验收；本轮先在私有 HOME、D-Bus 和独立 Wayland Shell 中验证候选；随后用户单独授权备份安装扩展，记录见第三片。该阶段未变更生产服务与数据库；后续用户要求继续完成实装，安装及备份记录见本文末尾。

## 模块判断

| 模块 | 判定 | 主线落点与条件 |
| --- | --- | --- |
| GNOME 采样状态：窗口、无窗口、锁屏、不可用、未知 idle | 复用状态设计与合成场景，重写集成 | `platform/linux/foreground` 负责观测，`engine/tracking` 负责计时；保留主线现有 X11 支持与 daemon owner，不复制整套平台门面 |
| 采样中断与恢复 | 已在 main 内修复，草稿不能直接回流 | 草稿 tracking 主要改动是 import 路由，poller 尚未接通新的 Linux 接口；main 的首片已实现并验证“恢复不补记未知间隙” |
| GNOME 扩展的锁屏/overview、字段边界与 disable 清理 | 复用纯逻辑及测试场景 | 主线旧协议、旧客户端与已安装扩展必须兼容；JS 单元通过不代表真实 Shell 行为已验收 |
| `WindowTracker1.GetSnapshot` 协议 | 主线双协议生产/消费端已实现 | main 优先识别新接口，仅名称无 owner 时回退旧 `GetFocusedWindow`；version 4 同时保留旧方法/信号，不能让同 UUID 升级使旧客户端失联 |
| 扩展 legacy/ESM 双入口与构建 | 部分复用，需重新接入打包/验证 | 草稿 legacy42/ESM45 使用共享 JS 和 XML；主线当前只复制两个文件，直接复制会漏依赖。更多 Shell 版本需各自真实验证 |
| 上游 Windows consumer 路由、通用平台层重排 | 暂缓 | 为上游当前边界服务，不能作为 fork 修复的前置重构；Windows 冻结策略不变 |
| tray/widget、KDE/wlroots、发行版与包格式扩展 | 暂缓 | 不属于本轮计时正确性修复；支持声明按独立验证矩阵决定 |
| 扩展独立仓库、GPUI/TUI/浏览器客户端 | 暂缓 | 不因主线合流自动启动新仓库或多客户端实现 |

## 已核对的兼容事实

- main 评估基线 `5eefcf4e`：扩展 UUID `patina-window-tracker@patina`，metadata version 2，GNOME Shell 42；旧 D-Bus 名、路径和接口均为 `WindowTracker`，`GetFocusedWindow` 返回五元组，现有 Rust provider 与诊断依赖旧接口。本轮小修升至 version 3，接口保持不变。
- 草稿：相同 UUID、metadata version 3；legacy42 与 ESM45 都仅导出 `WindowTracker1.GetSnapshot`。同 UUID 不代表协议兼容；现有打包脚本只复制 `extension.js` 与 `metadata.json`，草稿还依赖共享 tracker 与协议 XML。
- 草稿 JS 入口 `node integrations/gnome-extension/tracker.test.mjs` 已通过 7 项纯逻辑/构建测试，临时输出在 `/tmp`。未安装扩展，未采集用户活动，未证明 GNOME 42/45 实机通过；整体 Rust 集成仍待完成。
- 评估基线 `5eefcf4e` 的扩展原有两个缺陷：每次 `GetFocusedWindow` 查询会把真实标题/app 写入 Shell 日志；`FocusedWindowChanged` 构造 `(sstut)`，与声明及 RPC 的 `(sssut)` 不一致。本轮已完成下述兼容小修。合成 GLib 检查确认原信号的窗口类字符串会被错误处理；目前 Rust 只轮询方法，未发现该信号的产品消费者，影响范围不扩大为当前计时故障。

## 第一片：采样失败不能跨越未知时间

### 触发与范围

评估基线的窗口 poll 超时/任务失败会产生 unsuccessful outcome，但 tracking loop 跳过该轮并保留活动会话；超过 watchdog 阈值才会结算。较短中断后恢复同一窗口可能把未知时间继续计入原会话。

本片只修改 `engine/tracking` 现有会话和运行时 owner：在首个失败样本后按最后一次可信采样结算，恢复后从恢复时刻重新记录。保留 provider 和扩展协议；未知 idle 错当 0 是另一片，不在本片伪称已解决。

### 所有权与失败恢复

- 会话结算继续由 engine 调用 data port 执行，复用已有运行时锁、事件和数据边界；不让平台 provider 直接写数据库。
- 结算边界取 engine 接受的最近成功采样时间，不用故障发现或恢复时间补齐未知间隙。
- 结算失败保留待处理边界，后续迭代先重试；成功前不能开启恢复会话或推进成功采样时戳。
- 待处理边界保存在现有 engine 内部 runtime state；power/shutdown 的结算入口也必须采用更早边界，覆盖失败后外部生命周期事件先取得锁的交错。主要涉及 `runtime`、`session_timeout`、`runtime_snapshot` 和既有 `power_lifecycle`；API 暂停入口同样会直接结算，必须薄调用同一边界能力，不增加对外字段或新客户端协议。daemon shutdown 继续使用未被提前推进的 health 成功采样时间。
- 清理跨样本的连续性与持续参与状态，避免恢复时重接旧会话；网页片段、重复失败、暂停/锁屏/关闭等路径需检查是否复用同一有效边界。

### 验收与退出条件

1. 合成数据库回放：短于 watchdog 阈值的失败后恢复同应用，旧会话在最后可信时刻结束，新会话从恢复时刻开始，未知间隙不计时。
2. 连续失败、首次采样就失败、恢复为不同窗口与无窗口，均无重复封口或补记。
3. 注入结算失败后重试，边界不漂移；成功前不接纳恢复写入，成功后只恢复一次。
4. 核对网页事件/片段和关闭边界，避免 native 与 web 数据不一致。
5. 命中专项与 `check:full` 通过后才计为实现完成；不把合成测试称为生产断网、锁屏或扩展重载实机验收。

### 实现落点

主循环在 engine 接受成功样本后保存可信时间；失败时清空跨样本连续性，把固定结算边界交给 runtime state 保存。结算成功才清除该边界并接纳恢复样本，`session-ended-probe-failure` 事件沿现有刷新通道传播。结算失败期间运行状态保持 inactive，失败诊断复用已经过滤的窗口信息，避免重新带入禁止采集的标题。

API 暂停和 power stop 在同一 transition lock 下采用更早的待结算边界；shutdown 会尝试完成遗留结算。数据层继续复用既有有界结算与关联网页片段处理，没有新增 schema、公开协议或 provider 层写入。

## 后续片段

- idle 可用性与新接口检测/字段校验已完成第二片；扩展生产端的双协议兼容、锁屏/overview 与 disable 清理已在第三片完成。
- GNOME 42 包与独立 Shell 已验证，生产扩展经后续明确授权备份安装；ESM/更多 Shell 版本仍分开验证，不把消费端合成测试当成桌面支持证明。
- 下一 beta 的范围和版本准备以最终已验证差异为准；不会复用公开 beta.19 资产或提前解除 AppImage 门槛。打 tag、公开发布与本地源码/门禁分别记录。

## 第二片：idle 可信性与 GNOME 消费端兼容

### 边界与兼容决策

本片以 `e42ba3c9` 为基准。`platform/linux/foreground` 继续拥有外部观测，在其相邻子模块 `idle.rs` 与 `gnome.rs` 分别隔离 idle 来源选择和 D-Bus 协议。对 engine 返回 `Result<WindowInfo, ForegroundProbeError>`，错误仅携带固定诊断码；poller 复用既有失败状态与首片结算能力，保留 `WindowInfo`、运行时 JSON 字段和 Windows 路径的现有形状。没有新增共享平台门面或数据库 owner。

- Wayland 只接受 Mutter 的可信 idle；失败返回 `linux-idle-unavailable`，不查询 X11/XWayland。X11 仍可从 Mutter 回退 XScreenSaver，后者查询当前 screen 的 root drawable；两者都失败时不生成 0。真实的 0 毫秒保留为正常值，未知 session 不猜测来源。
- 新协议 `WindowTracker1.GetSnapshot` 只有在 D-Bus 明确报告新名称没有 owner 时才回退旧 `WindowTracker.GetFocusedWindow`。调用绑定查询得到的 unique owner；owner 消失、权限拒绝、超时、错误方法或非法响应均失败，不转成正常无窗口，也不继续切换协议。
- 验证消息签名/大小、协议版本/状态、UTF-8 字节长度、NUL 与窗口身份后才读取进程元数据。保持 `/proc` 名称优先和旧 app key；新协议 desktop ID 的 `.desktop` 后缀在 fallback identity 中去除。确有窗口但身份无法解析时返回失败。
- 新协议正常无窗口/overview 与明确锁屏只接受全空事实；现有消费端均投影为无活动窗口，不向 logind 注入锁定事件，也不虚造 idle。仍需可信 idle 才报告成功；锁屏时 idle 也失效则保守按最后可信样本结算。未知状态或 unavailable 状态不是正常空窗口。
- GNOME X11 只有明确不存在 provider 时才用 X11 焦点查询，不能用它覆盖已知空窗口或错误协议；非 GNOME X11 直接使用 X11。X11 连接、焦点查询或窗口身份失败不再伪装成正常空桌面。
- 本片只升级消费端；现有 version 3 旧扩展、UUID、安装与打包保持不变。不因此宣称 GNOME 45/其他 Shell 或更多发行版已支持。扩展双协议发布、锁屏/overview 生产端完善、ESM 打包和真实 Shell 验收另行推进。

### 本片验证结果

纯注入测试覆盖 idle 失败/真实 0、Wayland 禁止 X11、X11 fallback、GNOME 新旧 owner 选择、异常不降级、签名/字段边界和身份失败；真实 poller 的 provider error 已接入 tracking loop 合成数据库回放，验证 native/title/web 在可信边界结束且恢复不补记。

- 平台专项通过后补充了不完整桌面标签的能力检测回归；50 项 runtime 专项通过，最终完整门禁覆盖全部改动。
- `npm run release:check` 通过：56 个 TypeScript 测试文件、38 项浏览器回归、705 Rust passed / 15 ignored、构建/预算/边界/Clippy、扩展检查及 Firefox 签名、版本与 changelog。日志：`/tmp/patina-beta20-provider-release-check.log`。仍有浏览器临时目录清理的非阻断 ENOTEMPTY 警告。
- 首次专项编译发现测试构造使用了不同 zbus 版本的方法名，已按仓库锁定的 zbus 4 修正；未升级依赖。Rust 格式与 diff 检查通过，贡献草稿 52 个文件的原 SHA256 保持不变。
- 未读取生产焦点、安装/启用扩展、重启生产服务、操作真实数据库、构建安装包或发布。新协议与 X11 行为仍是合成验证，不能扩大真实 Shell/发行版支持范围。

## 并行小修：保持旧协议的扩展修正

仅删除活动标题/app 的 Shell 日志，并将信号构造改为原声明的 `(sssut)`；保持 UUID、旧方法和信号名、GNOME 42 支持范围及窗口身份规则。扩展因实际修复独立升至 version 3，与草稿中同为 version 3 的新协议不是同一实现，不能混用资产。合成测试检查日志隐私、RPC/信号字段与实际发射 payload；本轮不安装或启用扩展。

限长、锁屏/overview 状态、新协议协商和生命周期重构保持后续片段，避免把兼容小修扩成协议替换。

## 下一版 beta 准备范围

远端已核对：最新公开预发布 `v1.9.0-beta.19` 的标签提交为 `fbdad8eb`，尚无 beta.20 标签。计划准备 `1.9.0-beta.20`，涵盖已合入的后台目录维护、runtime 写侧/事件修复及本轮通过验证的两项小修；仅 DEB 与既有扩展资产，不新增桌面/发行版承诺。

版本文件与 changelog 已同步为 beta.20，完整 `release:check` 已通过；准备提交、推送 main、推 tag 和公开发布分别记录。现有已安装本地 beta.19 候选本轮未替换。

## 本批验证与限制

本节保留首片 `e42ba3c9` 的证据；第二片以其上方的最新验证记录为准。

- 新增 6 项真实 tracking loop 合成回归，通过 native/title/web 的中断与恢复、重复失败、首次失败、无窗口、结算失败重试、暂停、锁屏与 shutdown 交错；另核对既有“旧截止时刻不关闭较新会话”和 power stop 幂等回归。失败快照仍保留标题过滤。补充诊断断言时曾有一次夹具键名错误，按既有应用名规范化修正后通过，未改变产品设置规则。
- 完整 `npm run release:check` 通过：56 个 TypeScript 测试文件、38 项浏览器回归、683 Rust passed / 15 ignored，以及构建、预算、边界、Clippy、GNOME/Chromium 检查、Firefox 签名 XPI、版本与 changelog 校验。日志：`/tmp/patina-beta20-release-check.log`。浏览器临时 profile 清理出现非阻断 ENOTEMPTY 警告，不记为零警告运行。
- 独立只读源码复核未发现阻断项；改动 Rust 文件的格式检查和 diff 检查通过。贡献草稿的基准与 52 个文件 SHA256 保持原样。
- 本批没有构建 beta.20 DEB、安装或切换 GNOME 扩展、操作生产服务/数据库、打 tag 或公开发布。上述自动回归不代表真实采样故障、锁屏、扩展重载或 beta.20 安装包已做实机验收；前一候选证据不能自动覆盖本次新增行为。

## 进度

- [x] 核对主线、草稿基准及现有分支职责。
- [x] 完成可复用／需重写／暂缓清单，运行草稿纯 JS 测试。
- [x] 第一片的采样中断修复及合成回归。
- [x] 第一片完整门禁与源码交付记录。
- [x] 旧协议扩展小修及合成回归：新增 3 项测试与现有 4 项脚本测试、扩展源码检查通过；未安装或启用扩展。
- [x] beta.20 版本、changelog 与发布门禁准备；尚未公开发布。
- [x] idle 可用性、GNOME 新旧协议消费端兼容及完整门禁。
- [x] 扩展生产端双协议兼容、锁屏/overview 与 disable 清理；9 项合成回归与独立真实 GNOME 42.9 会话通过。
- [ ] legacy/ESM 打包与逐 Shell 实机验证；不直接替换已安装扩展。

## 第三片：扩展生产端与本地候选验收

- 所有权仍在 GNOME 扩展：只观测，不计时、不写数据库。保持两个文件的 GNOME 42 包结构；ESM/更多 Shell 版本单列验证，不作为本轮 GNOME 42 修复前置条件。
- version 4 同时持有旧 `WindowTracker` 与新 `WindowTracker1`。旧五元组/信号保持兼容；新快照提供版本、状态、带后缀 desktop ID 和十进制窗口 ID，身份沿用 `get_id()`。
- 锁屏/屏幕遮蔽与 overview 在读取焦点前处理；字段按 UTF-8 字节限长，无效数值和读取异常返回 unavailable，日志不携带窗口或异常文本。无 screenShield 的会话仍可正常观测。
- 明确启用 user/unlock-dialog 两种 session mode；锁屏只提供空事实。enable/disable 幂等，所有名称、导出对象、信号、定时器按生命周期清理；旧生命周期回调不能污染新实例。名称丢失只清理对应接口，可在重新取得名称后恢复。
- 验收顺序：合成回归 → 打包扩展的独立真实 GNOME 42 会话 → AppImage 本地候选、持久 AppDir 与临时 systemd 单元 → 完整发布检查。真实用户认证、注销/登录、睡眠和正式签名升级不以夹具替代。
- 私有 Shell 用独立总线承接 session/system 两种地址，禁止服务自动激活；GDM Version 属性仅用于开启真实 Shell 的 screenShield。测试锁屏不连接生产 GDM/logind，不证明密码认证或生产登录恢复。

### 第三片验证证据

- 最终扩展源码 SHA256：`57ec5449747f929d68322e79339c3a4ba35d7e3cfbc5e85629f9f17452b0e8db`。包仍为两个文件，独立 version 4；应用版本保持尚未公开的 beta.20。
- `scripts/gnome-shell-acceptance.py` 对构建目录中的候选执行真实 Shell/GTK/D-Bus 调用。最终结果：`/tmp/patina-gnome-acceptance-i0m7wl7s/result.json`，包含 Shell 42.9、候选哈希、两个协议、overview/锁屏恢复和三轮 disable/enable；每次 disable 后两个名称均无 owner。
- 锁屏时真实 screenShield 为 locked/active，session mode 为 unlock-dialog；两协议返回全空窗口事实。使用合成窗口，无生产窗口标题或数据库读取。私有环境缺少 GeoClue、账号与日历服务产生预期警告，不宣称零警告或生产认证通过。
- 真实 Shell 首轮暴露无 screenShield 时启动失败，已修复并补回归；测试启动 overview 的竞争也已由等待真实焦点解决。最终字段复核保留合法的零窗口 ID，并对齐新协议应用身份条件。
- 用户随后授权备份安装：用户目录已从 version 2 替换为经过验收的 version 4，前后哈希核对通过。旧文件与 receipt 位于 `/home/arinp22/.local/state/patina/acceptance/20260922-gnome-v4-a9cxs25d/`。系统目录仍为 version 2，未改包管理文件；当前 Shell 未强制重载，等待用户注销/登录后确认实际加载及锁屏恢复。安装文件不等于生产激活验收完成。

- 最终 `npm run release:check` 通过：56 个 TypeScript 测试文件（扩展 9 项运行时 / 4 项脚本测试）、38 项浏览器回归、705 Rust passed / 15 ignored，以及构建/预算/边界/Clippy/扩展/版本/changelog。日志 `/tmp/patina-beta20-extension-release-check.log`。首次沙箱运行在子进程 `spawnSync EPERM` 处中止，宿主门禁已重跑通过。

### AppImage 本地候选与证明范围

候选保留于 `/tmp/patina-beta20-platform-candidate-sqr83d1y/`，包含 AppImage、version 4 扩展 ZIP 与 manifest。AppImage 为 `Patina_1.9.0-beta.20_amd64.AppImage`（103,184,888 bytes），SHA256 `1bf6c87b153da6dc2f16873a8a2ecdffa5ce781afc630ea876980b3199f9633d`。它基于 `4f6124ab` 和本轮工作区，只是本地未签名测试候选，未作为公开资产发布或更新生产 daemon。

- 本地 release 编译通过；首次沙箱 linuxdeploy 失败，宿主仅重跑 bundle 成功。构建日志 `/tmp/patina-beta20-appimage-build.log`，最终打包日志 `/tmp/patina-beta20-appimage-bundle.log`。linuxdeploy 有依赖 copyright 定位警告，不记录为零警告。
- 实际 AppImage 通过 extract-and-run 启动 `--patinad --version`，返回 `patinad 1.9.0-beta.20`，私有目录未创建数据库。证据 `/tmp/patina-appimage-launch-3m96alsv/result.json`。
- `built_appdir_runs_from_durable_store` 通过；持久运行时 `/tmp/patina-durable-appimage-13202d0fbe050963/current`，完成复制、前后版本预检与原子激活。日志 `/tmp/patina-beta20-appimage-durable.log`。生成 user unit 另通过真实 `systemd-analyze` 解析，日志 `/tmp/patina-beta20-appimage-unit-parser.log`。
- 持久 AppRun 在真实随机临时 systemd user unit 中完成本地与私有 WebDAV 的 replace/merge/注入失败回滚，共六组跨进程恢复，均通过并收尾；没有控制生产 `patinad.service`。日志 `/tmp/patina-beta20-appimage-systemd.log`、`/tmp/patina-beta20-appimage-webdav.log`。
- 实际 AppImage 字节使用临时测试密钥签名，经回环 HTTP 和 Tauri updater 验签后进入原子替换；旧包保留，篡改下载与无效签名均拒绝且不改变目标。日志 `/tmp/patina-beta20-appimage-signature.log`；测试私钥已删除。此项使用测试 app handle，不是正式渠道/密钥的桌面升级验收。

剩余清单：

- [x] 扩展生产端、合成回归、独立真实 GNOME 42.9 验收。
- [x] 经授权备份并安装用户扩展 version 4，哈希与候选一致。
- [ ] 生产激活、锁屏恢复与持续记录全部通过：重新登录后的 v4 激活已确认，人工锁屏/挂起已完成，仍有下方会话环境诊断异常及边界自动证据限制。
- [x] beta.20 未签名 AppImage 成品、实际 daemon 入口、持久 AppDir、真实临时 systemd 恢复和测试密钥验签替换。
- [ ] AppImage 正式 Desktop 首次启动/接管、与 DEB 实装共存、真实登录启动。
- [ ] 正式签名渠道升级、旧 AppImage 客户端更新路径；完成后再评估恢复双包 workflow/manifest。现阶段继续 DEB-only beta。
- [ ] ESM 入口与更多 Shell 版本单独实现、打包和真实验证。

本轮未新建分支、修改上游贡献草稿、推送、打 tag 或公开发布。`feat/linux-desktop` 仍基于 `80204c73`，保留 52 个既有未提交/未跟踪文件；旧 `/tmp` 草稿 SHA 清单当前不存在，因此本轮不声称重新完成逐文件旧哈希比对。

### 用户重新登录后的生产核对（2026-09-22）

用户确认注销并重新登录后，只读核对发现：GNOME 从用户目录加载 version 4，状态 ENABLED；旧/新 D-Bus 名称同属 `:1.40`，证明新代码已在生产 Shell 激活。未请求焦点窗口或标题。`managed` 15 项检查通过，后台版本仍为 beta.19，service PID 1559、NRestarts 0、lease owner 一致，数据库 quick_check 为 ok。证据 `/tmp/patina-gnome-v4-post-login-managed.json`。

两次间隔 4 秒的 diagnostics 显示 probe_status=ok、连续 fallback=0，成功采样时间推进；但平台诊断仍为 unavailable / unknown-session-type。只读环境白名单确认：旧 daemon 仅持有 session bus 地址，缺少 XDG_SESSION_TYPE、XDG_CURRENT_DESKTOP、DISPLAY 和 WAYLAND_DISPLAY；当前 systemd manager 已持有 wayland/GNOME 及显示变量。服务启动时间早于本次桌面登录，PID 在注销后未改变。

这不是扩展加载失败，也不能据此宣称所有平台观测均正常。已安装 beta.19 的旧 foreground 路径直接尝试 GNOME D-Bus，而 beta.20 新路径拒绝未知 session type，因此会话环境的获取/更新必须在 beta.20 生产安装前处理并验证。不能只用一次重启掩盖下次登录的相同竞态。本轮未重启生产 service、未修改 manager 环境或服务配置。

- [x] 用户注销/登录后确认 version 4 实际激活、双名称 owner 与后台托管基本状态。
- [x] 修复并验证 daemon 在桌面晚于 user service 启动、注销/重新登录时的会话环境识别；源码、私有总线生命周期及本机清空环境观测通过，生产 beta.19 尚未替换。
- [x] 用户报告已完成锁屏/解锁与挂起操作，恢复后的采样已核实；边界自动证据不足单独记录。

### 用户报告锁屏后的验证（2026-09-22）

用户报告已手动锁屏/解锁。事后只读检查：ScreenSaver=false，daemon PID 1559、NRestarts 0，probe_status=ok，最近成功样本距检查 773 ms，最近会话已恢复写入；证据 `/tmp/patina-gnome-v4-after-user-lock.json`。仅读取会话时间边界和状态，不读取标题或网页内容。最近 15 分钟无 power watcher 固定日志事件，但该实现不会记录每次 lock/unlock，所以日志缺失不能证明未发生锁屏。

随后启动 180 秒只读同步监测，等待用户在监测期间再操作一次；该窗口未观察到锁屏，且无观测调用错误。证据 `/tmp/patina-production-lock-dyaqjqar/result.json`。结果为 **未捕获完整周期、验收未完成**，不是发现锁屏功能失败。当前仍不能证明此前锁屏期间双协议全空、native/title/web 时间边界正确及恢复不补记。用户随后明确说明已测试锁屏与挂起，记为人工实机操作完成，解锁后恢复已有只读证据；未同步捕获的计时边界保持证据限制，不要求用户重复操作或以此阻塞开发。原有 unknown-session-type 问题已由下方第四片完成源码修复，生产安装验证另行推进。

## 第四片：daemon 图形会话识别与监听重绑（源码及 AppImage 隔离验证完成，未安装）

owner 为 `platform/linux`：新增窄的 logind 会话读取模块，供 foreground/diagnostics 和 power watcher 使用。受管 daemon 使用当前 UID 的 logind User.Display 与 Session 事实，不修改进程全局环境，不依赖重新启动继承 manager 环境；校验本地、活动、用户类型及 UID，缺失/失效时保守失败。直接桌面进程保留有效的显式会话环境路径。

power 的全局睡眠/关机订阅不随图形会话缺失停止；当前 Display 变化或会话删除后丢弃旧会话订阅，重新订阅并读取当前 LockedHint（包括 unlocked），避免旧锁状态遗留。增加自动场景覆盖桌面晚启动、注销、新会话、锁屏状态重建和无有效图形会话；不会重启生产服务或要求重复人工锁屏。

第四片使用当前 UID 的 `GetUser` + `User.Display`，读取 Session 的 Id/Type/Desktop/Display/User/Class/Active/Remote/State 并二次确认 Display 未切换。读取有 700 ms 超时，既不修改全局环境，也不永久缓存旧会话；采样和诊断共用这份平台事实。X11 连接和 idle 显式接收 Display，不让受管服务在缺失 Display 时隐式连接旧地址。

power watcher 保持全局睡眠/关机订阅，每秒重新校验当前图形会话；接收会话信号时也检查其仍被选中。会话重绑时先订阅，再同步 LockedHint，包括 unlocked。新 helper 只拥有环境观测，不承接 engine 会话写入或服务控制。

此前 AppImage SHA `1bf6c87b…9633d` 的证据仅覆盖第三片候选，不包含第四片运行时修改；不得将旧候选安装/升级结果记为本次源码结果。

第四片本机只读观测已通过：以 `PATINA_SYSTEMD_SERVICE=patinad.service` 启动测试进程，移除 XDG_SESSION_TYPE/XDG_CURRENT_DESKTOP/DESKTOP_SESSION/DISPLAY/WAYLAND_DISPLAY 后，新的同步采样上下文和异步 logind 读取结果一致，识别 wayland；诊断返回 available / gnome-shell-extension。日志 `/tmp/patina-logind-host-test.log`。没有请求焦点窗口、重启生产后台或修改当前用户 manager 环境。该证据验证新源码能力，不意味着已安装 beta.19 的诊断随之改变。

第四片验证收口（2026-09-22）：

- 4 项普通会话校验回归通过，覆盖 UID、活动/本地/用户类型、缺失和畸形属性、X11 Display 与显式非图形环境。
- 私有真实 D-Bus 夹具通过无图形会话、晚登录、LockedHint、注销与新会话重绑、旧会话信号隔离，以及全过程的全局睡眠/关机订阅。日志 `/tmp/patina-logind-private-test.log`。首次运行暴露属性订阅初始通知重复发送 unlock，现按每次绑定的最后锁状态去重；重绑仍主动同步当前状态。夹具不操作生产 logind 或真实锁屏。
- `npm run release:check` 全部通过：56 个 TypeScript 测试文件、38 项浏览器回归、709 Rust passed / 17 ignored，以及构建、预算、边界、Clippy、扩展和 changelog 检查。日志 `/tmp/patina-session-release-check.log`。两个新增 opt-in 测试已分别显式执行通过，不把普通门禁的 ignored 计为执行。
- 本批未重启、安装或修改生产服务，未构建新的 DEB/AppImage。下一候选需纳入第三、第四片的完整源码，再验证其打包行为；旧 AppImage 的通过记录不能替代新候选。正式 AppImage 实装与签名升级门槛继续保留。

### 第四片候选打包与隔离验收（2026-09-22）

用户同意后，已将第三、第四片完整源码构建为新的本地未签名 beta.20 AppImage。候选与日志保存在 `/tmp/patina-session-candidate-3f0v71k1/`；`source-manifest.json` 记录基准 `4f6124ab` 和构建时各文件 SHA256，验收结束后确认源码哈希未变化（随后仅更新本文等验收记录）。

- 包：`Patina_1.9.0-beta.20_amd64.AppImage`，103,320,056 字节，SHA256 `98b23f6925b75f76afd6357c3f2e84090ffa0d17168888e744bf96326001272d`。
- 实际包 `--appimage-extract-and-run --patinad --version` 返回准确版本，私有启动目录未生成数据库；持久 AppDir 发布和启动、真实 systemd 单元解析均通过。
- 真实临时 systemd 单元使用该 AppDir 的 `AppRun --patinad`，本地与私有 WebDAV 各完成 replace、merge、故障回滚，共 6 个跨进程恢复场景。各阶段日志见候选目录 `results.json`。
- 实际候选字节使用临时测试密钥，经 Tauri 下载验签后原子替换；篡改内容和无效签名均被拒绝。测试私钥及密钥生成日志已删除，不涉及正式发布密钥。
- 本轮生产服务前后均为 PID 1496、NRestarts 0、active，启动时间 2026-09-22 12:06:50 +08。此前记录的 PID 1559 属于上一轮观察，本轮未重启服务。没有安装候选或改变生产数据。

本候选取代旧第三片候选作为当前打包证据。版本仍为未发布 beta.20，所有验收采用新建私有目录；同版本不同内容的包不能覆盖已有持久运行时，这不是升级路径验收。正式 Desktop 首次接管、DEB 共存、真实登录启动和正式签名渠道升级仍未完成；继续保持 DEB-only beta 发布门槛。旧 `/tmp/patina-beta20-platform-candidate-sqr83d1y/` 本轮已不存在，前述旧候选条目仅保留历史记录。

### 实际 Desktop 首次启动补验发现与修复（2026-09-22）

新增 `scripts/appimage-startup-acceptance.py`，使用实际包入口、私有 X11 和不自动激活服务的 D-Bus，在 mount/PID/network 隔离环境覆盖 standalone、DEB unit 存在、自定义 unit、profile roots 不一致四条启动路径。systemd 是夹具，检查截止于运行时准备完成或预期拒绝，不将其计作真实后台接管/登录。

旧 SHA `98b23f69…1272d` 的 Desktop 首次启动失败：Tauri 的 `.DirIcon` 绝对链接仍指向原构建 AppDir，实际解包后位于运行时根之外，触发 `package link escapes AppDir`。此前构建目录 AppDir 测试通过不能证明实际包可首次运行；该候选不再作为可安装候选。失败证据 `/tmp/patina-appimage-startup-5um5e51o/standalone/desktop.log`。更早的 Xvfb/NVIDIA 与门户等待属于夹具问题，已通过禁用 GLX、干净环境和禁止 D-Bus 自动激活修正。

修复 owner 保持 `platform/linux/appimage_runtime`：复制持久运行时前忽略根 `.DirIcon` 元数据，不解析其目标；其他位置的同名文件及所有运行时链接仍遵守原有越界/悬空拒绝。增加回归验证根图标别名跳过、嵌套同名链接仍拒绝；持久解包验收改用实际候选解出的 AppDir。新候选构建和完整门禁已通过，详见下方结果。

本次收口结果：

- 修复后的候选：`/tmp/patina-startup-fixed-candidate-187v1ib3/Patina_1.9.0-beta.20_amd64.AppImage`，103,307,768 字节，SHA256 `eea0e8a041555337c502c3d937dd7fa7751f267a59e5aac3fd5ffc8b97b122b2`。仍是未发布、未安装的本地 beta.20，同版本候选之间不作持久运行时覆盖升级。
- 实际包 Desktop 四条路径通过：standalone 完成持久运行时和 unit 落盘并 Reload；DEB presence 路径进入既有服务检查而不生成另一份运行时/unit；自定义 unit 原样保留并拒绝启动；manager roots 不一致时在落盘前拒绝。证据 `/tmp/patina-appimage-startup-5n7yxuiz/result.json`，包哈希与候选一致。夹具缺少 AT-SPI 服务产生预期警告，不宣称零警告或完整 UI 验收。
- 实际包 daemon 入口、**实际包解出的** AppDir 持久化、真实 systemd parser、本地/WebDAV 共 6 个恢复场景及测试密钥验签原子替换全部通过。候选目录 `results.json` 收录每项日志和启动检查摘要；测试私钥已删除。
- `npm run release:check` 全部通过：56 个 TypeScript 文件、38 项浏览器回归、710 Rust passed / 17 ignored，含新图标别名回归及 Clippy/构建/边界/扩展门禁。日志保存在候选目录 `patina-appimage-startup-check.log`。
- 生产服务仍 active、PID 1496、NRestarts 0，本轮没有重启或安装生产后台，没有推送、打 tag 或发布。源文件哈希核对仅有后补验收文档变化。

首次启动的运行时准备与 DEB 路径选择已获得实际包证据；**真实后台接管、DEB 安装后共存、真实登录启动及正式签名渠道升级**仍保留为实装门槛，不能由上述私有 systemd 夹具代替。后续实装使用新 SHA 候选，旧 `98b23f69…1272d` 不再推荐使用。

### 完整进程接管、实际 DEB 文件共存与临时 systemd 故障恢复（2026-09-22）

沿用 AppImage SHA `eea0e8a0…122b2`，本轮只扩展验收脚本，没有修改产品运行时源码。证据汇总在 `/tmp/patina-startup-fixed-candidate-187v1ib3/coexistence-summary.json`。

- 同一批 release 二进制另打包本地 DEB：`Patina_1.9.0-beta.20_amd64.deb`，SHA256 `1cc4ebbb79ec32774732f9cf8b46c2b7bfa04c6615b33869bd7868639d855fd5`，保存在上述候选目录。DEB 契约检查通过。AppImage/DEB daemon 的 ELF build ID 同为 `3e2d1f7a7963e8de66ed2f82797a289e2203ed23`；linuxdeploy 给 AppImage 增加 RUNPATH，因此两个文件的 SHA 不相同，分别记录，不把它们说成逐字节一致。
- 私有 dpkg root 完成旧本地 beta.19 安装 → beta.20 升级 → 卸载 → 重装，包文件/模式和合成用户数据保留检查通过。正常依赖检查使用宿主已安装包的元数据副本；没有验证依赖 payload 安装。证据 `/tmp/patina-deb-acceptance-w4z68nzl/evidence.json`。
- 实际 AppImage Desktop 在私有 X11/D-Bus 中运行六条路径：原四条保护/准备路径，加 standalone 与真实 DEB payload 两种完整进程接管。两者均由 Desktop 自己生成预约、自动重启，最终状态 completed；API 确认 daemon/tracking/service owned+ready；界面退出、重开后只启动过一次 daemon、PID 不变；daemon SIGINT 正常退出后数据库 quick_check=ok。证据 `/tmp/patina-appimage-startup-sapxvafa/result.json` 和两条 handoff 的 `cleanup.json`。
- DEB 共存 case 使用前述私有 dpkg 实际安装的 `/usr/bin/patinad` 与 unit，AppImage 不生成另一个持久 runtime 或用户 unit；服务管理仍是私有夹具。首次完整接管试跑缺少 INVOCATION_ID，被新增 managed capability 断言拦截；补齐夹具后最终六条通过，不把早期 manual daemon 结果冒充受管验收。
- 另用真实 user systemd 随机临时单元 `patina-hardening-6u6pf3te.service` 启动该 AppImage 的持久 AppDir。配置并回读发布 unit 的 NoNewPrivileges、PrivateTmp、ProtectSystem=strict、RestrictSUIDSGID、UMask=0077、Restart=on-failure；受管 API ready 后向该临时 daemon 注入 SIGKILL，systemd 自动恢复，PID 从 231836 变为 231917、NRestarts=1。随后正常 stop，observer exit=0、数据库 quick_check=ok，临时 unit 已收集。此项验证带这些配置的启动/恢复，不单独证明内核对每项限制的强制效果。
- 临时 systemd 证据位于 `src-tauri/target/acceptance/patina-hardening-6u6pf3te/result.json`；该测试将运行时和数据放在仓库被忽略的 target 目录，避免 PrivateTmp 隐藏 `/tmp` 候选。复现用一次性 runner 保存在候选目录 `hardened-acceptance.py`。只连接不存在的私有采样总线/音频地址，没有读取生产窗口；这些 provider unavailable 日志是预期的。
- 真实生产 patinad 的 PID、重启次数和启动时间前后完全一致。没有安装宿主 DEB、切换宿主后台、推送或公开发布。

本轮补齐的是 **完整实际进程接管 + 真实 DEB 文件共存（服务管理夹具）**，以及单独的 **真实临时 systemd 安全配置与故障恢复**。尚未将两者组合成真实安装用户的端到端 systemd 接管；真实登录启动、正式签名渠道升级也仍未验证，继续保持 AppImage 发布门槛。脚本实跑、语法、文档契约和 diff 检查作为本轮验证；产品源码未变，复用上一轮完整发布门禁，不重复全量构建。

### 本机 beta.20 安装及真实 AppImage/DEB 共存验收（2026-09-22）

用户在明确说明实装流程后要求继续完成验收。候选复制至持久私有目录 `/home/arinp22/.local/state/patina/acceptance/20260922-beta20-6ytczb9e/`，目录 0700；数据库/配置备份和摘要 0600，包含私有数据，不进入 Git。`acceptance-summary.json` 为本轮实装汇总。DEB SHA 为 `1cc4ebbb…55fd5`、AppImage SHA 为 `eea0e8a0…122b2`，与前述隔离验收候选一致。

- 安装前 beta.19 managed 检查通过；确认无未完成迁移/恢复/清缓存预约，SQLite backup API 在线备份与 profile 归档通过。系统认证后 dpkg 将 beta.19 升级为该本地 beta.20；所有安装文件 SHA 与候选 manifest 一致。
- 旧 PID 1496 停止后确认 MainPID=0、获取 runtime owner 排他锁并建立停写备份；完整性、外键及 SQLx 校验通过后启动新后台 PID 1027382。没有自动降级数据库或恢复旧备份。
- 新后台 managed 检查通过；实时平台诊断为 available / gnome-shell-extension / wayland，旧 unknown-session-type 异常已消除。
- 实际 AppImage 在本机打开设置页并正确读取受管存储状态；关闭窗口、第二次启动单实例唤回、托盘正常退出均通过。UI 全流程后台 PID 不变；无 UI 的 15 秒中成功采样时间推进 15,602 ms。
- 随后启动真正安装的 `/usr/bin/Patina`，设置页及托盘正常退出通过，继续复用同一后台。没有新建 AppImage 用户 unit 或 runtime store；正式 unit 仍由 DEB 提供。
- 最终 managed 检查、数据库 quick_check/外键、SQLx 元数据以及固定历史 session/web/title/import 摘要与停写备份一致。运行中的活动与新增记录单独允许变化。未执行真实数据迁移或清缓存。
- 后台登录启动已经 enabled，注销前 logind 会话基线存为 `login-baseline.json`。目前仅 daemon 在后台，等待用户保存工作后注销、重新登录，再核对新会话、自动启动和持续采样。之前用户已完成的锁屏/挂起不要求重复。

当前结论：**本机 DEB beta.20 升级与实际 AppImage/DEB 共存验收通过**。纯 AppImage 首次部署的真实 systemd 安装用户接管仍只具备分层隔离证据；新版本的实际注销/登录与正式签名渠道升级仍未完成。没有打 tag、推送或公开发布，AppImage 发布门槛继续保留。

### beta.20 重新登录后只读验收（2026-09-22）

用户确认已重新登录后，本轮没有启动 Desktop、重启服务或触发锁屏/挂起。持久证据仍位于 `20260922-beta20-6ytczb9e/`：`post-login-managed.json`、`post-login-session.json`、`ui-post-login-background.json`、`post-login-comparison.json`；汇总已更新。

- logind 当前图形会话为 25，Wayland、本地、active/user；其创建时间晚于注销前保存的观测时刻。旧 `login-baseline.json` 的 session 字段为空，原因是原采集命令使用了 loginctl 不支持的逗号属性列表；本轮通过 D-Bus 直接取属性，以创建时间交叉确认新登录，不声称比较过旧新会话 ID。
- daemon 仍为 beta.20、PID 1027382、NRestarts 0、enabled/active，跨注销持续运行，没有被 Desktop 或验收工具重新拉起。GNOME 两协议名称均属于同一 owner `:1.32`，平台诊断 available / gnome-shell-extension / wayland。
- 无 Desktop 的观测窗口中成功采样时间推进 12,578 ms，心跳也推进；managed 检查及固定历史摘要、SQLite 完整性、外键与 schema 核对全部通过。

**新版真实注销/登录后的后台延续与会话恢复验收通过。** 由于 daemon 在本轮注销期间未退出，此证据不覆盖冷启动自动拉起；该项明确保留，不能仅凭 enabled 状态记为通过。纯 AppImage 首次安装用户的 systemd 接管及正式签名升级门槛也继续保留。本轮只更新验收记录，无产品源码、包、安装或发布变更。

### 当前批次收尾与发布准备（2026-09-22）

用户要求关闭 Patina 桌面开机自启后完成剩余工作。只读核对发现 `launch_at_login=0`，用户及系统 autostart 目录均无 Patina 入口，已经满足要求，无需重复修改；`background_tracking_at_login=1` 且 `patinad.service` enabled 保留，以验证没有 UI 时的自动记录。

- 已安装候选与当前产品源码 SHA 核对一致；后续差异仅为验收脚本和文档。复用完整门禁 710 Rust、56 TS 文件、38 浏览器回归；本轮发布契约另通过 25 项 policy、3 项 DEB、11 项 installed acceptance 测试，版本/changelog/文档契约/diff 检查通过。
- beta.20 发布说明草稿已生成至持久验收目录 `release-notes-beta20.md`。继续 DEB-only beta；现有发布 workflow 会公开发布，尚未调用，不以准备步骤隐式授权 tag、push 或 release。正式签名由发布环境的 secret 提供，本地候选无正式签名。
- `cold-boot-baseline.json` 记录当前 boot ID、自启设置和 daemon 状态；`cold-boot-verify.py` 已准备，在相同 boot ID 上明确拒绝通过。用户保存工作并真实重启后，不打开 Patina，执行该脚本可完成 managed/无 UI 采样/历史数据检查；本轮不会自行重启用户电脑。

剩余门槛按顺序管理：

1. 真实重启后的 DEB daemon 冷启动检查已通过，见下方记录。
2. beta.20 正式签名及发布：核对最终提交/tag 与 DEB-only 资产、正式签名，公开发布需单独授权；正式升级不使用同版本本地候选互相覆盖。
3. 恢复 AppImage 发布前：在无 DEB 的独立安装用户环境完成首次 systemd 接管、登录启动和正式签名升级。当前宿主已有 DEB，只验证了真实共存；不为凑齐证据卸载用户当前可用后台或伪造正式密钥。

本批代码、验收脚本及文档固定为本地提交；不改上游贡献草稿、不推送或发布。

### 真实系统重启后的冷启动验收（2026-09-22）

用户确认重启后运行预先准备的只读脚本，当前 boot ID 与 `cold-boot-baseline.json` 不同。未启动 Desktop 或控制服务；`patinad.service` 已自动进入 enabled/active，PID 1477、NRestarts 0，启动时刻为 22:17:42 +08。

- `launch_at_login=0`、`background_tracking_at_login=1` 保持不变；无 Desktop 的观测窗口中成功采样时间推进 18,733 ms，心跳也推进。
- 新 Wayland 会话 3 为本地 active；GNOME 新旧协议属于同一 owner，采样诊断 available / gnome-shell-extension / wayland。
- beta.20 managed 检查、数据库完整性/外键/schema 和固定历史数据摘要全部通过。
- 持久证据目录 `20260922-beta20-6ytczb9e/` 中保存 `cold-boot-managed-1790086803.json`、`ui-cold-boot-1790086803.json`、`cold-boot-comparison-1790086803.json`、`cold-boot-result-1790086803.json` 与 `cold-boot-platform.json`；汇总已将冷启动标记为通过。

本机 DEB beta.20 的实装、实际 AppImage 共存、注销/登录恢复和冷启动验收现已收口。剩余正式签名/发布与纯 AppImage 独立安装环境门槛不变；本轮只有只读验收及文档更新，没有修改产品代码或重新打包。

### 独立 systemd 首次接管发现与修复（2026-09-22）

用户要求完成剩余工作后，使用 Docker 创建无 Patina DEB 的 Ubuntu 22.04 独立用户环境，运行真实 PID 1、user manager 与实际 AppImage。容器使用私有 cgroup、无网络及无宿主挂载，不卸载或改变生产 DEB。新增可复跑脚本 `scripts/appimage-systemd-acceptance.py`，范围见开发文档。

- 首次夹具试跑的 `/tmp` 默认 noexec 阻止 AppImage 解包执行，已显式设置临时目录 exec；该次没有进入应用逻辑，不记为产品失败。证据 `/tmp/patina-appimage-systemd-jj1i7h6b/`。
- 真实 manager 下，旧候选 SHA `eea0e8a0…122b2` 暴露凭据生成竞态：Desktop 在 daemon 生成 `api_token` 前读取失败，提前返回导致客户端适配器与接管确认任务均未启动；daemon 已 active，但预约永久停留 activating。证据 `/tmp/patina-appimage-systemd-njlsjpl4/desktop.log` 与 `driver.log`。旧候选的共存/冷启动结论保持原范围，不能据此宣称纯 AppImage 首次接管通过。
- 边界判断：凭据读取仍归 `engine/api/auth`；等待和客户端初始化归 `app/daemon_client/runtime`，`app/runtime` 只装配参数。客户端异步等待读取现有凭据，10 秒上限、可取消；读取错误不重写文件。接管确认任务始终启动，沿用 15 秒确认/失败窗口，避免缺失凭据时无限 activating。没有新增 runtime owner 或 embedded 回退。
- 4 项专项覆盖延迟生成、超时不创建凭据、取消及无效字节保留。完整 `release:check` 通过：56 个 TypeScript 文件、38 项浏览器回归、714 Rust passed / 17 ignored，以及 Clippy、扩展与版本/changelog 检查。沙箱内子进程 EPERM 后在宿主重跑通过，两次日志分别为 `/tmp/patina-credential-release-check.log` 与 `/tmp/patina-credential-release-check-host.log`。
- 独立验收给容器用户 unit 加入仅用于测试的两秒 `ExecStartPre` 延迟，以确定性复现首次凭据晚到。新候选打包与回归结果续记于下方；本机仍运行先前实装的 beta.20，不能把新候选称为已经安装。

只读发布准备确认 GitHub 登录有效、两个 Tauri 签名 Secret 名称均存在，远端 main 为 `4f6124ab`，最新预发布 beta.19，beta.20 无远端 tag。Secret 存在不证明本次候选已经正式签名；现有 workflow 会公开发布，尚未调用。

本次修复的新候选与收口结果：

- 持久目录 `/home/arinp22/.local/state/patina/acceptance/20260922-credential-startup-dyrh0uxe/`（0700）保存实际包、`source-manifest.json`、`artifacts.json`、完整门禁、两次失败证据及成功报告；不依赖重启后可能消失的 `/tmp`。产品源码在构建前后逐文件 SHA 校验一致。
- AppImage SHA256 `84da760d5ded1833a951b16424b5c2cd5e81e53cf78693a1c1c6a521c438de75`，DEB SHA256 `134bab945bd04f99dd8d70d9507de8293e72021005a9ddf1d1b00fd6fd16d838`；均为本地未签名 beta.20，不与旧同版本持久运行时互相覆盖升级。
- 无 DEB 的独立容器用户由 Desktop 自行生成持久运行时、user unit 并完成接管；两秒 daemon 启动延迟下也自动连上。真实 systemd 初始 PID 271，界面退出/重开保持同一 InvocationID；SIGKILL 后自动恢复至 PID 459、NRestarts=1。正常 stop 后退出状态 0、SQLite quick_check 与外键检查通过。
- 重启容器 PID 1 与 lingering user manager 后，enabled daemon 自行启动为 PID 64、新 InvocationID、NRestarts=0；没有手动 start 服务或打开 Desktop。证据 `systemd/result.json`。这是独立真实 user manager 的安装接管与容器重启证明，不是 GNOME 图形登录、宿主冷启动、真实窗口采样或 FUSE 挂载证明。
- 新 DEB 发布契约通过；私有 dpkg root 完成 beta.19 安装、beta.20 升级、卸载与重装，保留合成数据。使用宿主依赖元数据副本，仍不宣称验证干净发行版的依赖 payload 安装。证据 `deb/evidence.json`。
- 新 AppImage 六条隔离启动路径全部通过：standalone、DEB presence、custom unit、profile mismatch、完整 handoff，以及复用上述实际私有 dpkg daemon/unit 的 deb-handoff。此组服务管理是夹具，与独立真实 systemd 证据分开保存于 `startup/`。
- 临时容器已全部删除，仅保留可复用的本地验收镜像。生产后台仍 PID 1477、NRestarts=0、active；没有安装新候选、修改宿主服务、推送、tag 或发布。本机此前的实装证据继续对应旧包 `1cc4ebbb…55fd5`，不冒充新修复已经实装。

当前批次的代码修复、完整门禁、新候选打包、独立真实 systemd 接管/恢复和共存回归已完成。下一发布动作是按既定策略发布 DEB-only beta.20，必须另获 push/tag/公开发布授权并由 CI 正式签名；不能发布本地未签名候选。AppImage 恢复公开发布前仍需独立 GNOME 安装环境的图形登录与正式签名渠道升级验收，当前不解除门槛；ESM/更多 Shell 版本仍属于后续兼容阶段。


### 审核后修复：凭据恢复与桌面自启动（2026-09-23）

本轮在 `main` 工作区修复审核确认的两项问题，发布说明记入 Unreleased，不修改已发布版本的条目。owner 保持不变：凭据等待归 `app/daemon_client/runtime`，桌面登录入口归既有 `app/autostart`。

- 移除客户端独立的 10 秒终止期限。缺失或空凭据持续等待、支持取消，15 秒接管确认仍独立记录成功或失败；服务晚到可恢复客户端连接，但不会擅自清除已记录的接管失败。不可恢复的读取错误报告 Stopped，不再留下没有任务工作的 Reconnecting。
- AppImage 登录入口使用原始包路径，DEB 共存优先使用已安装 Desktop；缺少有效包路径时拒绝写入临时二进制路径。原始包移动或删除后需要重新设置自启动。带空格、百分号的包名经过实际 GIO Desktop Entry 解析验证。
- `npm run check:full` 通过：56 个 TypeScript 测试文件、38 项浏览器 smoke、715 Rust passed / 18 ignored、Clippy 与前端构建。另显式运行 7 项 autostart 测试（包含默认忽略的真实 GIO 启动），全部通过。凭据回归实际等待 11 秒后生成 Token，并覆盖取消、持续缺失不创建 Token、无效字节不重写。文档契约、changelog、Python 语法、定向 rustfmt 与 diff 检查通过。
- 本地未签名 AppImage 候选：`/tmp/patina-review-fixes-candidate-mxmefj0v/Patina_1.9.0-beta.20_amd64.AppImage`，SHA256 `833d273a230a20a37a744eda941f7ba287a240d32282064c27c57ab9b86a8353`。仅用于全新隔离配置，不是公开 beta.20 原包，不安装或覆盖本机运行时。该目录保存源码哈希、构建/检查日志与结果摘要。
- 六条私有启动路径通过，证据 `/tmp/patina-appimage-startup-fczpmk2z/result.json`。独立部署和真实 DEB payload 共存分别断言原始 AppImage 与 `/usr/bin/Patina` 自启动入口；服务管理仍为夹具，不冒充真实 systemd。
- 真实 Docker user systemd 验收使用 11 秒 daemon 启动延迟；先删除旧临时解包目录，再执行生成的自启动命令。验证接管 completed、桌面重开不替换 daemon、SIGKILL 恢复、数据库完整性和容器冷启动。固定显示配置下连续两个全新容器通过，证据为 `/tmp/patina-appimage-systemd-tehp9l4l/result.json` 和 `/tmp/patina-appimage-systemd-u3945lyi/result.json`。
- 失败证据保留：首次 `/tmp/patina-appimage-systemd-qe79jkad/` 在自启动重开后退出，原样复跑 `/tmp/patina-appimage-systemd-0badbe39/` 全流程通过；软件渲染下 `/tmp/patina-appimage-systemd-v5lu13nh/` 捕获退出码 127 和 `XI_BadDevice`。最终 headless 夹具显式使用软件渲染与 Xvfb `-noreset`，模拟持续存在的显示服务器；不把早期失败记成成功，也不据此宣称 GNOME 渲染或 FUSE 已验收。
- 沙箱中的 Node 子进程 EPERM 后在宿主完成全量检查；首轮打包等待期间终止了本轮打包任务，随后复用已编译产物完成正常 Tauri bundle。重复打包提示二进制已无法再次写入 bundle 标记，核对已有且唯一的 AppImage 标记，并通过实际包启动路由验证。

本轮没有安装、推送、打 tag 或公开发布，也没有更改本机桌面开机自启动设置。AppImage 的真实 GNOME 图形登录、FUSE 和正式签名升级门槛继续保留。


### beta.21 候选与后续门槛（2026-09-23，本机验收完成）

用户授权完成上一轮提出的 main 提交、本机候选验收、下一版本准备与 AppImage 门槛推进。默认不把“准备下一版本”解释为公开发布授权。

- [x] 已验证修复提交到 main：`fa1cec27`。
- [x] 同步 beta.21 版本与发布说明，`release:check`、发布策略/DEB/已安装检查专项和 Firefox 检查通过。
- [x] 生成 DEB 与本地 AppImage 验收候选；DEB 契约与 beta.20→beta.21 私有 dpkg 安装/升级/卸载/重装通过，真实 FUSE 挂载与卸载通过。
- [x] 备份本机现有数据/配置，安装候选，验证 GNOME 中启动、退出、重开和后台持续记录，保持桌面自启动关闭。
- [x] 使用真实 FUSE 挂载验证 AppImage 的本机 DEB 共存；复核独立 GNOME 登录与正式签名升级的可用环境。
- [x] 固定版本准备及验收记录，留下明确的发布与 AppImage 未完成门槛。
- [ ] AppImage 发布前补齐独立 GNOME 登录、解决 Xvfb 默认悬浮窗退出问题，并完成正式签名升级；本轮不具备完整验收条件，不勾选通过。


本批包与环境证据：

- 源码准备提交 `0fe0838e`，已从提交文件独立验证版本/changelog。私有持久证据目录 `/home/arinp22/.local/state/patina/acceptance/20260923-beta21-d39le7kc/` 保存已校验的在线 SQLite 备份、配置归档、源码哈希、包与发布门禁日志，不进入 Git。
- 本地未签名 beta.21 DEB SHA256 `1400104ad8c1c87a52283ede414f2af42bb779ff7b7db8b7421be9db4eb58cb2`；AppImage SHA256 `a1170977b858d31358a3176986b8d92a7ecb7437846de388c1aa298150af2257`。它们是验收候选，不是正式签名发布资产。
- DEB 私有 dpkg 根证据 `/tmp/patina-deb-acceptance-s4hbh0th/evidence.json`。真实 FUSE 验证使用 `--appimage-mount`，包内 daemon 报告 beta.21，挂载已清理，证据 `fuse-result.json`；首次清理检查与自动卸载竞态已在私有脚本中改为等待实际卸载完成，不以错误的重复卸载证明产品故障。
- **AppImage 容器验收存在未收口问题：** beta.21 在 `/tmp/patina-appimage-systemd-vis4sudo/` 完成 11 秒延迟接管后，自启动默认悬浮窗退出码为 127，GDK 报 `XI_BadDevice`。本次已启用软件渲染和 `-noreset`，因此前一批两次通过不能证明该 Xvfb 路径已稳定，也不能把本次整套流程记为通过。暂不扩大到已暂停的悬浮窗改造，不解除 AppImage 发布门槛。
- 用户确认没有独立 GNOME 测试环境，本轮只完成本机与可隔离验收；独立 GNOME 登录、上述 Xvfb 默认悬浮窗问题和正式签名 AppImage 升级继续保留。现有发布 workflow 会直接公开发布且 prerelease 只构建 DEB，本轮未触发它，也未读取签名私钥。

本机实装与收尾：

- 用户完成系统管理员认证后，dpkg 将 beta.20 升级为上述 beta.21 候选。旧后台停止后，确认 runtime owner 排他锁可获取，保存并验证停写 SQLite 备份，再启动新后台；受控重启耗时 4.06 秒，PID 从 1477 变为 426291。安装的 Desktop、daemon 与 unit 的 SHA256 均与候选 payload 一致。
- 在真实 GNOME Wayland 会话中，已安装 DEB 的设置页及受管存储控件、关闭隐藏、单实例唤回、托盘正常退出、新进程重开均通过。退出后无 Desktop 的 15 秒观测窗口中，心跳前进、成功采样时间推进 15,393 ms。
- 直接启动原始 AppImage，未设置解包运行选项；Desktop 实际运行于 FUSE `.mount_` 路径。相同的设置页、隐藏/唤回、正常退出及新进程重开流程全部通过，退出后采样推进 15,286 ms；两个实际 Desktop 挂载均已卸载。未新建 AppImage user unit 或持久运行时，继续复用 DEB 服务。
- 两种客户端的全部 14 项 UI/后台动作前后，daemon PID 与 InvocationID 保持不变，NRestarts=0。最终 15 项 managed 检查、SQLite quick_check/外键、SQLx 1–8 元数据和固定历史 session/web/title/import 摘要通过；未对生产数据执行迁移、恢复或清缓存。
- `launch_at_login=0`、`background_tracking_at_login=1` 与安装前一致，Desktop autostart 文件不存在。最终无 Desktop 进程，DEB daemon enabled/active，后台继续采样；没有要求用户再次注销或重启，也不把 beta.20 的既有冷启动结果称为 beta.21 冷启动验收。
- 持久目录中的 `acceptance-summary.json` 汇总当前证据，`service-upgrade.json`、`after-ui.json`、`final-comparison.json`、`ui-*.json` 记录具名动作；在线/停写备份和配置归档保留在同一私有目录。源码 manifest 与构建时提交 `0fe0838e` 逐文件一致，收尾只修改文档，不重复构建候选。

当前结论：**beta.21 本地 DEB 候选实装、本机 GNOME 生命周期与 AppImage FUSE/DEB 共存验收通过。** 完整发布门禁为 715 Rust passed / 18 ignored、56 个 TypeScript 文件、38 项浏览器回归与 Clippy；发布策略/DEB/installed acceptance 专项为 25/3/11 项。此结论不覆盖独立无 DEB 的 GNOME 登录、失败的 Xvfb 悬浮窗路径或正式签名升级。修复与版本准备已提交本地 main，当前公开版本仍为 beta.20；本轮未 push、tag 或公开发布，AppImage 发布门槛不解除。
