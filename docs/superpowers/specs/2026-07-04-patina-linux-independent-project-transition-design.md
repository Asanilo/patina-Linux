# Patina Linux 独立项目迁移设计

状态：规范审查通过，待用户最终确认

日期：2026-07-04

目标版本：`1.9.0`

## 1. 目标

把当前 Linux-first fork 收口为独立维护的 `Patina Linux` 项目，同时保持现有 Linux 用户的数据、扩展、更新和安装连续性。

本轮包含五个连续目标：

1. DEB 与 AppImage updater 按当前安装包类型分流。
2. 建立 `Patina Linux` 的独立项目身份。
3. 在完整备份后原地脱离 GitHub fork network。
4. 分阶段删除不再支持的 Windows 平台代码。
5. 建立安装包体积基线并完成第一轮安全优化。

目标 1 已在 `1.8.3` 后完成并推送。其余目标完成后统一发布 `1.9.0`，不发布中间过渡版本。

## 2. 非目标

- 不迁移或重命名现有 `Patina` 数据目录。
- 不修改 SQLite 数据库文件名或数据格式。
- 不修改 GNOME D-Bus 协议、扩展 UUID 或浏览器扩展内部 ID。
- 不把项目扩张到 Windows、macOS、移动端或团队 SaaS。
- 不为了减小安装包而删除来源不明的运行库。
- 不承诺把旧 GitHub Actions 运行历史重新导入 GitHub UI。
- 不移除仍被 Linux 数据、备份或升级路径使用的跨平台兼容逻辑。

## 3. 身份矩阵

### 3.1 新的项目身份

| 身份面 | 目标值 |
| --- | --- |
| 项目显示名称 | `Patina Linux` |
| GitHub 仓库 | `Asanilo/patina-Linux` |
| 正式版 Tauri identifier | `io.github.asanilo.patinalinux` |
| Local identifier | `io.github.asanilo.patinalinux.local` |
| Dev identifier | `io.github.asanilo.patinalinux.dev` |
| 首个独立版本 | `1.9.0` |

用户可见名称需要在 README、窗口标题、About、托盘、通知、Release 标题和 Linux 桌面菜单中统一为 `Patina Linux`。GNOME 与浏览器扩展属于可复用的协议配套组件，继续使用现有 `Patina Window Tracker` 与 `Patina Web Sync` 名称。

品牌文字应由明确 owner 管理，避免继续在前端和 Rust 中散落新的硬编码名称。不能借此新增无 owner 的通用 `shared` 常量桶。

### 3.2 保持稳定的安装与数据身份

以下名称保持不变：

| 兼容面 | 保持值 | 原因 |
| --- | --- | --- |
| Tauri `productName` | `Patina` | 保持 Debian 包身份和 updater 连续性 |
| `mainBinaryName` | `Patina` | 避免旧安装并排残留 |
| Release 安装包 | `Patina_X.Y.Z_amd64.*` | 保持脚本、用户下载和 updater 契约 |
| 数据目录 | `Patina` | 避免无意义的数据迁移 |
| 配置目录 | `Patina` | 保持 storage anchor 与迁移状态 |
| 数据库 | `patina.db` | 保持数据兼容 |
| 自启动文件 | `Patina.desktop` | 避免重复启动项 |
| WebDAV 默认目录 | `/Patina` | 避免远端备份分叉 |
| API 环境变量与 token 前缀 | `PATINA_*` / `patina_api_` | 保持外接客户端兼容 |

Linux 桌面菜单使用自定义 desktop template 将显示名称设为 `Patina Linux`，但不改变 Debian 包和二进制身份。

### 3.3 保持稳定的协议身份

以下协议、扩展 ID 和扩展显示名保持不变：

- GNOME Shell UUID：`patina-window-tracker@patina`
- D-Bus bus/interface：`org.patina.WindowTracker`
- Firefox 扩展 ID：`patina-web-sync@patina.local`
- Chromium 扩展现有安装身份
- localhost API 路径和 MCP tool 名称
- Agent Skill 的现有兼容调用方式

这些值属于已部署协议，不用于表达仓库归属。修改它们会造成扩展重装、设置丢失或双协议兼容负担。

### 3.4 identifier 变化的路径约束

新的 Tauri identifier 不得改变实际业务数据和 WebView 路径。当前路径 owner 已显式从 identifier 推导目录的父级，再附加稳定的 `Patina` profile 目录；主窗口和 widget 也显式使用 resolved WebView root。

实现时必须先写路径回归测试，再修改三个 identifier。验证至少覆盖：

- Production、Local、Dev profile 识别。
- `~/.config/Patina` 控制目录不变。
- `~/.local/share/Patina` 默认数据与 WebView 根目录不变。
- 自定义数据目录和 WebView anchor 不变。
- API token 仍位于稳定的 `Patina/api_token`。

若 identifier 修改导致任何默认或自定义路径变化，本轮立即停止，不进入 GitHub 脱离阶段。

## 4. 许可与来源说明

项目继续使用 MIT License，并保留上游版权声明。README 应明确说明：

- `Patina Linux` 基于 Ceceliaee 的 Patina 演进而来。
- 当前项目由 Asanilo 独立维护，面向 Linux 桌面。
- Windows 上游功能仅作为选择性参考，不代表当前支持承诺。

脱离 fork network 不得被描述为删除或隐藏上游来源。

## 5. GitHub 备份设计

### 5.1 当前远端基线

截至 2026-07-04：

- 仓库公开，约 6 MB，无子 fork，符合 GitHub 原地 `Leave fork network` 条件。
- 3 个 Releases：`v1.8.0`、`v1.8.2`、`v1.8.3`，每个 6 个附件。
- 20 条 Actions runs。
- 2 个 repository Actions Secrets：`TAURI_SIGNING_PRIVATE_KEY` 与 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。
- 无 Actions Variables。
- 0 issues、0 stars、0 watchers。

GitHub 官方警告离开 fork network 是永久操作，并可能丢失 issues、PR、wiki、stars、watchers、comments、child forks 和其他仓库元数据。设计上把 Releases、Actions 历史和仓库配置全部视为可能丢失，不依赖未承诺的保留行为。

参考：<https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/working-with-forks/detaching-a-fork>

### 5.2 归档位置与权限

备份必须写到 Git 仓库外的持久目录，默认建议：

```text
~/backups/patina-linux-github-detach-YYYYMMDD-HHMMSS/
```

目录权限必须为 `0700`。归档路径不能使用 `/tmp`，不能加入 Git，不能同步到未加密的公开位置。

### 5.3 归档内容

归档至少包含：

- GitHub 远端的 mirror clone，包括全部 branches、tags 和 refs。
- repository metadata、settings、topics、rulesets 和 branch protection 的 JSON 快照。
- 每个 Release 的 JSON、正文和全部附件。
- 每个 Release 附件的 SHA-256、字节大小和下载结果。
- Actions workflows 的仓库版本。
- 20 条 Actions runs 的 metadata、日志和仍可下载的 artifacts。
- repository/environment Variables。
- repository/environment Secrets 的名称、创建时间和更新时间，不包含 secret value。
- 一份机器可读 manifest 和一份人工检查清单。

若脱离后需要重建 Release，GitHub UI 中新的发布时间和部分内部 metadata 无法恢复为原值。离线 JSON 负责保存原始事实，重建只承诺恢复 tag、标题、正文、草稿/预发布状态和附件内容。

### 5.4 Secret 安全

GitHub 不允许读取已保存的 Secret value，因此“备份 Secrets”定义为：

1. 记录 Secret 名称和时间戳。
2. 确认本机仍持有加密的 Tauri 私钥源。
3. 确认私钥密码存在于独立的密码管理位置。
4. 验证私钥对应三个 Tauri 配置中的 updater 公钥。
5. 脱离后通过安全输入重新写入 Secrets。

任何命令输出、manifest、Actions log 或文档都不得包含私钥或密码值。若本机私钥或密码无法验证，禁止脱离 fork network。

### 5.5 完整性门槛

执行 `Leave fork network` 前必须全部满足：

- mirror clone 可读取并包含远端默认分支与全部 tags。
- 18 个 Release 附件全部下载成功并通过记录的大小与 SHA-256 校验。
- 每个 Actions run 至少有 metadata；可用日志和 artifacts 已下载。
- 两个 Secrets 的恢复来源已验证，但值未写入归档。
- 当前 `main` 与 `origin/main` 一致且 Verify workflow 成功。
- 归档 manifest 明确记录无法恢复到 GitHub UI 的 Actions 历史。

## 6. 原地脱离与恢复

### 6.1 不可逆操作边界

最终 `Settings -> Danger Zone -> Leave fork network` 由用户在 GitHub 网页手动确认。自动化不得代替用户执行该不可逆动作，也不得采用“删除仓库后重建”的备用流程。

### 6.2 脱离后验证顺序

脱离完成后按以下顺序验证：

1. `Asanilo/patina-Linux` URL 可访问。
2. GitHub API 返回 `fork=false`。
3. 默认分支、branches、tags 和 commit refs 与 manifest 一致。
4. workflows 存在且 Actions 可执行。
5. Releases 和附件逐项比对，缺失时从归档重建。
6. 重新设置两个 updater Secrets。
7. 触发 Verify workflow。
8. 验证 updater endpoint 仍为同一仓库 URL。
9. 在不创建 tag 的情况下完成发布配置检查。

任一步骤失败时先从归档恢复，不创建 `v1.9.0` tag。

## 7. Windows 平台删除

Windows 删除在 GitHub 独立化完成后执行，但仍属于 `1.9.0` 范围。删除分为独立提交和验证批次。

### 7.1 审计分类

每个 Windows 相关项先分为：

- 纯 Windows 平台实现，可删除。
- Windows-only 依赖或 bundle 配置，可删除。
- 跨平台业务逻辑，不因历史来源于 Windows 而删除。
- 数据、备份或升级兼容逻辑，除非证明 Linux 从未使用，否则保留。
- 图标和文档资产，按 Linux 实际引用关系决定。

### 7.2 删除批次

1. 删除剩余 Windows bundle、CI、updater 和发布配置。
2. 删除 `src-tauri/src/platform/windows/*` 及其注册入口。
3. 删除 Cargo 的 Windows target dependencies 和未使用 feature。
4. 删除仅供 Windows 使用的安装资源、`.ico`、NSIS/WiX 配置和脚本。
5. 清理活跃文档中的 Windows 支持表述，同时保留 MIT 来源说明和必要历史记录。
6. 增加边界检查，阻止 Windows runner、bundle target 和平台模块重新进入默认发布线。

Windows 删除和独立项目身份生效时，必须同步更新以下长期事实源，而不是只修改 README：

- `docs/product-principles-and-scope.md`
- `docs/roadmap-and-prioritization.md`
- `docs/architecture.md`
- `docs/engineering-quality.md`
- `docs/versioning-and-release-policy.md`
- `docs/linux-port-and-api-design.md`
- `README.md` 与 `README.zh-CN.md`

这些文档中的“当前 fork”和“暂时保留 Windows 源码”表述在迁移完成后必须消失。历史执行文档保留原始上下文，不批量改写。

每个批次必须独立通过 Linux 编译和对应测试。不得一次性大删除后再处理编译错误。

## 8. Linux 安装包体积优化

### 8.1 v1.8.3 基线

| 资产 | 字节数 |
| --- | ---: |
| `Patina_1.8.3_amd64.AppImage` | 91,052,536 |
| `Patina_1.8.3_amd64.deb` | 13,406,882 |

删除 Windows 条件编译源码不等于减小 Linux 包。体积优化必须以实际 bundle 内容和 release binary 为依据。

### 8.2 分析方法

- 解包 DEB 与 AppImage，生成按目录和文件排序的体积报告。
- 记录 Rust release binary、前端 dist、图标、WebKit/Tauri 运行依赖和 GNOME 扩展占比。
- 检查 release binary 是否包含可安全移除的调试符号。
- 检查重复图片、未引用图标、重复前端资源和不再使用的 Rust/JS 依赖。
- 对 LTO、strip、codegen units 或 panic 策略只做单变量实验，并记录构建时间、包体积和运行验证。

### 8.3 安全门槛与目标

- 不删除 AppImage 为跨发行版运行所需的库。
- 不以破坏 updater、GNOME 扩展安装或浏览器扩展附件为代价减小主包。
- DEB 与 AppImage 都必须可启动、追踪、打开主窗口、驻留托盘并执行 updater 检查。
- 第一轮目标是至少让一个主安装包相对 v1.8.3 减小 5%，且另一个主安装包不得增长超过 2%。
- 若实验无法达到目标，必须给出逐项体积证据，不得通过高风险删除凑百分比；是否发布由用户重新确认。

## 9. 验证与发布门槛

### 9.1 自动验证

每个架构或发布相关批次至少执行：

```bash
npm test
npm run test:replay
npm run build
npm run release:check
```

身份与路径批次还需执行 Rust 路径、storage、autostart、updater 和扩展定向测试。Windows 删除完成后，边界测试必须证明默认 CI 和 Release 不再包含 Windows target。

### 9.2 手动 Linux 验证

发布 `1.9.0` 前必须分别验证：

- 从 `1.8.3` DEB 应用内更新到候选版本。
- 从 `1.8.3` AppImage 应用内更新到候选版本。
- 旧数据库、分类、Web activity、设置和 storage anchor 保持可读。
- GNOME extension 与 D-Bus 保持可用。
- Firefox/Zen 与 Chromium 扩展无需重装且设置保留。
- `Patina.desktop` 自启动仍指向当前可执行文件。
- 桌面菜单、窗口、About、托盘和 Release 显示 `Patina Linux`。

### 9.3 发布顺序

只有以下条件全部满足才创建 `v1.9.0`：

1. 身份调整已验证且不迁移用户数据。
2. GitHub 已脱离 fork，URL 与 updater endpoint 保持稳定。
3. Releases 与 Secrets 已恢复，Verify 成功。
4. Windows 删除批次全部通过。
5. 安装包体积报告和优化结果已审查。
6. DEB/AppImage updater 真实升级验证通过。
7. 版本文件、CHANGELOG、tag 和 Release 标题一致。

## 10. 成功标准

- GitHub 显示仓库为独立项目，URL 仍为 `Asanilo/patina-Linux`。
- 用户看到的产品名称为 `Patina Linux`。
- 现有 Linux 用户无需迁移数据、重装扩展或重新配置 API/MCP。
- DEB 与 AppImage 继续按安装包类型安全更新。
- 默认源码、依赖、CI 和 Release 不再承诺或构建 Windows。
- v1.9.0 安装包达到体积目标或在发布前由用户明确接受有证据的例外。
- GitHub 脱离前状态具有可校验的离线归档，Secret value 未泄漏。
