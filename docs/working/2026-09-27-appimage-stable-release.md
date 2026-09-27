# AppImage 公开分发与 1.9.0 正式版执行单

状态：进行中。用户已要求把工作推进到正式发布并完成 AppImage。起点为已发布的 `v1.9.0-beta.21`（DEB-only）和干净的 Linux `main` `b1aa38eb`。R1 两周连续运行观察仍按用户要求暂停，不作为本轮门槛。

## 发布边界

- 维持 GNOME Wayland 和 Debian 系当前支持承诺；Ubuntu 24.04/GNOME 46、Fedora 44/GNOME 50 的隔离 AppImage 通过记录不自动扩大支持矩阵。
- 先用正式签名的双包预发布候选验证真实 GitHub 资产、签名、版本选择、旧 AppImage 回退目标、安装与失败恢复；通过后再准备 `1.9.0` 稳定版。预发布候选使用独立版本与 tag，不覆盖 beta.21 或现有稳定包。
- 正式包由 GitHub Actions 从确定 tag 构建并签名。本地未签名包只供隔离 UI/打包检查；发布后按公开下载字节重新验证，不复用本地 SHA 作为成品身份。
- 不安装到宿主生产配置，不改动真实活动数据；涉及安装和升级的检查在隔离 KVM 用户配置中完成。公开发布事实与宿主安装分别记录。

## 顺序和完成条件

1. 审计 AppImage 持久 AppDir、systemd owner、原子替换和当前 updater 的 bundle 目标选择。复用已通过的 GNOME 42/46/50、正式签名 loopback 证据，仅补新版本或新渠道引入的风险。
2. 将发布契约改为：`beta` 继续 DEB-only；用于公开渠道验收的 `rc` 和稳定版均交付 AppImage、DEB 与三项 Linux updater 目标（AppImage 专用、DEB 专用、旧客户端通用回退）。发布脚本、工作流、测试、README 与长期发布规范同步。
3. 冻结 `1.9.0-rc.1`，完成版本/changelog、`test:release`、`release:check`、包载荷和独立签名检查，再发布双包预发布候选。核对真实下载的字节、签名及 manifest URL。以旧客户端和新包格式分别验证目标选择；在隔离 GNOME 客体测试正式 AppImage 下载/升级、冷登录、单一 daemon owner、旧数据和失败恢复。
4. 根据候选结果收口 `1.9.0`：从完整发布范围整理 changelog，重新构建并验收准确正式签名候选，校验干净 tag 源码和发布门槛；推送稳定 tag 后跟踪工作流，下载公开双包和 manifest，核对签名、目标、服务版本、数据与 Release 资产。若候选暴露缺陷，先修复并重新冻结，不复用失效证据。

完成只在稳定版的实际公开资产和 updater 渠道通过后标记。D2 公开 AppImage 渠道验收与正式版本发布是本执行单的关键路径。

## 已完成的 RC 准备（2026-09-27）

- 版本已同步至 `1.9.0-rc.1`。发布脚本与工作流将 `beta.N` 保持 DEB-only，将 `rc` 和稳定版设置为 AppImage/DEB 双包与三项 updater 目标；发布脚本测试覆盖两种契约和缺失签名拒绝。双包发布增加正式公钥签名预检和上传前的成品验签；固定 minisign 0.11 源码 SHA256 与既有签名候选工作流相同。新增独立 GNOME 46/50 ESM 扩展附件，公开支持范围暂不扩大。
- 从 GitHub 真实下载的 beta.21 DEB 及公开 `latest.json` 提取签名，用新增验签器对产品公钥验证通过；同一 DEB 末字节篡改后被拒绝。本地 `release:check` 对 RC 版本与双包契约通过；本地未签名 `1.9.0-rc.1` AppImage SHA256 `5c9dfc9922be58c9892af158303f6909c2e990338b9c476ff0132424de9fe6cf`、DEB SHA256 `db350938bd453481ad4c0745120e608891a26dbaa6fe4007760960ccfdaf6973` 构建通过，准确 DEB 载荷检查通过。它们只用于隔离预检，不作为正式资产。
- [生产签名 AppImage 候选工作流](https://github.com/Asanilo/patina-Linux/actions/runs/36316756022) 从 beta.21 源码 `b1aa38eb` 成功构建并验签，下载后 SHA256 `500a08698891dec661c271700b290f1fe7ddc715a4c373a5239cc9efad6d4ed6` 与私有 candidate.json 一致。此包用于旧稳定版迁移检查，RC 仍需对准确源码重新签名。
- 隔离 Ubuntu 22.04/GNOME 42.9 KVM 从公开 `v1.8.4` AppImage SHA256 `3b65a1f1205acbf2027201058ec65c87a20988b859f26f01976d656289c5c557` 启动并记录真实窗口，随后在保留旧包的前提下换用上述已签名 beta.21 候选。首次接管、旧 session ID 1 保留、旧数据库一致备份不变、Desktop/daemon 版本与持久运行时一致、Dashboard 显示旧活动、退出界面后的新记录及 GDM Wayland 冷登录后新实例和再次记录通过。升级替换由隔离夹具在签名已验后执行，**不是旧客户端从公开 RC manifest 下载并执行的更新**。私有 JSON/截图及准确包在 `$HOME/.local/state/patina/acceptance/20260927-appimage-stable-rc1/`；VM 快照 `old-stable-to-signed-beta21-pass` 保留，活动磁盘已恢复 `pre-rc-public-channel`。
- 准确 RC 源码 `d4604ed2` 的[首次签名候选工作流](https://github.com/Asanilo/patina-Linux/actions/runs/36319662194) 在 `release:check` 的浏览器烟测阶段失败：headless Chrome 进程仍在，但 15 秒内未出现 DevTools 端口文件，尚未进入 AppImage 编译。已调整 CI 启动等待、共享内存选项和失败诊断/清理；本地以 `CI=1` 重跑 38 个真实浏览器烟测通过。另补充只在明确隔离夹具中执行的公开 RC updater 专项，用真实 tagged manifest、产品公钥下载验签、通用回退项和原子替换检查；公开 RC 存在前保持忽略。修复后须重新推送并对准确新提交重跑签名候选。
- 修复提交 `62b1d701` 的[签名候选工作流](https://github.com/Asanilo/patina-Linux/actions/runs/36320170987) 已通过完整门禁、生产密钥 AppImage 构建和独立验签；准确 artifact 在下载核对中。此前首次工作流失败只属于浏览器夹具，本地与远端修复后均已通过。
- 正式公开 beta.20→beta.21 DEB 在隔离 Ubuntu 22.04/GNOME 42.9 客体完成真实安装升级。下载包 SHA256 分别为 `b76f641c1ab01876169a3d3be385389c37572a2db537704719e33abd8214afca` 和 `57fa0cc14406ad4b341a29b8f46da63e622cbe4b0136bce857187007ffb282b5`。旧版先记录原生窗口，升级后 Settings 明确显示 Desktop beta.21 / Daemon beta.20；用户在实际页面确认“重新加载后台”后，新 daemon 实例为 beta.21，旧 session ID 1 与一致备份保持不变，SQLite 完整。退出 Desktop 后及 GDM Wayland 冷登录后均继续记录真实窗口。私有 JSON/截图在同一证据目录的 `public-deb-beta21/`，VM 快照 `public-beta20-to-beta21-deb-pass` 保留，活动磁盘恢复原状态。首次夹具缺少中文字体时误点红色“回退内置追踪”，按设计停掉 daemon；新 Desktop 以嵌入模式重开且旧会话完整。该误操作另存快照 `public-deb-rollback-fixture`，补入 Noto CJK 字体后按正确 UI 路径重跑通过，不计作产品缺陷。
- [v1.9.0-rc.1 双包预发布](https://github.com/Asanilo/patina-Linux/releases/tag/v1.9.0-rc.1) 的[工作流](https://github.com/Asanilo/patina-Linux/actions/runs/36323227229) 成功。公开 AppImage SHA256 `090d6c7409da72283bf40af0835e39cdfc042fc34dc56f25b4a5372bb3cf143b`、DEB `31308f42d875144245cb620a8542eaa9b94095215cd97c9b4996c704e2470ee0`，公开 `latest.json` 有 AppImage 专用、DEB 专用和旧客户端通用 AppImage 回退项；两包从公开地址实际下载并用产品公钥验签通过。隔离私有目录内以旧正式版 `1.8.4` 为当前版本调用 Tauri updater，真实 tagged manifest 选中 AppImage，完成生产签名下载、原子替换与旧包恢复文件保留，结果见 `public-rc1-updater-result.json`。这仍不代替客体生命周期与稳定 Latest 通道验收。
- **RC1 扩展包装缺陷：** 公开 ESM v5 ZIP SHA256 `a4ddcb0dd6e8cb2101beaa03d44e4e2417ff27f9aba3c5ac7c551f3976592820` 把 UUID 目录作为 ZIP 根，Fedora 客体执行文档中的 `gnome-extensions install --force` 时报告根目录没有 `metadata.json`。预装扩展使旧 ACTIVE 状态不能充当该资产的安装证据。改为根目录直接放 `metadata.json`、`extension.js` 的 ZIP 后，在移走预装目录的 Fedora 客体中安装成功，实际文件 SHA 与仓库源码一致；GNOME 42 旧 ZIP 采用同样的平铺修复。按不可覆盖已发布 tag 的规则，修复进入 `1.9.0-rc.2`，RC1 不作为稳定版通过依据。
- RC1 AppImage 本体与修正后的平铺 ESM 夹具在 Fedora 44/GNOME 50.5 KVM 中完成实际 FUSE 首启接管、Dashboard 显示、真实 Wayland 窗口、退出 UI 后继续记录、GDM Wayland 冷登录新实例和再次记录；修正 ESM 从移走预装目录后的 ZIP 安装，在新 boot 为 ACTIVE，双 D-Bus 名称由 GNOME Shell 持有。私有截图/JSON 在 `public-rc1-fedora/`，VM 快照 `public-rc1-fedora-flat-esm-pass` 保留，活动磁盘恢复原状态。此项证明公开 AppImage 本体与修正 ESM 字节可用，不把有缺陷的 RC1 公开扩展 ZIP 算作通过。[RC1 Release](https://github.com/Asanilo/patina-Linux/releases/tag/v1.9.0-rc.1) 已补充缺陷提示，不改写 tag 或附件。
- GNOME 42 旧入口也以修正平铺 ZIP SHA256 `b9ec15f29acae5a11fe71246224a5453bf8239fb4e185a5aa106ddb7ac8b9b30` 在隔离 Ubuntu 22.04/GNOME 42.9 中移走预装目录后实际安装，冷登录后双 D-Bus 名称存在，安装文件 SHA 与仓库一致；快照 `legacy-flat-zip-install-pass` 已保留。Fedora ESM 平铺夹具 SHA256 `dc30efe42ca0798e0da2a2037ea93038f23183b13a2cc1624d5255ba22d18031` 同样实际安装成功。这些是本地修正 ZIP，不冒充尚未生成的 rc.2 正式附件；rc.2 发布后须按公开字节再检查。

## RC2 公开渠道验收与正式版准备（2026-09-27）

- RC2 精确源码 `5cd5c4cc` 的[私有签名候选工作流](https://github.com/Asanilo/patina-Linux/actions/runs/36326344783)完成 `release:check`、生产密钥 AppImage 构建与独立验签。从同一提交导出的干净源码通过版本和 changelog 校验，工作区干净后推送 `v1.9.0-rc.2`。
- [RC2 公开双包预发布](https://github.com/Asanilo/patina-Linux/releases/tag/v1.9.0-rc.2)的[发布工作流](https://github.com/Asanilo/patina-Linux/actions/runs/36327705770)成功。公开 AppImage SHA256 `213d99fc856a11b85f516a485d114e1c565592c7306d85a8fd178bbd5f0e30e3`、DEB `93baa9b01194b9dff5f8a541a761d84a64c2c605775436b907195e989434a821`、`latest.json` `87bce2754b13e05ee0beacb734985b25c3a153111af6292dab3d54c9f44a1758`。从公开 Release 下载两包并对产品公钥独立验签通过；manifest 包含 AppImage 专用、DEB 专用和旧客户端通用 AppImage 目标，URL 都指向 RC2 tag。
- 公开 ESM v5 ZIP SHA256 `ea74990e7253a4faeed590153ecaee0eefa2733992f7cb3163d26ff904aa1024` 与 GNOME 42 v4 ZIP `7c2229bdd8a9c7e0287f08fff77cb2e8a0ba26b2036cc657fc15179668755a37`，两者根目录均恰有 `metadata.json`、`extension.js`。移走预装扩展目录后，Fedora 44/GNOME 50.5 与 Ubuntu 22.04/GNOME 42.9 的 `gnome-extensions install --force` 均成功，客体安装文件 SHA 与各自公开 ZIP 内容一致；冷登录后 Shell 持有新旧双 D-Bus 名称。
- 以公开 `1.8.4` AppImage 为旧包，在 0700 私有隔离夹具调用 Tauri updater，真实 GitHub RC2 tagged manifest 选中旧客户端通用 AppImage 目标，下载公开 RC2 AppImage、验证生产签名并原子替换；旧包 SHA256 `3b65a1f1205acbf2027201058ec65c87a20988b859f26f01976d656289c5c557` 的恢复文件保留，结果在 `/tmp/patina-public-updater-rc2.H9SD3z/public-update-result.json`。首次命令误加 `--exact` 导致 0 项测试；去掉后沙箱 DNS 不可用，再以获准网络重跑 1 项通过。此项验证公开 updater 与私有目标，不冒充真实宿主更新。
- GNOME 42 隔离客体从已验收的公开 beta.21 DEB 状态升级到公开 RC2 DEB；包版本 `1.9.0-rc.2`，GDM Wayland 冷登录后 daemon 为 `1.9.0-rc.2`，旧会话 ID 1 保留且 SQLite `integrity_check=ok`。关闭 Desktop 后，Shell 双 D-Bus 名称存在，真实 GTK 前台窗口由 daemon 记录。首次窗口脚本失败因 GNOME Overview 未点选窗口，点选后同脚本通过。
- Fedora 44 隔离客体从干净快照运行公开 RC2 AppImage，首次调用创建持久 user service 后前台退出；第二次打开显示真实 Dashboard 并完成 daemon 接管。退出 Desktop 后以及 GDM Wayland 冷登录后，版本化 AppImage 运行时的 daemon 保持 active，真实 GNOME 前台窗口记录通过，Shell 双 D-Bus 名称存在。冷登录窗口脚本同样需先在 Overview 点选窗口。此结果是隔离技术验收，不扩大 Fedora 公开支持承诺；首次前台退出仍按该客体首启行为记录，不隐去。
- 已把版本同步至 `1.9.0` 并整理稳定版 changelog 和支持文档。下一门槛是正式版源码的 `test:release`、`release:check`、准确签名候选、干净源码导出校验，再推稳定 tag；公开稳定资产与 Latest 必须重新下载验签、检查双包及更新目标后才能标记完成。
