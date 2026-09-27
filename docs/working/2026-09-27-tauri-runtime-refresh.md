# Linux Tauri 运行时依赖升级执行单

状态：实现与隔离验收通过，`v1.9.0-beta.21` 已作为 DEB-only 预发布公开发行。Tauri 升级源码提交为 `d56669c8`，tag 指向文档收口提交 `a1dc5ee8`；本地未签名候选与正式发布包分别标识。先前 CLI 2.12.0 DEB 的隔离升级证据保留在 `$HOME/.local/state/patina/acceptance/20260927-deb-refreeze-cli212/`。

## 范围与归属

- Rust `tauri` 从锁定的 2.10.3 升至 Windows 上游已使用的 2.11.5；核对 `tauri-build`、`tauri-runtime-wry` 和相关 Tauri crate 的锁文件解析。
- JS `@tauri-apps/api` 及 SQL、opener 插件与 Rust 端按兼容版本对齐；现有 CLI 2.12.0 保留，因为它承担已验证的 Fedora AppImage 打包修复。
- 变更属于构建依赖和 Tauri 平台边界，不借机移动 daemon、命令或页面 owner，不修改用户数据 schema、发布渠道或宿主安装。

## 验收顺序

1. 记录升级前后依赖锁定版本与锁文件差异；排除无关的大范围包更新。
2. 运行 `npm run check:full` 与 `npm run release:check`，确认 Rust/前端测试、Clippy、扩展和发布元数据门禁。
3. 对新准确 DEB 候选运行包载荷检查和私有 beta.20→候选安装/升级/卸载/重装；若二进制变动，旧候选的真实客户端证据不能直接沿用。
4. 对新准确 AppImage 候选复验 Fedora 44/GNOME 50 与 Ubuntu 24.04/GNOME 46 的 WebView、Desktop/daemon 版本、真实窗口、登录与数据路径。新的同版本包不能直接覆盖客体已有 runtime，使用已保留的干净快照或明确的测试版本。
5. 固定候选摘要、源码提交和未覆盖项，再决定是否把这项升级纳入下一版本。推送、tag、Release、宿主安装和公开 AppImage 分发分别遵守现有授权边界。

## 当前约束

R1 连续运行观察按用户要求暂停。beta.21 DEB-only 准备基点保持可追溯；本升级不自动扩大 Fedora 支持承诺，也不自动启动 D2 的公开 AppImage 渠道验收。

## 执行结果（2026-09-27）

- 锁定 Rust `tauri` 2.11.5、`tauri-build` 2.6.3、SQL 插件 2.4.1；锁文件解析出 `tauri-runtime` 2.11.3、`tauri-runtime-wry` 2.11.4。JS API 2.11.1、opener 2.5.5、SQL 2.4.1；CLI 保持 2.12.0。Cargo 锁文件的其他变化来自这些版本的传递依赖解析。生成的 Tauri schema 随构建同步更新。
- `npm run release:check` 与单独的 `npm run check:full` 均通过，含 718 个 Rust 测试（20 个忽略）、56 个 TypeScript 测试文件、前端构建、Clippy、扩展和版本/发布元数据检查。首次在受限沙箱执行 `check:full` 时，测试内部启动 Node 子进程收到 `EPERM`；在允许该子进程运行的环境中重跑通过。未更改产品版本、数据 schema 或发布渠道。
- 准确新核心 DEB SHA256 `98eafa7d242dd4f83cd28752de9a2e88a32c5debe30bb79641f237fd5adae08c`，内含 `patinad` SHA256 `6ebbf2110b30005cead538546e7c83bd4b71b72e225114dd8b1e5e1caa8e5d23`。包载荷验证和私有 beta.20→候选安装/升级/卸载/重装通过。该二进制在 Ubuntu 24.04/GNOME 46 客体中通过真实窗口标题隐私、C1 导入汇总排除/恢复及原 user unit 恢复回归。
- 准确新核心 AppImage SHA256 `a03ac05a9009d2ed0fe35fa291089d84837b73de5066dc35a61886d3fa86f0bb`。Fedora 44/GNOME 50 客体通过首次运行接管、Dashboard/History/Settings 实际显示、设置写回与数据库检查、原生 Wayland 窗口记录、锁屏停止/解锁恢复、退出 Desktop 后继续记录及冷登录后记录。Ubuntu 24.04/GNOME 46 客体通过首次运行接管、冷登录自启动、Dashboard 实际显示、原生 Wayland 窗口记录和数据库检查。两客体的登录会话均为 Wayland；Fedora 扩展为 ACTIVE，Ubuntu 两个扩展 D-Bus 名称由 GNOME Shell 持有。
- 首次 AppImage 构建在沙箱中因 `appimagetool` 下载官方 runtime 失败，允许官方下载后完整重建通过。两客体最初采样时 GNOME 概览遮挡了测试窗口，退出概览重跑通过；Ubuntu 干净夹具只有 GTK4，换用 GTK4 测试窗口后通过。这些是构建/夹具条件，不计作产品通过的替代证据。
- 准确包、隔离安装记录、截图和逐项 JSON 存于 `$HOME/.local/state/patina/acceptance/20260927-tauri2115-candidate/`；Fedora VM 快照为 `post-tauri2115-fedora-pass`，Ubuntu VM 快照为 `post-tauri2115-ubuntu46-pass`，验收后已恢复原 `pre-tauri212-regression` 活动基线。仍未验收宿主安装、真实硬件挂起、公开 updater 渠道或正式 AppImage 分发。

## 公开发布结果（2026-09-27）

- [发布工作流运行 36313336376](https://github.com/Asanilo/patina-Linux/actions/runs/36313336376) 成功：干净 tag 源码版本/changelog、完整质量门禁、正式签名 DEB 构建、包内 daemon 校验、更新资产与扩展包准备均通过。[Patina v1.9.0-beta.21](https://github.com/Asanilo/patina-Linux/releases/tag/v1.9.0-beta.21) 为非草稿预发布。
- 从公开 Release 实际下载的 `Patina_1.9.0-beta.21_amd64.deb` SHA256 为 `57fa0cc14406ad4b341a29b8f46da63e622cbe4b0136bce857187007ffb282b5`，再次通过 `release:verify-daemon-deb`；包内正式 `patinad` SHA256 为 `578e28d5b06a1f42abf81e07312e687fd9d0c3d7af72761f9754141b1f6081b3`。`latest.json` SHA256 为 `2213d69f035b1946fc7a7363a8d9ee312215a8ea72b431ff36a24ed068adde92`，版本为 `1.9.0-beta.21`，仅含 `linux-x86_64-deb` 目标，指向上述公开 DEB 且带非空签名。Release 另有 GNOME v4、Chromium v0.1.0、Firefox v0.1.1 扩展附件，没有 AppImage。
- 正式包与本地未签名候选的字节不同，不能沿用本地 SHA 或声称对正式签名包完成了新的宿主安装/真实硬件验收。R1 仍暂停，D2 公开 AppImage 分发仍未启动。
