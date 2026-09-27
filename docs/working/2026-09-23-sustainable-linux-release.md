# 可持续维护的 Linux 产品版本：阶段执行单

> 建立：2026-09-23，更新：2026-09-27。本文记录阶段执行状态与证据入口；产品范围、支持承诺、架构和发布规则仍以 `docs/` 下对应长期文档为准。P1–P3、C1/R2、F1 和 DEB-only 候选准备已有隔离证据；用户已暂停 R1 的连续运行观察。后续 Tauri 运行时升级、正式出包及公开发布结果以[独立执行单](2026-09-27-tauri-runtime-refresh.md)为准。尚未在宿主安装新的正式包。

## 当前基线

- 开发主线为 `main`，接手时本地与 `origin/main` 均为 `d3a704c2193a4a0bb070985fa78b55314113e3c2`。接手时有 3 个未提交的规则文档修改：`AGENTS.md`、`docs/issue-fix-boundary-guardrails.md`、`docs/versioning-and-release-policy.md`；它们与本执行单分开审阅，不覆盖或代为提交。
- 最新公开预发布为 beta.21，仍遵循 DEB-only beta 契约；正式发布包 SHA 与先前各本地候选不同，见[运行时升级执行单](2026-09-27-tauri-runtime-refresh.md#公开发布结果2026-09-27)。源码推送、候选构建、隔离验收与公开发布分别记录。
- 已完成的 daemon 分离、本机 beta.21 DEB 候选实装、AppImage 无 DEB 的 GNOME 42.9 登录、正式签名候选的隔离升级，不作为本阶段待重做功能。详见 [daemon 当前状态](2026-07-10-patinad-runtime-design.md)和[平台验收记录](2026-09-21-linux-platform-reuse.md#正式成品验收结果与持久证据)。
- 正式签名 AppImage 的源码为 `f9fb9b8b5839cfdfdea0f82557cb0e154e818cce`，包 SHA256 为 `9cbc8e54205a92f46609f20494afdf94dafd18b65f1817952a06b45cdf627598`；[Actions 运行](https://github.com/Asanilo/patina-Linux/actions/runs/35811958233)成功。它已通过隔离 loopback 更新、原子替换、服务切换与数据保留；公开 updater 分发尚无验收。
- 私有持久证据位于 `$HOME/.local/state/patina/acceptance/20260923-appimage-gates-7kp1c2hi/` 与 `$HOME/.local/state/patina/acceptance/20260923-signed-appimage-dlahd_9e/`，入口分别为 `acceptance-summary.json`。引用时只提取候选身份与测试结论，不复制真实活动数据或凭据。运行 VM 或容器前先核对其当前状态。

## 执行队列

`[x]` 只表示已有对应证据；选定环境、写出计划或通过旧候选测试不能替代实际验收。任务按下表依赖推进，观察与核心流程复核可在平台适配期间独立进行。发现缺陷时先判 owner，再按[修复边界规则](../issue-fix-boundary-guardrails.md)选择小修、边界判断或执行单。

| ID / 状态 | 任务与顺序 | 交付物及完成判定 |
| --- | --- | --- |
| B1 [x] | 固定现有基线 | 已核对分支、3 处先存规则文档修改、候选身份、持久证据和未覆盖路径；下方索引、复用检查顺序和公开分发门槛已记录。后续改源码或包必须重新判断可复用证据。 |
| C0 [x] | 桌面数据库读取边界静态清点 | 下方记录 6 个直接 SQLite 读取文件、现有 API 覆盖与具名例外；清点不等于要求立即迁移，也不证明每条 UI 刷新路径正确。 |
| R1 [暂停] | 持续运行观察 | 用户指出同一实例连续运行两周不符合正常使用规律，要求等相关 issue 出现再处理。已有按实际使用分段的只读汇总证据保留；不继续定时采样，也不把连续运行天数作为本阶段门槛。 |
| P1 [x] | GNOME ESM 扩展入口与打包；依赖 B1，先于 P2/P3 | 同 UUID 的 GNOME 42 旧包与 Shell 46 ESM 候选分开构建，保留旧/新 D-Bus 协议；双入口合成回归、独立构建/ZIP 内容、GNOME 42 与 GNOME 46 私有 Shell 的真实窗口、锁屏/overview 和禁用/启用通过。Shell 46 ESM 已走 Ubuntu 24.04 AppImage/GDM 隔离验收；现行 DEB/Release 仍交付 GNOME 42 包，ESM 未公开分发。 |
| P2 [x] | Ubuntu 24.04 / GNOME 46 隔离验收；依赖 P1 | 准确镜像、扩展、旧/新包身份与会话/后端均已记录。无 DEB 的实际 AppImage 首次接管、双协议窗口记录、GDM 冷登录、锁屏恢复、退出 UI 后采样、正式公钥篡改拒绝与原子升级、Settings 后台切换、旧数据与恢复材料保留、升级后冷登录/窗口记录通过。公开 updater 投递、手动密码登录和宿主硬件挂起不在此项证明范围。 |
| P3 [x] | Fedora 44 / GNOME 50 隔离验收；依赖 P2 | 经校验的 Fedora Cloud 44 + GNOME 50.5/GDM Wayland 客体，使用 Tauri CLI 2.12 的准确 AppImage 和仓库 Shell 46/50 ESM ZIP，完成 FUSE 首次接管、独立 WebKit 页面、Dashboard/History/Settings 渲染与 Settings 持久化、真实 Wayland 窗口、退出 UI 后后台记录、锁屏恢复、冷登录及本地下一版本 Settings 确认重载；旧会话和设置保留。旧正式签名包的验签/篡改拒绝/原子升级证据独立保留。新 AppImage 为本地未签名 QA 候选，Cloud 补装 Noto CJK 字体，非 Workstation ISO 或公开分发验收。 |
| C1 [x] | 核心流程复核；可与 P1–P3、R1 同期 | 在隔离 GNOME 46 真实客户端完成 Dashboard、History、Settings、跨日精确导入/小时桶、排除刷新、标题隐私、备份导出/预览/Merge 恢复及损坏备份拒绝；发现并修复 Summary 当前 override 与 Linux 无 `.exe` 标题设置两个 owner 内缺陷。最终 DEB 包内 daemon 的准确字节在同一真实客户端/窗口下复验两项修复。时区/DST 的多时区自动回归通过；手动密码登录、其他系统时区、备份错误 toast 的截图不在已验范围。 |
| R2 [x] | 故障恢复与诊断 | 隔离 VM 的短断连、扩展禁用/恢复、Desktop 崩溃/重连、daemon 异常退出与锁屏恢复均有前后状态、成功采样与 session 边界；短断连时 Settings 明确警告，恢复后清除，未知间隙没有被补记且没有第二 owner。最短诊断路径见下方。此项不证明宿主硬件挂起/唤醒。 |
| F1 [x] | 修复、候选冻结与支持矩阵；依赖相关缺陷闭环 | AppImage 故障归属为旧 Tauri 打包器携带 Wayland 客户端库，固定 CLI 2.12.0 后以准确本地包完成 Fedora 重跑和 Ubuntu 24.04 页面回归；Shell 46/50 ESM 包与回归身份已固定。`docs/linux-platform-support.md` 区分当前声明范围与隔离已验收但未公开支持的环境，并列包格式、GNOME/会话和实际客户端后端。新 AppImage 未正式签名，不替代 D2 的公开渠道验收；版本文件仍为 beta.21。 |
| D1 [x] | DEB-only beta 交付准备；可在 P2/P3 之外单独收口，依赖对应候选与核心回归 | 当时使用固定的 Tauri CLI 2.12.0 重新构建本地未签名 beta.21 DEB，SHA256 `41f0742ef005e6c1cfa7487ab1e0d30527ce81c824b24f315b3a04f8a891abf7`；版本/changelog、`release:check`、daemon/扩展载荷及私有 beta.20→beta.21 安装、升级、卸载、重装与数据哨兵保留通过。包内 daemon 与先前已在隔离 GNOME 客户端验证 C1 修复的字节一致，旧新 DEB 的解包载荷和 control 文件一致。该包后来被 Tauri 核心升级候选取代，准确新包身份见下方候选更新。 |
| D2 [ ] | AppImage 公开分发决策与渠道验收；依赖 P2/P3、F1 和明确发布授权 | 决定是否调整 DEB-only 契约并审阅 workflow、双包 manifest、旧客户端 fallback；获相应授权后核对正式资产/签名、公开 manifest 实际目标与下载、失败恢复、安装后 Desktop/daemon 版本、冷登录和数据保留。隔离 loopback 的既有成功不能勾选公开渠道项。若本阶段保持 DEB-only，明确保留 D2 未完成而不阻塞 D1。 |

当前阶段：P1–P3、C1/R2、F1 与独立 DEB 候选 D1 已有证据；R1 按用户要求暂停，出现相关 issue 后再决定专项范围。D2 只在决定恢复公开 AppImage 分发且得到相应授权后进入，不阻塞 DEB-only 候选准备。上游草稿、KDE/wlroots、Flatpak、新客户端、Widget 暂停项、Windows 删除和 Cargo workspace 重排不在此队列。

**候选更新：** 上表 D1 中 CLI 2.12.0 的 DEB 已被 Tauri Rust 核心 2.11.5 的新候选替代；准确 DEB SHA256 为 `98eafa7d242dd4f83cd28752de9a2e88a32c5debe30bb79641f237fd5adae08c`，其包内 daemon 的 GNOME 真实客户端回归、隔离升级和完整门禁见[运行时升级执行单](2026-09-27-tauri-runtime-refresh.md)及私有 `$HOME/.local/state/patina/acceptance/20260927-tauri2115-candidate/manifest.json`。旧 D1 字节仅作历史证据，不能作为 beta.21 最终出包身份。

每项执行记录至少包括：任务 ID、源码提交/工作区差异、包与扩展摘要、测试环境和会话/显示后端、实际动作、预期与结果、证据路径、未覆盖项、可复用旧证据的理由。合成数据和私有日志留在 owner-only 验收目录，工作文档只存摘要。失败不改写为“待优化”；记录最先失败的边界和重跑条件，修复后仅重跑受影响专项及相应质量门禁。

Fedora P3 的 owner 是 AppImage 打包器与客体图形库边界；准确包、独立 WebKit 页面和真实客户端均已复测。启动 VM/容器前核对保留资源的状态；不为建立计划而重装宿主 beta.21、打开桌面登录自启动或重复已通过的正式签名升级。

### 本轮执行进展（2026-09-23）

- **R1 已启动，观察窗口未结束。** 2026-09-23 04:49 UTC 在已安装的本地未签名 beta.21 DEB 上建立只读基线；同日 05:26 UTC 的后续采样保持同一 service invocation、NRestarts=0、SQLite quick_check=ok。私有汇总入口为 `$HOME/.local/state/patina/acceptance/20260923-sustainable-observation/baseline.json`，后续采样使用 `scripts/acceptance/runtime-observation.py`。记录仅含计数、时间、资源与服务身份，不含标题、URL 或凭据；这段短时观察不能证明 7～14 天稳定。
- **P1 已完成到扩展边界。** 现有 GNOME 42 `extension.js`/metadata、DEB 文件映射和 Release ZIP 保持原入口；新增同 UUID 的 ESM 源与独立 `build-esm`/`install-esm` 路径，候选 metadata 当前只声明 Shell 46。在 GNOME 42 宿主尝试 ESM 安装会在写入前拒绝。双协议合成回归、构建/语法检查与 `npm run check` 已通过；私有 GNOME 42 Shell 再验通过双协议、overview/锁屏恢复和三轮禁用/启用。一次 headless 夹具未取得前台窗口后原样重跑通过，失败日志保留于 `/tmp/patina-gnome-acceptance-5pd6rvgq/`。
- **GNOME 46 扩展隔离验收通过。** 官方 Ubuntu 24.04 容器基底 `sha256:008173c23f95b170204355c12626cb5a965d779a7e1283b09e9cffbb1bf33ca3` 中的 GNOME Shell 46.0，使用只读仓库、无网络、私有 D-Bus 与 headless Wayland，验证 ESM 扩展的双协议、合成前台窗口、overview/锁屏恢复及三轮禁用/启用。扩展源码 SHA256 `a46d0026235e6cd5a82609fd446a9467938a3e08812a20e559082394400a5186`；私有持久摘要 `$HOME/.local/state/patina/acceptance/20260923-gnome46-esm/result.json`。此单项不替代随后完成的 GDM/AppImage P2 验收或公开发行验证。
- **P2 的首次安装、前台与冷登录。** 独立 Ubuntu 24.04 KVM VM 使用官方 cloud image SHA256 `612b2c0cc1bc413a6cb8c38fd611794caf0f2b436c50013d8b3794db12ad7354`，无 Patina DEB；扩展 ESM 候选 ZIP SHA256 `d162a9fe482095716d2bcff58028e22a000cface5db1329aeef1d8ba250cb8b5`。准确正式签名 AppImage SHA256 `9cbc8e54205a92f46609f20494afdf94dafd18b65f1817952a06b45cdf627598` 首次 FUSE 接管、GDM Wayland 冷登录、user service 新实例和两次退出 Desktop 后持续采样通过；客户端后端均为 X11/XWayland。原生 Wayland 合成窗口的标题/PID 由扩展识别并进入 daemon 实际 session。私有汇总 `$HOME/.local/state/patina/acceptance/20260923-gnome46-esm/acceptance-summary.json`。首次登录的扩展目录归属、合成窗口缺 GTK GI、overview 遮挡均属于夹具失败，修复后保留失败记录并重跑通过。
- **P2 的正式签名升级与锁屏恢复。** 从安装前快照重新建立旧本地未签名 beta.20 `84da760d…38de75`、合成窗口历史和只读数据基线。在同一 VM 中运行产品正式公钥的 opt-in Tauri 下载/原子安装专项：篡改拒绝且旧包未动，有效签名候选升级为上述 beta.21 包并保留旧包。实际启动新 Desktop 后，设置页确认“重新加载后台”完成 daemon 版本切换；旧历史逐行、偏好、schema、unit 与恢复材料不变，退出 UI 后继续采样。再次冷启动 GDM Wayland 后，新包自启动、后台版本及新原生窗口记录通过。logind 锁定/解锁客体会话时，扩展报告锁屏、成功采样停止，解锁后采样和真实窗口记录恢复；daemon invocation 不变。私有升级汇总 `$HOME/.local/state/patina/acceptance/20260923-gnome46-esm/upgrade-summary.json`，详细证据在同目录 `upgrade-guest/`。这是隔离 loopback 签名升级，不是公开 updater 渠道；手动密码登录、真实硬件挂起与正式分发仍未验收，不能据此扩大公开支持承诺。
- **R2 隔离故障路径已覆盖三类，观察与硬件边界待补。** 同一 VM 内，合成 Wayland 窗口活动时 SIGKILL 确认过的 daemon MainPID，service 以新 invocation 恢复且只有一个 daemon；两段 session 不重叠、累计时长有界、SQLite 完整并继续成功采样。第一次用 `systemctl --user kill` 时它实际触发重启，但因辅助进程返回错误退出；失败保留后改为只对 MainPID 发信号并重跑通过。另验证 SIGKILL Desktop 后后台 PID/invocation 不变、无界面采样继续，重开 Desktop 不替换后台；禁用 GNOME 扩展时两个 D-Bus 名称释放、成功采样时间停止且诊断不可用，重新启用后诊断恢复，并通过新的真实窗口再次写入 session。私有结果见上述目录的 `guest/fault-ad0bd728.json`、`guest/client-fault.json`、`guest/extension-fault.json` 与 `guest/sample-post-extension.json`。可复跑入口为 `scripts/acceptance/appimage-gnome/{fault,client_fault,extension_fault,sample}.py`；真实宿主挂起/唤醒和 7～14 天稳定性不能由这些隔离故障替代，R2 仍未全部完成。
- **P3 Fedora 44 / GNOME 50.5 后端路径通过，客户端 UI 受阻。** 官方 Fedora Cloud 44 x86_64 镜像 SHA256 `28680fe5b371a5a82ebf43a31926e086a168e59949d03969c5093e7071f90b7f` 经官方签名校验后，在独立 KVM 客体中配置 GDM Wayland；无 Patina DEB。测试用同源 ESM 扩展只将 metadata 的 Shell 版本改为 50，ZIP SHA256 `080f869a6c048e24f32bb0b8b7737e695055d8275d06b10ca36f64ca5a49b387`，仓库候选仍只声明 46。正式签名 beta.21 AppImage 的 FUSE 首次接管、后台持续采样、原生 Wayland 前台窗口、GDM 冷登录和锁屏/解锁恢复通过；Desktop 为 X11/XWayland。安装前快照上的旧 beta.20→beta.21 正式公钥验签、篡改拒绝、原子替换与旧包保留通过。Settings WebView 实际显示为空白，WebKit 子进程报 `Could not create default EGL display: EGL_BAD_PARAMETER`；启用无障碍及有界软件渲染/DMABUF 回退未解决。使用受支持的本地 API 仅验证后端重载：版本切换、旧历史/偏好/schema/unit 保留及新真实窗口记录通过，**不能代替 Settings UI 确认**。归属已收窄到 AppImage WebKit/GTK 运行时与 Fedora 图形栈的组合，具体修复仍待定；P3 保持未完成，Fedora 不进入支持承诺。私有证据位于 `$HOME/.local/state/patina/acceptance/20260923-fedora44-gnome50/{initial-guest,upgrade-guest}/`，关键结果为 `upgrade-guest/acceptance/upgrade-api-reload.json` 和 `signed-desktop.log`。复测先验证客户端实际 Dashboard/History/Settings 可渲染并可操作，再重跑 UI 服务切换、数据与冷登录验收；只通过后端 API 不勾选 P3。
- **收尾：** GNOME 46 测试 VM 已正常关机，专用工具容器停止，私有磁盘与安装前快照保留供升级复跑。宿主生产 daemon 仍为原 MainPID 426291 / InvocationID `54a3d8ae56354a7aa20ecdeb5d862cd4` / NRestarts=0；桌面登录自启动关闭、后台登录追踪开启。新扩展未安装宿主，未改变生产数据或公开发布状态。

### 续测进展（2026-09-27）

- **R1 分段。** 宿主仍安装 beta.21 DEB；原包 SHA256 `1400104ad8c1c87a52283ede414f2af42bb779ff7b7db8b7421be9db4eb58cb2` 再次核对一致。05:53 UTC 汇总快照的 SQLite quick_check=ok、session_count=55329，daemon active、NRestarts=0；但当前 InvocationID `272f70f5224f4f488c44e5623663dd07` 与 9 月 23 日基线不同。user journal 显示期间多次正常启停，因此按服务实例分段，不能把四天间隔记成同一实例连续观察。新快照为 `$HOME/.local/state/patina/acceptance/20260923-sustainable-observation/sample-20260927T055345Z.json`。
- **P3 归属复核。** 在相同 Fedora 44/GDM Wayland 客体里，系统 WebKitGTK 2.52.5 的独立最小页面在 virtio-vga 和标准 VGA 下均能加载；让同一页面继承正式 AppImage 的库路径与工作目录后，两种虚拟显卡下均复现 `Could not create default EGL display: EGL_BAD_PARAMETER`、空白页面。禁用 WebKit 加速合成仍未恢复。包内旧 GLib 还与 Fedora 的 Python GI 依赖出现符号不匹配，说明直接混用系统和包内图形库不是可靠修复。先前“可能是 virtio 图形夹具”的判断已收窄为 **AppImage 携带的 WebKit/GTK 运行时与 Fedora 图形栈的兼容边界**；尚未确定最小可发布修复，不调整支持承诺或把临时环境变量写入产品默认值。复测须使用新的准确 AppImage 候选，在 Fedora 客体先通过独立 WebKit 页面，再通过实际 Dashboard/History/Settings 与升级 UI。
- **C1 实际客户端复核与缺陷修复。** 在保留的 Ubuntu 24.04/GNOME 46 隔离客体中，正式签名 beta.21 AppImage 的 Dashboard、History、Settings 均可操作；通过真实系统选择器导出并预览备份，Merge 恢复后旧合成记录逐行不变、SQLite 完整、daemon 重启后新的原生 Wayland 窗口继续记录。损坏备份在预览阶段未进入确认，旧记录和 daemon 实例不变；错误 toast 未留到截图，不将其用户可见文案记为已验。真实 Settings 导入一条跨午夜精确会话和一条小时桶：Dashboard 当日显示合成应用 40 分钟，History 前后两天各显示 10 分钟，桶未进入精确时间线；API Summary、Daily Apps 和 Heatmap 在未排除时给出对应总量。排除该应用后，History 自动隐藏、Daily Apps/Heatmap 前一日归零，但旧候选的 `/summary/range` 仍错误返回 10 分钟。真实 owner 为 `data/repositories/activity_read_model.rs` 的旧设置读取；已在该 owner 内补读当前应用 override 的分类/排除语义并加具名回归，Rust 定向测试和 `npm run release:check` 通过。**修复尚未在正式签名 AppImage 成品中复验，C1 暂不勾选。** 私有摘要和截图在 `$HOME/.local/state/patina/acceptance/20260927-c1-r2/`，不含真实活动标题或凭据。
- **R2 短断连补测。** 仅在同一 Ubuntu 隔离 VM 中停止受管 `patinad.service`；Settings 明确显示后台未运行，成功采样时间停止且没有第二 daemon。重新启动后变为新 InvocationID，Settings 警告消失、成功采样恢复，SQLite quick_check=ok，未知间隙没有跨越两端的 session。此前 daemon SIGKILL、Desktop 崩溃、扩展禁用与锁屏路径的证据仍有效；宿主硬件挂起/唤醒和持续观察门槛未由此替代。
- **C1 标题隐私缺陷与修复联调。** 隔离客户端的分类页把 `python3` 的标题记录设为关闭后，旧签名候选仍保存合成窗口标题。owner 是 `data/repositories/tracker_settings.rs`：旧读取器一律补 `.exe`，未读到 Linux UI 写入的 `__app_override::python3`。源码改为优先读取实际可执行名，并保留 `.exe` 旧键后备；具名 Rust 测试通过。本地重编 daemon SHA256 `a71292c89e33d3f9614ab194d05ad43764b114637a7340f0555a1018962be081` 曾在长时间无输入的 VM 会话中因 AFK 未生成 session，不能把两次无 session 记为产品失败；向客体发送真实虚拟输入后，在真实 GNOME Wayland 前台窗口下验证 session 仍记录、标题字段与样本均为空。同一候选 daemon 对导入事实的 Summary 排除返回 0，撤销排除后回到 10 分钟。临时 user-service 覆盖自动移除，原 AppImage daemon 和 unit 恢复。私有摘要为 `$HOME/.local/state/patina/acceptance/20260927-c1-r2/{privacy-c1-fixed-4,candidate-daemon-c1-fixed-4}.json`。这是本地 daemon 字节联调，不等同于新的完整 AppImage 或公开包验收。
- **R1 续采与 D1 门禁。** 07:18 UTC 的同候选宿主汇总快照与 05:53 UTC 相比保持同一 InvocationID、NRestarts=0，成功采样时间推进、session 数量增加 31、SQLite quick_check=ok；仍只是本次开机的一小段观察，不补写成 7～14 天。`npm run release:check` 已在两个 Rust 修复及具名回归后通过，Rust 718 项通过、20 项按原配置忽略；本地未签名 DEB 的准确最终字节、私有 dpkg 升级结果另在 D1 候选固定后记录。首次打出的 `fb7b6604…5689b7` 包早于标题隐私修复，已作废为最终候选，但保留其静态验收证据；不得与后续同版本包混用。
- **R1 日志异常记录。** 9 月 23 日和 26 日的宿主 user journal 各有一次音频会话查询超时；本轮只读核对未见 daemon 因这两次日志退出，后续按音频参与来源继续观察。多次其他正常启停与 9 月 27 日的新 InvocationID 已按服务实例分段，不据此声称候选连续运行四天。
- **D1 DEB-only 候选已准备、尚未发布。** 最终本地未签名包 SHA256 `9fb32ba39d5ca833d816039436eb7271112bae2028a444a31dd83676fdbd122a`，包内 daemon SHA256 `69eed3951a53a0c6958062a29c21c96423536cda5ada4f1c5e25a5d7b958218c`；源码基点为未提交工作区所在的 `main@d3a704c2`，修复源文件摘要和前一版作废包记录在 `$HOME/.local/state/patina/acceptance/20260927-deb-candidate/manifest.json`。`release:check`、`release:verify-daemon-deb`、扩展资产核对均通过；以已核对的 beta.20 DEB SHA256 `1cc4ebbb79ec32774732f9cf8b46c2b7bfa04c6615b33869bd7868639d855fd5` 为基线，私有 dpkg 根目录的安装→升级→卸载→重装与合成数据哨兵保留通过，见同目录 `isolated-dpkg-evidence.json`。从最终 DEB 解出的 daemon 原字节在隔离 Ubuntu 24.04/GNOME 46 的真实客户端下，重验标题隐私、导入 Summary 排除与撤销、服务恢复，见 `candidate-daemon-deb-exact-1.json` 和 `privacy-deb-exact-1.json`。该 runner 不证明新 DEB GUI、生产 systemd 接管、公开签名/updater 或宿主安装；宿主已装旧 beta.21 包 SHA256 `1400104a…eb58cb2` 仍未替换。本地候选同版本异字节，后续提交或发行前必须按确切 manifest 重新审阅，不能拿旧候选证据代替。
- **P3 AppImage WebKit 故障闭环。** 旧签名包的 AppDir 副本在 Fedora 44/GNOME 50.5 用相同 WebKit 最小页面复现 `EGL_BAD_PARAMETER`；只移走副本中的 `libwayland-client.so.0` 后，页面出现 `started/committed/finished`。归属与 [Tauri #15976](https://github.com/tauri-apps/tauri/issues/15976) 一致；[Tauri #16062](https://github.com/tauri-apps/tauri/pull/16062) 已合入，仓库固定 `@tauri-apps/cli` 2.12.0。新本地未签名 beta.21 AppImage SHA256 `2a05f5c33651eea6a463e88eb91928be98eb8cfc4b983418eba1d68b1461fcfd` 不含该库；从准确包解出的库再次通过独立页面探针。该包在 Fedora 干净快照中先完成 FUSE/runtime 接管，再重启 Desktop；Dashboard、History、Settings 有真实截图，History 出现实际 Patina 记录。Cloud 客体缺少 CJK 字体，安装 Fedora 官方 `google-noto-sans-cjk-vf-fonts` 后中文可读。Settings 用实际鼠标把活动保持时间 3→4→3 分钟，数据库值 180→240→180 秒且 quick_check=ok。新打包器未再强制 `GDK_BACKEND=x11`，客体 `ss -xap` 显示 Patina 与 GNOME 的 `wayland-0` 套接字成对连接；这与旧签名包的 XWayland 客户端结论分开记录。
- **P3 版本切换、冷登录与扩展候选。** 为测试同版本异字节保护，临时构建未签名 beta.22 QA AppImage SHA256 `7e0ad0cb0da723e7cc0b6094e3f0276c223cf34fee3f02e6e022466029f785eb`，之后把 Cargo.toml/Cargo.lock 版本恢复 beta.21，未修改正式版本契约。Fedora Settings 先显示 Desktop beta.22/Daemon beta.21，真实点击确认“重新加载后台”后变为新 service InvocationID 与 beta.22/beta.22；旧会话 ID 1、2 保留，设置、8 项迁移及 SQLite/外键检查正常，两份 runtime 均保留。新 boot 的 GDM Wayland 自启动、退出 UI 后真实原生窗口记录、锁屏停止/解锁恢复、新 boot 再记录通过。仓库 ESM 候选源码 `extension.js` SHA256 `a46d0026235e6cd5a82609fd446a9467938a3e08812a20e559082394400a5186`，metadata SHA256 `9157fcaad55c4fab0df3af2a45e1741ea014629604ab2e18e90a03ab2baae2c7`，46/50 ZIP SHA256 `fa0b089a2273227d66bdd589221029f15f68bd11b550fec172db3254cd9b1481`；准确 ZIP 在 Fedora 重启后为 ACTIVE 并记录新窗口，GNOME 46 无网络私有 Shell 的双协议/锁屏/overview/三轮禁启回归通过。Fedora VM 保留 `pristine-fedora44-ready`、旧失败 `post-upgrade-webkit-fail` 和新通过 `post-tauri212-fedora-pass` 快照；容器已停止。私有截图和 JSON 位于 `$HOME/.local/state/patina/acceptance/20260927-fedora44-tauri212/`。新 QA 候选没有产品签名，不能把本地 UI 重载写成正式公钥升级或公开 updater 验收。
- **F1 跨发行版回归与边界。** 同一 beta.22 QA AppImage 在 Ubuntu 24.04/GNOME 46 隔离 VM 显示 Dashboard 和旧活动卡片；VM 已恢复回归前快照并停止。`docs/linux-platform-support.md` 将 Fedora 写为已通过隔离技术验收、尚未进入公开支持承诺，标注 Cloud 基底、GNOME/会话、包格式、显示后端和 CJK 字体夹具。AppImage 打包器和 ESM 元数据修改后的 `npm run release:check` 通过：Rust 718 项通过、20 项按原配置忽略，Clippy、扩展、版本/changelog 门禁通过。原 beta.21 DEB 候选摘要不因这次变动而冒充新构建；D2 的正式签名、公开资产与渠道测试仍未执行。
- **候选字节保留。** 由于临时 beta.22 打包会清理 `target/release/bundle/appimage` 中的上一版本文件，两份准确未签名 QA AppImage 已单独保存到上述 2026-09-27 Fedora 私有证据目录，文件名分别为 `Patina_1.9.0-beta.21_tauri212_amd64.AppImage` 和 `Patina_1.9.0-beta.22_tauri212_amd64.AppImage`，复制后 SHA256 与上文一致。它们不改变 beta.21 DEB 候选、宿主安装或发布状态。
- **D1 用当前打包器重新冻结。** 在版本文件仍为 beta.21、Rust Tauri 仍为 2.10.3 的源码上，固定 CLI 2.12.0 重新打出 DEB SHA256 `41f0742ef005e6c1cfa7487ab1e0d30527ce81c824b24f315b3a04f8a891abf7`，包内 daemon SHA256 `69eed3951a53a0c6958062a29c21c96423536cda5ada4f1c5e25a5d7b958218c`。`release:verify-daemon-deb` 和准确新包的私有 dpkg 安装→升级→卸载→重装通过；旧候选 `9fb32ba3…d122a` 与新包的解包载荷及 control 文件逐项一致，故旧包内 daemon 的 GNOME 真实客户端专项仍适用于新包相同字节。新包及私有证据保留于 `$HOME/.local/state/patina/acceptance/20260927-deb-refreeze-cli212/`；此项不表示宿主安装、签名或公开发布。

R1 历史采样命令（当前暂停；只有相关 issue 出现并明确恢复后再使用，私有目录须由当前用户拥有且权限为 0700）：

```bash
python3 scripts/acceptance/runtime-observation.py \
  --database /home/arinp22/.local/share/Patina/patina.db \
  --output-dir /home/arinp22/.local/state/patina/acceptance/20260923-sustainable-observation \
  --candidate-sha256 1400104ad8c1c87a52283ede414f2af42bb779ff7b7db8b7421be9db4eb58cb2
```

若本机数据目录、包字节或 service invocation 变化，先核对实际 owner/候选再继续，并从变化点分段，不盲目复用上述路径或摘要。

### R2 最短诊断路径（隔离故障路径与后台断连提示已实测）

1. **后台停止：** 先看 `systemctl --user show patinad.service` 的 ActiveState、MainPID、InvocationID、NRestarts，再看该 unit 的 `journalctl --user -u patinad.service`。对照 `/api/v1/health` 的版本与 `/api/v1/capabilities` 的 tracking/service owned+ready；不要仅因 unit active 就判断实际采样正常，也不要直接启动第二个 daemon。
2. **GNOME 扩展失效：** 核对 logind 当前图形会话、Shell/扩展版本和启用状态，再核对新旧 D-Bus 名称与 `/api/v1/diagnostics` 的 `window_tracking`、`tracker_runtime`。`gnome-extension-dbus-unavailable`、未知 session type 和响应损坏要分别定位；仅有名称 owner 仍须用合成或授权的真实窗口记录验证。私有窗口标题不进入公开日志或文档。
3. **Desktop 断连：** 先区分 API 无响应、401 凭据/端口变化、协议不兼容与 SSE `resync-required`；重连后重新读取 capabilities、当前运行时快照和对应页面读模型。隔离 VM 已验证服务停机提示与恢复清除；401、协议不兼容及页面重连的细分反馈未逐项实测。不能改走未授权的前端 SQL 写入。
4. **恢复核对：** 按 R1 脚本记录动作前后 service、成功采样时间、数据库完整性和 session 边界。未知间隙不补记，必须没有第二个 owner、重复或虚增长记录。隔离 VM 的 daemon、Desktop、扩展与锁屏路径已有上述实测；用户可见反馈、宿主硬件恢复仍待核验。

### 待测环境选择

| 目标 | 选择理由与起始包 | 验证前置条件 |
| --- | --- | --- |
| Ubuntu 24.04 LTS，GNOME Shell 46，amd64 | 在现有 Ubuntu 22.04/GNOME 42 基线上先验证 [GNOME 45 起采用的 ESM 扩展接口](https://gjs.guide/extensions/upgrading/gnome-shell-45.html)；[Ubuntu 24.04 发行说明](https://documentation.ubuntu.com/release-notes/24.04/)确认 GNOME 46。优先用隔离 AppImage，避免把未经验证的 DEB 直接安装宿主。 | 当前扩展 `metadata.json` 只声明 Shell 42，`extension.js` 是旧入口；须先实现并测试 ESM 入口及打包，再在客体核对实际 `gnome-shell --version`、Wayland 会话和 AppImage 客户端后端。 |
| Fedora Workstation 44，GNOME Shell 50，x86_64 | [Fedora 官方下载页](https://www.fedoraproject.org/workstation/download/)提供 44；[Fedora 测试说明](https://fedoraproject.org/wiki/Test_Day%3A2026-02-11_GNOME_50_Desktop)标明 Workstation 44 / GNOME 50。用独立发行版检验 AppImage 依赖、扩展、systemd 用户服务和登录链路。 | 先完成 Ubuntu 的 ESM 协议闭环，再以官方镜像校验和启动隔离 VM；初始只测 AppImage，不将 Debian 包安装步骤等同于 Fedora 支持。记录客体实际 Shell 小版本、Wayland/XWayland、CPU 架构和包哈希。 |

表内保留选型时的起始条件；当前结果以上方执行进展为准。Ubuntu 24.04 / GNOME 46 和 Fedora Cloud 44 + GNOME 50.5 已完成各自的隔离验收，ESM 与 AppImage 均未公开分发。GNOME 42 仍是已声明的扩展范围。

## 候选与证据索引（2026-09-23）

下表的 SHA256 是包字节身份，版本号相同不能据此互换。`amd64` 均指 x86_64；源码提交是构建记录所载的基准，不把后续文档提交算入包源码。私有目录仅提供索引，不将原始活动、凭据或日志复制进仓库。

| 候选与构建来源 | 源码 | 包 SHA256 | 签名与环境 | 已有结论 / 证据入口 |
| --- | --- | --- | --- | --- |
| 本地 beta.21 DEB，宿主已安装 | `0fe0838e` | `1400104ad8c1c87a52283ede414f2af42bb779ff7b7db8b7421be9db4eb58cb2` | 本地未正式签名；amd64，宿主 GNOME Wayland | beta.20→beta.21 安装、数据保留、受管服务、Desktop 生命周期与无 UI 采样通过；`$HOME/.local/state/patina/acceptance/20260923-beta21-d39le7kc/acceptance-summary.json`。后续 AppImage 修复未进入此安装包。 |
| 本地 beta.21 AppImage，崩溃修复候选 | `d3418d1d` | `dae1bb5a3a756cc91223759425498850c09f6ffb2fdf93f49c7de41d1097bd8a` | 本地未正式签名；x86_64，隔离 X11/systemd 与无 DEB 的 GNOME 42.9 Wayland VM，GTK 为 XWayland | 默认悬浮窗、首次接管、冷登录、真实前台记录通过；`$HOME/.local/state/patina/acceptance/20260923-appimage-gates-7kp1c2hi/acceptance-summary.json`。未安装宿主。 |
| Actions 正式签名 beta.21 AppImage，run `35811958233` | `f9fb9b8b5839cfdfdea0f82557cb0e154e818cce` | `9cbc8e54205a92f46609f20494afdf94dafd18b65f1817952a06b45cdf627598` | 产品正式公钥独立验签；x86_64，隔离 systemd 与无 DEB 的 GNOME 42.9 Wayland VM，GTK 为 XWayland | 篡改拒绝、原子替换、设置页后台重载、旧历史/偏好保留、冷登录及记录通过；`$HOME/.local/state/patina/acceptance/20260923-signed-appimage-dlahd_9e/acceptance-summary.json`。远端 artifact 只短期保留，本地证据仍在；无 tag、Release 或公开 updater 投递，未安装宿主。 |

这些结论来自现有验收记录与本地持久摘要，本轮未重跑包或宿主安装验收。首次接管的旧 beta.20 基线是本地未签名 AppImage，并非公开 beta.20 AppImage；升级证明不能扩大为“公开渠道升级已通过”。

## 复用检查顺序与公开分发门槛

1. **冻结身份。** 记录源码提交、包和签名 SHA256、格式/架构、构建来源、目标发行版和 GNOME Shell、会话类型及客户端实际后端；比对包字节后才复用证据。已安装 DEB 与正式签名 AppImage 是不同字节，不按 beta.21 字符串互相覆盖或降级数据库。
2. **隔离安装和记录。** DEB 使用 `docs/linux-development-setup.md` 的私有 dpkg 验收入口；AppImage 先用同文的 startup/systemd 检查，再按 [`appimage-gnome/README.md`](../../scripts/acceptance/appimage-gnome/README.md) 在无 DEB 的图形环境确认实际窗口记录、退出 Desktop 后持续采样、冷登录和服务唯一 owner。D-Bus owner/心跳只作诊断，不代替 session 记录。
3. **升级和恢复。** 动作前记录旧包、service 版本/invocation、备份及 SQLite 完整性；执行对应包格式的升级，核对 Desktop/daemon 版本、历史与偏好、schema、unit、服务切换和无 UI 采样；核对旧包恢复材料。失败路径需确认旧包未被破坏或可恢复，不自动把旧二进制接到较新数据库上。正式签名 AppImage 的隔离升级入口和限制见上述签名摘要与验收 README。
4. **公开渠道仍待验收。** 只有在获准修改发布契约并形成准确发行候选后，才能验证实际公开 updater manifest 的 AppImage/DEB 目标匹配、旧客户端 fallback、正式签名、真实下载及升级失败恢复；再核对实际运行版本、登录、数据和后台记录。隔离 loopback 下载不能替代此步骤。发布流程的 tag、推送、公开 Release 与宿主安装分别遵守现有授权边界；通过后才更新支持/发布承诺。

## Desktop SQLite 读取与接口缺口（静态清点）

`src/` 中直接 `getDB()` 和 SQL 查询只落在 `src/platform/persistence/*`；`sqlite.ts` 是底层适配器，其余 6 个文件有直接读取。下表按真实 owner/用途记录，不把读取例外误报成第二 runtime owner；写入侧仍遵循[架构 4.3](../architecture.md#43-前端本地-sqlite-通道)。本轮只作静态清点，未验证每条 UI 刷新路径。

| 直接读取文件 | 用途与保留原因 | 已有边界 / 缺口与处理 |
| --- | --- | --- |
| `sessionReadRepository.ts` | Dashboard、History、详情的原生/导入事实合成、标题样本、图标；目前 Desktop 读模型仍组合精确记录与小时桶。 | API 有 `/sessions`、聚合接口，但 `/sessions` 只暴露原生精确记录，不能直接替换该组合。先核对跨页面日期/覆盖口径，发现真实不一致再设计 daemon 共享读模型。 |
| `webActivityRepository.ts`、`dataWebActivityTrendRepository.ts` | History、Data、网站详情的网页段、域名统计/覆盖设置和趋势。 | `/web-activity` 已有有界读取，但 Desktop 的本机组合与覆盖设置不等同于该端点；先复核实际筛选、隐私和刷新语义，再判断接口形状。删除仍走 command。 |
| `classificationPersistence.ts` | 分类设置读取、旧版完整历史迁移所需的 exe 名称。 | 近期观察及专用迁移观察已有 command/API，完整旧迁移读取仍是具名例外；不能因新端点存在就静默改变旧迁移语义。删除走 command，缺 command/daemon 时不回退 SQL。 |
| `settingsPersistence.ts` | Settings 全量设置和追踪健康时间戳；WebDAV 配置也复用该读取。 | API 仅公开有限的 tracker/runtime/local-api 视图；全量设置可能含敏感字段，不以开放原表作迁移捷径。写入走受管 command，先审查失败反馈与状态刷新。 |
| `dataBootstrapSnapshotStore.ts` | Desktop 私有、可重建的 Data 首屏快照缓存读写。 | 架构明确保留的本机缓存例外；无需为该缓存新建 runtime API，不得承接追踪数据写入。 |

`dailyAppsRepository.ts`、`dailyActivityRepository.ts` 和 `observedAppsRepository.ts` 已通过 command 读取，不列入直接 SQLite 清单。迁移优先级由可复现的统计偏差、刷新/错误处理或实际新客户端需求决定；当前清点不授权跨层重构。

## 阶段收口

分别报告以下成果，不把“候选准备”“通过隔离验收”“公开发布”合为一个状态：

1. **可维护的 DEB-only beta 候选：** C1、R2 的相关缺陷闭环，D1 具有准确候选身份、DEB 安装/升级/恢复证据和可复跑门禁。R1 已按用户要求暂停，不作为本阶段完成门槛。发布政策的最低门槛适用于实际发布准备；架构/Rust runtime 改动执行 `npm run check:full`，前端交付执行 `npm run check`。已通过的门禁只因新改动、失败或剩余风险重跑。
2. **有限平台支持清单：** P2、P3 各有真实窗口记录、服务/登录/升级证据和失败归因；F1 更新支持矩阵，分别写清 GNOME Shell、发行版、安装格式、会话与客户端显示后端。未通过的环境不能写为支持，也不能把 GNOME 42 或 Ubuntu 结果转写成 Fedora 通过。
3. **可复用维护资料：** 候选索引、版本/签名与证据路径、后台停止/扩展失效/客户端断连的诊断路径、核心页面与备份恢复回归步骤，以及有界的未覆盖项。受控 Desktop SQLite 读取可以保留明确例外。
4. **公开渠道状态：** 仅在相应授权后执行 tag、推送、公开 Release 或宿主安装。若 D2 未执行，明确写“DEB-only beta，公开 AppImage updater 未验收”；获授权且实际渠道、成品、运行时与数据均核验后才写“已公开发布”。
