# Linux 稳定阶段收口与正式补丁发布

状态：已完成并归档。1.9.2 已公开发布，真实旧客户端升级和宿主正式安装验收完成，本阶段为后续新功能架构规划留下准确基线。产品和发布规则仍以长期文档为准；本文件保存本轮证据，不是新的待办清单。

## 基线与范围

- 接手主线 `main` 为 `e6590d52`，工作区干净，比本地 `origin/main` 记录领先 1 个提交。远端与目标版本占用情况另行实时核对。
- 本轮包含已审查的 Summary 停用分类与别名修复、发布资产保护，以及升级夹具证明范围修正；使用向后兼容的补丁版本 `1.9.2`，未覆盖已发布 tag 或附件。
- 用户已授权完成上述顺序工作和正式发布，包括所需 main 推送、正式 tag／Release，以及备份后的宿主升级验收。管理员认证若需用户操作，不在聊天接收密码。仅在明确的隔离客体执行旧版本及故障场景。
- R1 连续两周运行观察、Widget 暂停项、KDE/wlroots、Flatpak、新客户端实现、Windows 删除及 Cargo workspace 重排继续不在本轮范围；不提前实施尚未确定的新功能架构。

## 完成清单

- [x] 核对远端基线、已发布版本、目标 tag 与宿主／隔离环境。
- [x] 准备补丁版本与完整发布说明，通过 `test:release`、`release:check` 及干净提交的版本／changelog 校验。
- [x] 推送准确源码，完成正式双包发布；核验公开资产签名、摘要、manifest、Latest 与实际发布资产保护结果。
- [x] 在隔离环境实际运行公开旧客户端，完成其自身的公开更新路径、后台交接、数据保留与恢复材料检查；不得用模拟版本号的当前 updater 替代该证据。
- [x] 备份宿主既有数据与安装基线，再升级准确正式 DEB，检查 Desktop／daemon 版本、旧数据、单一 owner、无界面记录和登录偏好。
- [x] 完成阶段文档状态和链接整理，归档已结束执行单；留下已证实支持面、明确未覆盖项、暂停项和新功能规划入口。
- [x] 按清单复核实际完成证据，记录发布源码提交、公开版本与宿主版本；收口文档单独提交，不改变发布标签。

## 收口后的维护入口

- 以实际用户反馈和可复现故障为入口，先处理记录遗漏、统计不一致、升级与恢复问题；每项记录环境、版本、最小复现、真实 owner、回归证据和交付版本。没有具体问题时，不另开持续重构任务。
- 2026-10-02 只读查询确认仓库 GitHub Issues 未启用；这不等于没有缺陷。当前通过用户反馈收集问题，不在本轮启用或修改 Issues。
- 后续新功能先明确一个完整使用场景、行为验收和数据／接口归属，再评估现有 daemon 与客户端边界；暂不选定新 UI 框架，不将已记录的 Desktop 读取例外自动升级为全量迁移任务。

## 正式成品

- [1.9.2 正式 Release](https://github.com/Asanilo/patina-Linux/releases/tag/v1.9.2)已公开并成为 stable Latest，Release ID 为 `401525618`；工作流 `36960195478` 成功，标签准确指向 `ecb018f5af11494330303b923b7aecd3884fcac6`。
- 从公开下载入口重新取得全部 7 项附件，逐项匹配 GitHub SHA-256 与大小；tag 清单和 stable Latest 清单字节一致，三个 updater target 均指向 1.9.2，旧版通用 fallback 保持 AppImage。
- AppImage SHA-256：`536de30a88a931f618036946c363f5a93aa62280f09a469a1990e0d6b651790a`。
- DEB SHA-256：`d9e8abcd43aa0dc6ae3f847da2749a31061732339463ce3811eaaefcc6ba1914`。
- 两包使用产品配置公钥独立验签通过；正式 DEB 布局检查通过。新版本上传经过资产保护；对公开原字节再次运行只读 `guard-release-assets` 返回 `publish:false, files:[]`，没有覆盖或重写 Release。

## 真实旧客户端升级

- 在无 Patina DEB 的 GNOME 42.9 客体实际运行公开 1.8.4 AppImage，通过 About 检查到公开 stable Latest 1.9.2，经过其下载确认与“重启安装”按钮完成文件替换；新文件 SHA-256 与公开 1.9.2 完全一致。
- 旧版限制已实测：1.8.4 的“重启安装”仍启动旧 FUSE 挂载程序，API 继续报告 1.8.4。正常托盘退出并从原 AppImage 路径重开后，新版完成首次 daemon 接管。README 已明确该手动步骤；本轮不把它表述为全自动重启成功，也没有用当前 updater 模拟版本号替代旧进程。
- 迁移后的 capabilities 报告 daemon / 1.9.2，单一 patinad、NRestarts=0；一致备份中的已封口合成会话逐字段保持，SQLite 完整性与外键检查通过，旧 AppImage 和升级前数据库保留。
- 正常退出 Desktop 后真实 GNOME 窗口被后台记录；冷启动取得不同 boot ID 和 daemon InvocationID，版本、单 owner、历史数据与恢复材料检查再次通过。采样检查须先有实际客体输入退出 AFK；仅用自动化展示窗口不能冒充用户活跃状态。
- 冷登录 Desktop 由 `--autostart` 启动，唤起后 About 显示 `v1.9.2`，再次检查更新显示“已是最新版本”；正常退出后又完成真实窗口采样，无循环更新。完整证据保存在私有验收目录的 `legacy184-to192-complete.tar`。

## 宿主正式升级

- 用户完成系统认证后，准确公开 DEB 以 `dpkg` 从 `1.9.0-beta.21` 安装到 `1.9.2`。安装前已完成在线一致数据库与配置备份；切换后台前另正常停止服务，保存停机一致备份，再 reload unit 并启动新版，不删除旧数据或自动降级。
- `release:inspect-installed-patinad --phase managed --expected-version 1.9.2` 通过；运行中的 daemon、安装版本和实际 Desktop About 均为 1.9.2。Settings 本地存储读取和缓存维护控件正常，UI 操作未重启后台。
- 停机备份与升级后数据库的六类历史事实摘要一致，schema migration 元数据不变，SQLite 完整性与外键检查通过。恢复材料只保存在当前用户的私有验收目录，不进入仓库。
- Desktop 登录自启仍为 `0`，无 `Patina.desktop` 自启条目；后台登录启动仍为 `1`、systemd unit enabled，与升级前完全一致。此次验收未重启宿主，不将隔离客体冷启动表述为宿主系统重启实测。
- 正常托盘退出 Desktop 后，daemon PID 保持不变，15 秒观察内 heartbeat 和 successful-sample 时间戳继续推进；验收结束恢复 Desktop 未打开的初始状态，保留正常后台记录。

## 证据与执行记录

发布规则见 [版本规范](../versioning-and-release-policy.md)。既有 1.9.1 交付与隔离环境证据见 [正式版记录](../archive/2026-09-27-appimage-stable-release.md)，具体 GNOME 客体脚本见 `scripts/acceptance/appimage-gnome/README.md`。旧文档中的模拟 updater 结果不视为真实旧客户端升级证据。

- 发布前远端基线：`origin/main` 为 `910d3691`，公开稳定版为 `v1.9.1`，`v1.9.2` 未占用。此次提交范围包含 `e6590d52` 的全部修复和本轮版本／收口记录；最终成品见上文。
- 宿主起始基线为 `1.9.0-beta.21`，`patinad.service` active/running、NRestarts=0，后台登录启动 enabled；最终安装与验收结果见上文。
- 沙箱内无法访问用户总线、Docker 和 KVM；经系统权限核对后，实际 `/dev/kvm` 正常，既有 GNOME 42/46 与 Fedora 工具容器和磁盘均保留。已启动 GNOME 42 工具容器，仅用于隔离验收。
- `1.9.2` 已同步版本文件，Cargo.lock 仅产品版本变化；`test:release` 与 `release:check` 通过，含 720 Rust passed / 21 ignored、56 个 TypeScript 测试文件、38 项浏览器回归、Clippy、扩展和发布元数据检查。准确提交导出检查与公开成品核验均已通过。
- GNOME 42 客体起始状态先另存为 `pre-closeout-20261002`。旧版验收使用 `pristine-gnome` 安装前快照；最终成功状态保存为 `public184-to-stable192-pass`，证据已导出，客体受控关机且工具容器停止。
- 用户直接确认后，`main` 与全新 `v1.9.2` 标签已推送至准确提交 `ecb018f5af11494330303b923b7aecd3884fcac6`；干净提交导出的版本和 changelog 校验通过。[正式发布工作流 36960195478](https://github.com/Asanilo/patina-Linux/actions/runs/36960195478)成功。
- 真实旧客户端的起始进程来自公开 `1.8.4` AppImage（SHA256 `3b65a1f1205acbf2027201058ec65c87a20988b859f26f01976d656289c5c557`）的 FUSE 挂载，本地 API 报告 `1.8.4`。安装前快照缺少的字体和可访问性依赖只补在客体中；最终升级结果见上文。
- 宿主 baseline、在线和停机一致数据库备份、配置备份及升级后核验完成，证据及本轮完整门禁日志位于 `$HOME/.local/state/patina/acceptance/20261002-stable192-closeout/`。公开版、安装版及运行后台均为 1.9.2；Desktop 登录自启关闭、后台登录启动开启。
