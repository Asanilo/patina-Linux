# Linux 平台草稿评估与回流

状态：T6 模块评估和首片修复完成，idle 与协议兼容仍待实现；beta.20 源码发布准备已通过完整门禁，尚未打 tag 或公开发布。产品主线为 `main`，评估基准为 `5eefcf4e`；贡献草稿位于 `feat/linux-desktop`，基于上游 `80204c73`，另含未提交改动。草稿没有完成整体 Rust 集成或实机验收，不视为可直接合入的实现。

本轮按用户确认的顺序推进主线远端同步、平台模块评估和下一版 beta 准备。分支与产品范围遵循 [路线](../roadmap-and-prioritization.md#linux-main-and-daemon-experiment)，主 Todo 与发布证据由 [当前清单](2026-07-10-patinad-runtime-design.md) 管理。保持贡献 worktree 原状，不整支合并、不安装或切换 GNOME 扩展，不操作生产服务或数据库。

## 模块判断

| 模块 | 判定 | 主线落点与条件 |
| --- | --- | --- |
| GNOME 采样状态：窗口、无窗口、锁屏、不可用、未知 idle | 复用状态设计与合成场景，重写集成 | `platform/linux/foreground` 负责观测，`engine/tracking` 负责计时；保留主线现有 X11 支持与 daemon owner，不复制整套平台门面 |
| 采样中断与恢复 | 在 main 内修复，草稿不能直接回流 | 草稿 tracking 主要改动是 import 路由，poller 尚未接通新的 Linux 接口；“恢复不补记未知间隙”仍需在现有会话 owner 实现并验证 |
| GNOME 扩展的锁屏/overview、字段边界与 disable 清理 | 复用纯逻辑及测试场景 | 主线旧协议、旧客户端与已安装扩展必须兼容；JS 单元通过不代表真实 Shell 行为已验收 |
| `WindowTracker1.GetSnapshot` 协议 | 需兼容设计后重写适配 | 当前 main 使用 `org.patina.WindowTracker.GetFocusedWindow`；新旧协议及打包能力应分阶段接入，不能原位替换同 UUID 扩展后使旧客户端失联 |
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

- idle 可用性：观测错误保留为未知，Wayland 不用 X11 idle 掩盖 Mutter 失败；先确定失败输出与诊断，再接入第一片的计时边界。
- 扩展兼容：先保留旧协议可用性，再设计新接口检测及字段校验；按实际 Shell 版本分别验证，不直接替换已安装扩展。
- 下一 beta 的范围和版本准备以最终已验证差异为准；不会复用公开 beta.19 资产或提前解除 AppImage 门槛。打 tag、公开发布与本地源码/门禁分别记录。

## 并行小修：保持旧协议的扩展修正

仅删除活动标题/app 的 Shell 日志，并将信号构造改为原声明的 `(sssut)`；保持 UUID、旧方法和信号名、GNOME 42 支持范围及窗口身份规则。扩展因实际修复独立升至 version 3，与草稿中同为 version 3 的新协议不是同一实现，不能混用资产。合成测试检查日志隐私、RPC/信号字段与实际发射 payload；本轮不安装或启用扩展。

限长、锁屏/overview 状态、新协议协商和生命周期重构保持后续片段，避免把兼容小修扩成协议替换。

## 下一版 beta 准备范围

远端已核对：最新公开预发布 `v1.9.0-beta.19` 的标签提交为 `fbdad8eb`，尚无 beta.20 标签。计划准备 `1.9.0-beta.20`，涵盖已合入的后台目录维护、runtime 写侧/事件修复及本轮通过验证的两项小修；仅 DEB 与既有扩展资产，不新增桌面/发行版承诺。

版本文件与 changelog 已同步为 beta.20，完整 `release:check` 已通过；准备提交、推送 main、推 tag 和公开发布分别记录。现有已安装本地 beta.19 候选本轮未替换。

## 本批验证与限制

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
- [ ] idle 可用性与扩展协议兼容的后续实现。
