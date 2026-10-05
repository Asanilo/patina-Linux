# 多客户端基础分支交接

更新：2026-10-05；沿用文件名保持链接稳定。**本小阶段在 M6k 停用与恢复收尾，用户已明确要求暂停，等其说继续。** 已验收代码检查点为 `3b8a2479483dac0dc6f86c6e2546a58d387c5277`；本交接由随后的一次文档提交更新。

本文是当前接续入口，取代原 M6c 交接内容。详细证据见[阶段计划末尾](./2026-10-03-multi-client-platform.md)，长期边界见 [architecture](../architecture.md)。整体后端交付和四端计划尚未完成，历史记录中的“继续下一项”不取消本次暂停。

## 分支、授权与停止点

- 仓库：`Asanilo/patina-Linux`；分支 `feature/multi-client-platform`；worktree：`/home/arinp22/code/patina/.worktrees/multi-client`。
- 稳定 main：`/home/arinp22/code/patina`，保持 `f344c8a93b21d6ca8dac89adb56be96e7ce929c7`。本阶段没有替换生产 1.9.2。
- 当前本地跟踪引用 `origin/feature/multi-client-platform` 为 `bde592f354dc795df2b425857cba8bcecf9335ca`。验收代码领先该引用 10 个提交，收尾文档提交另计；本轮未联网核对远端，不把本地引用当作实时远端状态。
- 本轮仅收尾、保存草稿并本地提交；没有推送、合并 main、安装、PR、tag 或公开发布授权。早期交接中的一次性推送授权已执行，不能用于本次新增提交。
- 已确定产品方向：**后端独立安装和升级；Desktop 按协议兼容连接**。恢复后可继续原已授权的后端与架构基础实现；进入新 Web／TUI／GPUI 界面及产品交互前讨论。默认单 agent。
- 开发代码恢复为已验收检查点；未完成的 M6l 文件删除实现保存为草稿，不随本次提交交付。不要因其存在于本机就视为功能已实现。

## 已实现及未完成的边界

| 能力 | 当前状态 |
| --- | --- |
| 协议与客户端基础 | `crates/patina-protocol`、`crates/patina-client` 已提取；SDK 不依赖 Tauri、GTK、SQLx 或 tracking；现有 Desktop 复用 SDK |
| 同步与核心业务 | HTTP/SSE、快照协调、事件缺口／实例更换恢复、条件写入，以及应用聚合、Dashboard／History、设置资源和 Tools 等已有回归；网页 Desktop 迁移和本地偏好边界仍有缺口 |
| 无桌面后端与独立版本 | 同一业务 crate 的 headless 投影可独立构建；`packaging/daemon/VERSION` 提供后端版本，实际 Desktop 1.9.2 与后端 2.0.0-test.1 已通过隔离重载及双 SDK 验收 |
| 安装与激活 | 候选完整性校验、stage／inspect／条件 select、服务计划、显式 activate 和中断恢复已实现；版本选择、实际运行身份与安装审计分别核对 |
| 旧交付迁移与重载 | 已知 DEB／AppImage 布局显式迁移，保留自定义 unit／mask；standalone 重载验证目标摘要、实例和协议，不要求产品版本等于 Desktop |
| 分包基础 | 独立安装器 DEB 已有隔离 dpkg 共存、升级、两种移除顺序及重装证据；包管理器移除安装器不会移除用户 runtime 或数据。公开 DEB／AppImage 仍是旧整包 |
| 停用与恢复 | `--deactivate-runtime` 关闭启动准入并停止／屏蔽本安装服务，保留数据、载荷、登录意图和版本下限；`--activate-runtime` 显式恢复；错误归属与外部变更拒绝接管 |
| 尚未交付 | 用户 runtime 载荷卸载、真实 user systemd／登录验收、无 Desktop 的 GNOME 集成交付、纯客户端 Desktop 包、独立后端正式签名更新链路 |
| 新客户端 | 只有 SDK 验收探针，无交互式 TUI、GPUI 或可用 Web 客户端；浏览器 session／CSRF／静态入口仍待实现。后端兼容更新不等于四套 UI 自动生成 |

停用失败保留可恢复意图；恢复重载失败重新屏蔽，普通客户端不能隐式重新启用。启动准入在 daemon 存储初始化之前检查，也阻止 Desktop 回落 embedded。载荷仍保留；当前没有 `--uninstall-runtime` 命令。

当前主要源码入口：

- `src-tauri/src/platform/linux/standalone_runtime{.rs,/selection.rs,/probe.rs}`：载荷、选择、元数据核验；不是业务生命周期 owner。
- `src-tauri/src/app/standalone_activation{.rs,/native.rs,/journal.rs,/deactivation.rs}`：安装状态、迁移、停用与恢复编排。
- `src-tauri/src/platform/linux/patinad_service_unit/mask.rs`：有归属的 mask 发布与恢复。
- `src-tauri/src/app/daemon/installation/`：薄 CLI；`app/daemon_service/upgrade.rs`：Desktop 重载目标验证。
- `scripts/package-daemon{,-deb}.py`、`packaging/daemon/`：候选、安装器及说明；实际使用命令见[开发文档](../linux-development-setup.md)。

## 本小阶段新增提交

| 提交 | 内容 |
| --- | --- |
| `bdd3382c` | 共享服务客户端暴露实际运行映像身份 |
| `02c6f843` | 安装版本检查与条件选择 |
| `ea50a848` | 已选 runtime 的服务绑定预览 |
| `283b3074` | 激活与可恢复 owner 交接 |
| `30262413` | 已知旧服务迁移 |
| `337dfb02` | Desktop 按独立后端目标验证重载 |
| `d35dab60` | 后端独立版本来源 |
| `9e310d32` | 独立安装器 DEB 与生命周期检查 |
| `955c3bda` | 存储访问前的持久启动准入 |
| `3b8a2479` | 显式停用、恢复及中断重试 |

## 验证与证据

最终源码的门禁组成项均通过：68 个 TypeScript 测试文件、49 项浏览器检查、48 项 SDK 测试；Desktop **848 passed / 23 ignored**，headless **705 passed / 11 ignored**，两种 Rust 投影的 Clippy 与依赖／架构边界通过；安装器打包回归 11 项通过。忽略项未算作通过。首次完整运行有 Clippy 失败，修复后补齐最终 Rust 门禁，不将第一次完整日志误标为全部成功。

本机证据不随 Git 推送；`/tmp` 目录可能被系统清理：

| 证据 | 范围 |
| --- | --- |
| `tmp/acceptance/m6k-deactivation-full.log` | 未再变化的前端、浏览器、SDK、生成契约和 bundle 检查；包含已修复的首轮 Clippy 失败 |
| `tmp/acceptance/m6k-recovery-rust-final.log` | 最终两种 Rust 投影及 Clippy |
| `tmp/acceptance/m6k-packaging.log` | 11 项打包回归 |
| `/tmp/patina-activate-private-c9b56z_f/result.json` | 新安装、登录关闭、启动故障后的停用／恢复 |
| `/tmp/patina-activate-private-h5qtg6l5/result.json` | 旧 DEB 迁移、登录开启、停用／恢复 |
| `/tmp/patina-activate-private-162o54rn/result.json` | 合成 AppDir 迁移、启动故障、停用／恢复 |
| `/tmp/patina-mask-offline-hv9cpwo9/result.json` | 真实 systemctl 离线识别 mask／恢复 unit |
| `/tmp/patina-backend-deb-acceptance-vkzbdel2/` | M6j 私有 dpkg 根中的 10 阶段共存／升级／移除／重装 |
| `/tmp/patina-activate-private-lr8sdvwr/` | M6j 包内真实 A／B 后端载荷与 Desktop 跨版本重载 |

最终 M6k 候选为 `tmp/acceptance/m6k-deactivation/candidate-3/`，版本仍为 **1.9.2 debug**；该轮没有重新生成 DEB。

- 二进制 SHA256：`079661825a4a0d33dff6cf88e68d08fbf8fa6f096c63d425a9b90528f5838a07`。
- manifest SHA256：`5430451fa56c8f9d5bf364c2fea8c8a3469ee18243a3475da3da75852a205b57`。
- TAR SHA256：`e9d1e098ea3f6c3629c3a2456f7255cdcd3ab476dc26807064bcae07c2488651`。

三条停用／恢复路径使用真实 daemon、私有 D-Bus 和模拟服务管理器，覆盖错误 PID／环境／drop-in、外部 mask、重载失败、重复操作、停止前就绪失败、磁盘 unit 与 manager 缓存不一致，以及历史／登录偏好保留。**不是实际用户 systemd、登录、GNOME 采集、完整 React 交互或实际 AppImage/FUSE 验收。** dpkg 验收的依赖检查使用宿主包元数据副本，没有安装依赖载荷。旧正式 DEB 的验签不代表独立后端已完成签名发布。

恢复开发时按变更范围运行 `npm run check:full`、`npm run test:backend-packaging` 及相关 `scripts/acceptance/` 脚本。已通过且源码未变的检查不必反复重跑。本次收尾只有文档变更，校验路径、提交、证据摘要、UTF-8、diff 和分支状态。

本机普通构建使用 `CARGO_TARGET_DIR=/home/arinp22/code/patina/src-tauri/target`，缓存依赖可设 `CARGO_NET_OFFLINE=true`。不同构建投影会覆盖 `debug/patinad`，验收前保存并记录准确产物摘要。**临时源码副本／改版本的验收须用独立 target**；已有版本夹具使用 `target/daemon-version-acceptance`，不得与普通 target 混用。

## 未完成草稿

M6l 文件层草稿未编译、未测试、未接入 CLI／安装宿主，已从开发代码撤回。两份备份：

- 本 worktree 内 `tmp/handoff/patina-m6l-draft-20261005-00iz9dz0/`（Git 忽略，可跨重启保留）。
- `/tmp/patina-m6l-draft-20261005-00iz9dz0/`（临时副本）。

备份包括原文件、tracked patch、基线提交和逐文件 SHA256。恢复前核对当前源码与设计；不要直接覆盖后来改动。M6l 设计保留在阶段计划末尾，代码可参考或重写。

## 用户恢复后的顺序

1. **完成载荷卸载／重装。** 先核对固定清单，再在已证明停用和排他 lease 下按有归属文件逐项删除；持久计划支持中断恢复。保留个人数据、mask、cutover 和版本下限，未知内容不递归删除。重装及重复请求不得误删新载荷。
2. **补真实平台与交付证据。** 真实 user systemd 的初装、迁移、升级、失败恢复、停用、卸载和重新登录；安排无 Desktop 的 GNOME 集成，明确扩展及纯客户端包的文件归属。生产安装仍需相应授权。
3. **完成剩余业务契约。** 先审计已存在的网页 API 与 Desktop 读取、客户端偏好、维护和备份边界，避免重复实现已有后端；对下表涉及的产品变化先讨论。
4. **形成独立交付候选。** 固定协议兼容矩阵、签名／更新入口和恢复说明。正式包、实装和公开发布分别记录证据与授权；旧整包发布结论不能代替独立后端验收。
5. **再讨论首个新客户端完整场景。** 原规划优先 Web 复用 React，再 TUI／GPUI；在用户讨论前不开始新界面开发。

## 尚未选择的产品问题

| 问题 | 先前建议，尚未批准 |
| --- | --- |
| 停止浏览器记录后是否仍展示过去历史 | 保留过去历史可见 |
| URL 隐私是否跨客户端统一 | 统一策略，不默认让 Desktop 绕过裁剪 |
| 恢复数据时是否恢复客户端外观偏好 | 保留当前客户端偏好 |

这些选择不阻塞先完成独立后端生命周期。混合设置保存不是跨系统资源的全局事务；Tools 单操作事务不保证多请求原子性或 OS 通知恰好一次；剩余直接数据库路径仍需逐项记录退出或保留理由。

R1 长时间观察、Widget、KDE／wlroots、Flatpak、上游草稿、大规模 Windows 清理和 Cargo workspace 重排继续保持原暂停或独立范围。用户明确恢复前不开展下一批工作。
