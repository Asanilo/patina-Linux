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
