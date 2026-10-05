# 多客户端基础分支交接

日期：2026-10-04。代码检查点：`ce8baab4`；本交接作为后续文档提交加入同一分支。本文是接续入口，详细执行证据以[阶段计划的最新记录](./2026-10-03-multi-client-platform.md)为准，长期边界以 [architecture](../architecture.md) 为准。后续检查点取代本文时将其归档。

接续更新（2026-10-05）：此处的 M6c 停止点已推进到 M6i 独立版本来源，已有 Desktop 1.9.2 适配器与真实 2.0.0-test.1 后端的私有总线重载及双 SDK 验收；仓库版本仍为 1.9.2，AppImage 源迁移使用合成 AppDir。本交接保留原检查点，当前代码和验证结果请读取阶段计划末尾；正式分包、旧包卸载归属、真实 systemd 登录及剩余业务契约仍待完成。跨版本源码验收须使用专用 Cargo 缓存，不能再与普通开发 target 共用输出槽。

## 分支与授权

- 仓库：`Asanilo/patina-Linux`，远端 `origin`；开发分支 `feature/multi-client-platform`。
- 本机 worktree：`/home/arinp22/code/patina/.worktrees/multi-client`。稳定 main 在 `/home/arinp22/code/patina`，基线为 `f344c8a93b21d6ca8dac89adb56be96e7ce929c7`。
- 代码检查点领先 main 30 个本地提交；本交接提交另计。用户本轮明确授权将本开发分支推送远端。推送结果以 Git 远端核对为准，不能用本文代替网络操作成功证据。
- 用户授权继续建设后端和整体架构基础，必要时重构；进入新 Web／TUI／GPUI 界面及多客户端产品交互前讨论。例行实现和验证可继续，无须重复询问已确定方向。
- 用户已选择：**后端独立安装和升级，Desktop 按协议兼容连接**。这不是本机安装、合并 main、创建 PR、tag 或公开发布授权；过去稳定版的发布授权不能自动套用于本分支。
- 默认单 agent；当前无新增子代理授权。主线和生产 1.9.2 未被本阶段替换；历史上游草稿 worktree 不在本任务范围。

## 目标和实际交付

目标是一套业务后端，Tauri、TUI、GPUI、Web 复用事实、规则和同步契约；界面与宿主能力按客户端实现。后端兼容更新不等于自动生成四套界面。

| 能力 | 代码检查点状态 |
| --- | --- |
| 独立协议和 SDK | `crates/patina-protocol` 与 `crates/patina-client` 已提取；SDK 不依赖 Tauri、GTK、SQLx 或 tracking；Desktop 按用例接入同一 SDK |
| 同步基础 | 能力协商、认证 HTTP/SSE、订阅与快照协调、事件缺口恢复、daemon 实例更换和陈旧响应屏蔽已有回归；写入失败不自动重试 |
| 核心读取 | Dashboard、应用聚合、精确 History 与小时统计等核心链路返回后端产品结果；分类、排除、导入优先级不由这些客户端调用方重复实现 |
| 设置与资源 | 分类、普通追踪策略、浏览器／音频资源具备快照与条件写入；过期编辑产生冲突；设置页保留原始编辑基线 |
| Tools | 类型化 SDK 操作与 Desktop 共用契约；单项写入事务、owner 串行、快照与事件次序及前端异步响应协调已完成 |
| 读写隔离 | daemon 分析读取使用独立只读 WAL 池和准入／查询期限，避免长分析占住 writer；这不意味着任意 Rust 计算可被强制抢占 |
| 后端构建 | 同一产品 crate 的无默认 feature 投影可构建不链接 Tauri／GTK／WebKit 的 patinad，保留同一业务实现 |
| 独立交付基础 | 静态构建身份、确定性候选归档、完整性校验、版本化存放已实现；激活、旧包迁移、升级目标验证尚未完成 |
| 新客户端 | 只有独立 SDK 验收探针，尚无交互式 TUI、GPUI 或可用 Web 客户端；浏览器会话／CSRF／静态入口仍待实现 |

关键源码入口：

- `crates/patina-protocol/`、`crates/patina-client/`：契约、传输和客户端能力；类型生成不能把开发依赖带入运行时。
- `src-tauri/src/engine/tracking/runtime_settings.rs`：普通策略更新及追踪状态协调。
- `src-tauri/src/data/repositories/tools/`、`src-tauri/src/engine/api/tools_contract.rs`：Tools 事务与协议转换。
- `src/features/tools/services/toolsRuntimeSnapshotStore.ts`：前端 Tools 快照／动作协调。
- `src-tauri/src/app/daemon/installation.rs`：显式 staging 参数解析；业务与文件校验不在 CLI 中堆积。
- `src-tauri/src/platform/linux/standalone_runtime.rs`：私有安装根、独占锁、候选校验和原子版本目录发布。
- `scripts/package-daemon.py`：从实际二进制构建候选归档，输出归档及 manifest 摘要。
- `src-tauri/src/app/daemon_service/{appimage,upgrade}.rs`：下一批必须处理的既有服务归属与重载检查。

## 最近提交与当前停止点

| 提交 | 内容 |
| --- | --- |
| `a22cb049` | History 精确小时统计由后端统一，覆盖时区与夏令时边界 |
| `fda67ad1` | History 最短会话设置使用条件写入，迟到确认不能覆盖新状态 |
| `7b83fac4` | Tools 共享契约、类型生成与独立 SDK 操作 |
| `46974347` | Tools 写侧事务与串行 owner，修复并发编号、重复完成和部分回滚 |
| `adf9d69a` | Tools 前端读取／写确认协调，防止旧响应回退已收到的新状态 |
| `6ebc7673` | `--build-info` 与独立后端候选归档 |
| `ce8baab4` | `--stage-runtime` 及版本化存放 |

M6c 已完成并提交，未开始下一批代码。staging 接收显式目录、绝对 runtime 根及预期 manifest SHA256；验证固定文件集合、大小、模式、哈希、目标和构建投影后发布到 `versions/<manifest SHA256>`。重复操作重新验证已有版本，损坏时拒绝而非静默修复。它不运行候选、不选择 current、不操作 systemd、不读写 profile。摘要只能证明与预期内容一致，不能代替发布者签名。

## 验证证据及复现

`ce8baab4` 的完整 `npm run check:full` 已通过：68 个 TypeScript 测试文件、49 项浏览器检查、44 项 SDK 测试；Desktop 796 passed / 22 ignored，无桌面后端 637 passed / 11 ignored。类型生成比较、依赖／架构边界、Clippy 和 bundle 预算通过。忽略项未算作通过；细节见阶段记录。

本机证据（忽略文件，不随 Git 推送；临时目录可能被系统清理）：

- `tmp/acceptance/m6c-full.log`：完整门禁。
- `tmp/acceptance/multi-client-m6c-staging/`：存放回执、候选路径、metadata 验收；控制程序 SHA256 `e0507c4fe1d24908f9e50df868d72ef5ca2f0ba370e84424484d96293d3358bd`。
- 存放载荷是已验证的 M6b debug 二进制，SHA256 `45b8b0c3408062f3637f9ffdd60df18ac1096bb2cb24a63fe0372f9f09762b88`；并不包含 M6c 新 CLI。两次存放返回同一身份，只有一个版本，无服务、current 或 profile 副作用。
- `tmp/acceptance/m6c-independent-client.log`、`/tmp/patina-independent-client-mcffgxrc/`：存放后载荷的双 SDK 分类／Tools 同步、条件写入、认证拒绝、关闭／重启及数据保留检查。
- `/tmp/patina-m6c-stage-qtxtiygj/`：私有版本存放根。

上述隔离运行不能代替生产安装、真实 GNOME 采样、签名发布或四客户端 UI 验收；debug 成品仍依赖 X11／XCB／Pulse 等库，不能承诺所有 Linux 发行版直接运行。

从分支根目录验证：

```bash
npm run check:full
cargo build --locked --manifest-path src-tauri/Cargo.toml --no-default-features --bin patinad
cargo build --locked --manifest-path crates/patina-client/Cargo.toml --example inspect
python3 scripts/acceptance/daemon-build-info.py /absolute/path/patinad
python3 scripts/acceptance/independent-client.py /absolute/path/patinad /absolute/path/inspect
```

本机曾使用 `CARGO_TARGET_DIR=/home/arinp22/code/patina/src-tauri/target` 共享缓存，并以 `CARGO_NET_OFFLINE=true` 使用已有依赖；新机器需要先准备依赖。构建不同投影会替换共享 `debug/patinad`，验收前复制并记录对应二进制摘要。独立探针位于选用 target 目录的 `debug/examples/inspect`。运行验收脚本会创建临时 Local profile、使用回环临时端口并隔离显示／D-Bus／音频，不应替换成生产 profile。安装存放命令见[开发文档](../linux-development-setup.md)。

本次交接只修改文档，做路径、提交引用、UTF-8 和 diff 一致性检查，不重复无变化的完整门禁。

## 接续顺序与完成标准

先完成独立后端安装／升级工作包，再收口剩余业务契约。开始前核对分支、工作区和当前源码，避免照旧记录重复实现。

1. **激活与身份设计。** 在现有 M6c 文件边界之上明确激活 owner、安装身份、目标路径和失败恢复状态。记录短执行设计后继续实现，不因“需要决定 owner”本身重复请求授权。不能仅删除版本相等判断。
2. **旧交付迁移。** 当前 DEB 同时拥有 `/usr/bin/patinad` 与 `/usr/lib/systemd/user/patinad.service`；AppImage 有自己的持久 runtime 和 user unit。处理与独立安装的归属冲突，保留自定义 unit／mask、RuntimeLease、交接 reservation 和用户关闭开机启动的意图，防止双 owner。
3. **运行目标验证。** 区分已安装版本和正在运行版本；重载后验证新实例和预期安装身份，再按协议／capability 判断 Desktop 兼容性。现有 `upgrade.rs` 仍要求后端版本等于 Desktop，在替代检查就绪前保留保护。
4. **隔离安装升级验收。** 无 Desktop 场景覆盖初装、重复操作、版本切换、失败恢复、数据保留和明确不兼容；数据库迁移后不得自动降级二进制。正式签名／发布流程接入与本机安装分别处理。
5. **剩余 Desktop 契约清单。** 网页查询、本机偏好存储、运行诊断及维护等仍有旧 facade／数据库／宿主边界；逐项核对并记录保留理由。备份恢复的路径权限不能直接暴露给 Web。

以上完成后再讨论新客户端的首个完整场景。阶段计划原顺序是先 Web 复用现有 React 页面，再 TUI／GPUI；这是规划，尚未授权跳过新交互讨论直接交付这些界面。

## 待讨论事项与已知限制

| 未决问题 | 当前建议（未获选择，不得当作已批准） | 影响 |
| --- | --- | --- |
| 停止浏览器记录后是否仍展示历史 | 保留过去历史可见 | 网页业务读取迁移 |
| URL 隐私是否各客户端统一 | 统一策略；不默认让 Desktop 绕过裁剪 | 网页 DTO 与权限设计 |
| 数据恢复时是否恢复客户端外观偏好 | 保留当前客户端偏好 | 偏好的物理存储迁移和备份兼容 |

这些选择不阻塞独立后端交付基础。其他明确限制：混合设置保存不是跨系统资源的全局事务；Tools 单操作事务不承诺多请求原子性或 OS 通知恰好一次；断连期间的计时显示／新鲜度语义尚未全部解决。剩余直接数据库路径不能据此扩展成第二份业务实现。

Widget、KDE／wlroots、Flatpak、大规模 Windows 清理和 Cargo workspace 重排继续保持独立范围；不要从历史文档恢复暂停事项。本轮只交接并推送该分支，不开始这些扩展工作。
