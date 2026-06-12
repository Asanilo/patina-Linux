# Patina D+10 兼容代码清理执行方案

状态：待执行（受 D+10 准入门槛约束）  
创建日期：2026-06-10  
计划执行日期：2026-06-20  
来源：从 `docs/archive/patina-identity-and-data-directory-migration-plan.md` 的 D+10 清理部分拆分。  
目标版本：`1.6.0`，用于明确结束旧 Time Tracker 身份兼容窗口。  
相关质量记录：[`patina-architecture-quality-9-plus-execution-plan.md`](../archive/patina-architecture-quality-9-plus-execution-plan.md)

本文件是 D+10 之后清理旧身份兼容代码的独立执行依据。迁移版本 `1.5.2` 负责在 10 天窗口内把旧数据迁移到 Patina；本清理版本负责移除旧身份、旧数据库名、旧备份 identity、旧 credential target、旧远程备份路径和旧安装清理入口的兼容代码。

执行日期说明：这里按 `1.5.2` 在 2026-06-10 公开发布作为 Day 0 计算，D+10 为 2026-06-20。如果实际公开发布日期改变，执行日期必须按实际 Day 0 同步顺延。

## 执行定位

- [ ] 本文件是后续独立兼容清理方案，不纳入当前 `9.0+` 架构与工程质量提升方案的执行范围。
- [ ] 本文件只负责旧 Time Tracker 身份兼容退出，不负责 browser smoke 修复、厚文件重构或性能优化。
- [ ] 在 D+10 准入条件满足前，本文件只允许继续整理，不允许执行代码清理。
- [ ] 到达 D+10 且无延期理由后，直接按本文执行兼容清理，不需要把清理步骤复制回总方案。
- [ ] 清理完成后，单独完成本文验证矩阵、更新评分事实，并按文档卫生规则归档。
- [ ] 如果兼容窗口必须延期，先更新本文的执行日期、延期原因和用户影响说明，再继续保留旧兼容代码。

## 兼容决策矩阵

| 状态 | 默认动作 | 文档动作 | 代码动作 |
| --- | --- | --- | --- |
| `1.5.2` 未公开发布 | 不执行清理 | 保持待执行 | 不改旧兼容 |
| 未满实际 Day 0 后 10 天 | 不执行清理 | 可继续整理清单 | 不改旧兼容 |
| 已满 D+10 且无高优先级迁移问题 | 执行清理 | 勾选准入并实施 | 移除旧身份兼容 |
| 已满 D+10 但存在高优先级迁移问题 | 延期 | 记录延期原因和新日期 | 保留旧兼容 |
| 清理完成并发布 | 归档 | 移入 `docs/archive/` | 后续禁止回流 |

## 不延期时的边界

- [ ] 不新增 10 天后的二次自动迁移机制。
- [ ] 不新增旧身份 quarantine 机制。
- [ ] 不新增旧远程备份路径自动合并机制。
- [ ] 不新增旧 credential target fallback。
- [ ] 不新增旧本地备份 identity 恢复兼容。
- [ ] 不通过 `commands/*`、`lib.rs` 或前端页面层承接旧身份判断。
- [ ] 不删除通用 schema migration、当前 Patina 数据库修复或旧版本 schema 直升保护。
- [ ] 不删除用户旧目录中的未知文件。

## 0. 执行原则

- [ ] 只清理旧身份兼容入口，不改变 Patina 当前身份与当前数据路径。
- [ ] 不再新增任何 10 天后的运行时兼容、隔离、quarantine 或二次迁移机制。
- [ ] 清理版本发布后，迁移前版本直接跳到最新版本时，不再保证自动迁移旧本地数据。
- [ ] 已经在 10 天窗口内完成迁移的用户，应继续无感升级到清理版本。
- [ ] 保留正常数据库 schema migration 能力；只移除旧 Time Tracker 身份兼容，不删除通用 schema 升级机制。
- [ ] 不通过 PowerShell 重定向、`Set-Content`、`Out-File` 或类似输出命令改写中文文档。
- [ ] 代码编辑使用正常补丁方式完成，避免引入编码损坏。
- [ ] 清理完成前不归档本文件。
- [ ] 清理完成并发布后，将本文件和原迁移方案一起移动到 `docs/archive/`。

## 1. 清理范围

### 1.1 必须移除的旧身份兼容

- [ ] 旧生产 identifier：`com.timetracker`。
- [ ] 旧 Local identifier：`com.timetracker.local`。
- [ ] 旧 Dev identifier：`com.timetracker.dev`。
- [ ] 旧数据库名：`timetracker.db`。
- [ ] 旧数据库 WAL/SHM：`timetracker.db-wal`、`timetracker.db-shm`。
- [ ] 旧 roaming 数据目录自动发现：`%APPDATA%\com.timetracker`。
- [ ] 旧 Local roaming 数据目录自动发现：`%APPDATA%\com.timetracker.local`。
- [ ] 旧 Dev roaming 数据目录自动发现：`%APPDATA%\com.timetracker.dev`。
- [ ] 旧 local WebView/cache 清理：`%LOCALAPPDATA%\com.timetracker\EBWebView`。
- [ ] 旧 credential target：`com.timetracker.backup.webdav.default`。
- [ ] 旧本地备份 manifest identity：`TimeTrackerBackup`。
- [ ] 旧远程备份产品路径或 metadata：`Time Tracker`。
- [ ] 旧远程备份设置目录规范化：`/TimeTracker` 自动归一到 `/Patina`。
- [ ] 旧前端本地偏好 key：`time-tracker:*`。
- [ ] 旧 Time Tracker uninstall/autostart 清理 hook。
- [ ] 短暂中间实现的 WebView 清理入口：`%LOCALAPPDATA%\Patina\WebView\EBWebView`。

### 1.2 必须保留的新身份

- [ ] 生产 identifier 保持：`com.ceceliaee.patina`。
- [ ] Local identifier 保持：`com.ceceliaee.patina.local`。
- [ ] Dev identifier 保持：`com.ceceliaee.patina.dev`。
- [ ] 业务数据目录保持：`%APPDATA%\Patina`。
- [ ] Local 业务数据目录保持：`%APPDATA%\Patina Local`。
- [ ] Dev 业务数据目录保持：`%APPDATA%\Patina Dev`。
- [ ] 数据库文件保持：`patina.db`。
- [ ] WebView2 数据根目录保持：`%LOCALAPPDATA%\Patina`，实际缓存为 `EBWebView`。
- [ ] 新 credential target 保持：`com.ceceliaee.patina.backup.webdav.default`。
- [ ] 新本地备份 manifest identity 保持：`PatinaBackup`。
- [ ] 新远程备份产品路径保持：`Patina`。

## 2. 执行前准入检查

### 2.1 日期与发布窗口确认

- [ ] 确认 `1.5.2` 已公开发布。
- [ ] 记录 `1.5.2` 的公开发布日期。
- [ ] 确认当前日期已到 2026-06-20，或已满实际 Day 0 后第 10 个自然日。
- [ ] 确认 `1.5.2` 在完整 10 天窗口内可通过 GitHub Release 下载。
- [ ] 确认 `1.5.2` 在完整 10 天窗口内可通过应用内 updater 发现。
- [ ] 确认没有迁移版本的高优先级数据丢失报告。
- [ ] 确认没有迁移版本的高优先级远程备份 credential 丢失报告。
- [ ] 确认没有必须延长兼容窗口的已知问题。

### 2.2 用户影响确认

- [ ] 确认清理版本不再把 D+10 后仍停留在迁移前版本的用户视为自动迁移目标用户。
- [ ] 确认清理版本发布后，迁移前版本直接跳到最新版本时不再保证自动迁移。
- [ ] 确认发布说明会写明：本地数据迁移在 `1.5.2` 中自动完成。
- [ ] 确认发布说明会写明：D+10 后如果跳过迁移窗口，用户需要先安装迁移窗口版本或走手动恢复路径。
- [ ] 确认发布说明会写明：旧 `Time Tracker` 本地备份不再被清理版本直接恢复。
- [ ] 确认发布说明会写明：旧远程备份路径不再被清理版本自动读取。

### 2.3 仓库状态确认

- [ ] 确认迁移版本相关改动已经提交。
- [ ] 确认当前工作树中没有和清理任务无关的未提交修改。
- [ ] 确认当前分支是准备发布清理版本的目标分支。
- [ ] 记录开始清理前的 Git commit。
- [ ] 记录当前版本文件中的版本号。
- [ ] 确认清理目标版本为 `1.6.0`。

## 3. 兼容引用审计

执行清理前，先用搜索建立待清理清单。搜索结果必须逐项判断，历史 changelog、archive 文档和本执行方案本身可以保留旧词；运行时代码、测试 fixture 和当前发布文档不应保留旧兼容入口。

### 3.1 旧身份与旧数据库搜索

- [ ] 搜索旧 identifier：

```bash
rg -n "com\.timetracker" src-tauri src tests scripts
```

- [ ] 搜索旧数据库名：

```bash
rg -n "timetracker\.db|timetracker-db|sqlite:timetracker" src-tauri src tests scripts
```

- [ ] 搜索旧兼容常量：

```bash
rg -n "LEGACY_|legacy_identifier|legacy_.*dir|legacy.*timetracker" src-tauri src tests scripts
```

### 3.2 旧备份与旧远程备份搜索

- [ ] 搜索旧本地备份 identity：

```bash
rg -n "TimeTrackerBackup|Time Tracker Backup|TimeTracker" src-tauri src tests scripts
```

- [ ] 搜索旧远程备份路径或 product name：

```bash
rg -n "\"Time Tracker\"|time-tracker|timetracker" src-tauri src tests scripts
```

- [ ] 搜索旧前端远程备份设置目录：

```bash
rg -n "LEGACY_WEBDAV_REMOTE_DIR|/TimeTracker" src tests
```

- [ ] 搜索旧前端本地偏好 key：

```bash
rg -n "time-tracker:" src tests
```

### 3.3 旧 credential 与安装清理搜索

- [ ] 搜索旧 credential target：

```bash
rg -n "com\.timetracker\.backup\.webdav\.default|backup\.webdav\.default" src-tauri src tests scripts
```

- [ ] 搜索旧安装器 hook：

```bash
rg -n "patina-migration-hooks|installerHooks|Time Tracker|autostart" src-tauri src tests scripts
```

### 3.4 WebView 兼容清理搜索

- [ ] 搜索旧 WebView 清理入口：

```bash
rg -n "EBWebView|WebView compat|cleanup_webview|Patina[\\\\/]WebView" src-tauri src tests scripts
```

- [ ] 对每个搜索结果标注处理方式：
  - [ ] 删除运行时兼容分支。
  - [ ] 删除只服务兼容分支的测试。
  - [ ] 保留当前 Patina 必需逻辑。
  - [ ] 保留历史 changelog 或 archive 文档。

## 4. 代码清理步骤

### 4.1 路径与身份模块

- [ ] 打开 `src-tauri/src/platform/app_paths.rs`。
- [ ] 移除 `LEGACY_IDENTIFIER_PROD`。
- [ ] 移除 `LEGACY_IDENTIFIER_LOCAL`。
- [ ] 移除 `LEGACY_IDENTIFIER_DEV`。
- [ ] 移除 `AppProfile::from_identifier` 中对旧 identifier 的匹配。
- [ ] 移除 `AppProfile::legacy_identifier`。
- [ ] 移除 `legacy_roaming_data_dir`。
- [ ] 移除 `legacy_local_data_dir`。
- [ ] 保留 `PRODUCT_FOLDER`、`PRODUCT_FOLDER_LOCAL`、`PRODUCT_FOLDER_DEV`。
- [ ] 保留 `IDENTIFIER_PROD`、`IDENTIFIER_LOCAL`、`IDENTIFIER_DEV`。
- [ ] 保留 `product_roaming_data_dir`。
- [ ] 保留 `product_local_data_dir`。
- [ ] 保留 `product_webview_data_dir`，确保它仍返回 product local root。
- [ ] 更新路径单元测试，只验证新身份和用户可见目录。
- [ ] 删除旧 identifier 解析测试。
- [ ] 确认测试仍覆盖“不使用反向域名作为用户可见目录”。

### 4.2 SQLite 启动迁移与旧目录清理

- [ ] 打开 `src-tauri/src/data/sqlite_pool.rs`。
- [ ] 移除 `LEGACY_SQLITE_DB_FILE_NAME`。
- [ ] 移除 `MIGRATION_STATE_FILE_NAME`，除非有非兼容用途。
- [ ] 移除 `MigrationState`。
- [ ] 移除 `LegacyCleanupOutcome`。
- [ ] 移除启动时扫描旧 `%APPDATA%\com.timetracker\timetracker.db` 的逻辑。
- [ ] 移除 `migrate_legacy_database`。
- [ ] 移除旧数据库 `PRAGMA integrity_check` 比对复制逻辑。
- [ ] 移除 `core_table_row_counts`，除非仍有非兼容测试使用。
- [ ] 移除 `write_migration_state`。
- [ ] 移除 `migration_state_path`。
- [ ] 移除 `update_migration_cleanup_state`。
- [ ] 移除 `cleanup_legacy_roaming_data_dir`。
- [ ] 移除 `move_legacy_backup_dir`，除非仍被非兼容功能使用。
- [ ] 移除旧目录已知文件删除逻辑。
- [ ] 保留 `resolve_product_db_path`。
- [ ] 保留打开 `%APPDATA%\Patina\patina.db` 的逻辑。
- [ ] 保留通用 schema migration 执行逻辑。
- [ ] 保留只针对当前 `patina.db` 的 schema repair 或 baseline normalization；不要误删非旧身份兼容的数据库升级能力。
- [ ] 删除只覆盖旧数据库复制和旧目录清理的单元测试。
- [ ] 保留当前 Patina 数据库打开、schema、pool 注册相关测试。
- [ ] 确认清理后没有代码引用 `timetracker.db`。

### 4.3 SQL 前端入口

- [ ] 打开 `src/platform/persistence/sqlite.ts`。
- [ ] 确认只使用 `Database.get("sqlite:patina.db")`。
- [ ] 确认没有 fallback 到 `sqlite:timetracker.db`。
- [ ] 确认 reset/reopen 只调用 Patina 当前 pool。
- [ ] 删除只为旧数据库 URL 准备的测试 stub。

### 4.4 Credential target

- [ ] 打开 `src-tauri/src/platform/credentials.rs`。
- [ ] 移除旧 target 常量 `com.timetracker.backup.webdav.default`。
- [ ] 移除读取新 target 失败后读取旧 target 的 fallback。
- [ ] 移除“读到旧 credential 后复制到新 target”的迁移逻辑。
- [ ] 移除删除 credential 时同时删除旧 target 的逻辑。
- [ ] 保留新 target `com.ceceliaee.patina.backup.webdav.default`。
- [ ] 保留对新 credential 的保存、读取、删除。
- [ ] 更新 credential 单元测试或手动测试说明，只覆盖新 target。
- [ ] 确认清理后没有代码引用旧 credential target。

### 4.5 本地备份 identity

- [ ] 打开 `src-tauri/src/data/backup.rs`。
- [ ] 移除 `TimeTrackerBackup` 作为可接受 manifest identity。
- [ ] 移除旧备份 manifest 恢复兼容分支。
- [ ] 保留 `PatinaBackup`。
- [ ] 保留当前 Patina 备份导出。
- [ ] 保留当前 Patina 备份恢复。
- [ ] 更新错误文案：旧 identity 应被清楚拒绝，而不是被兼容恢复。
- [ ] 更新测试：
  - [ ] 新 Patina 备份可以导出。
  - [ ] 新 Patina 备份可以恢复。
  - [ ] 旧 Time Tracker 备份会被拒绝。
  - [ ] 非法备份 identity 会被拒绝。
- [ ] 确认清理后没有代码引用 `TimeTrackerBackup`。

### 4.6 远程备份路径与 metadata

- [ ] 打开 `src-tauri/src/data/remote_backup.rs`。
- [ ] 打开 `src/platform/persistence/remoteBackupSettingsStore.ts`。
- [ ] 移除旧远程备份产品名或目录名 `Time Tracker`。
- [ ] 移除旧 metadata 读取兼容。
- [ ] 移除新旧远程备份 index merge 逻辑中只服务旧路径的部分。
- [ ] 移除旧远程备份路径发现逻辑。
- [ ] 移除前端把 `/TimeTracker` 自动规范化到 `/Patina` 的兼容分支。
- [ ] 保留新 Patina 远程备份路径。
- [ ] 保留新 Patina metadata 写入和读取。
- [ ] 保留新远程备份同步、恢复、清理当前临时目录的逻辑。
- [ ] 更新测试：
  - [ ] 新远程备份路径写入正确。
  - [ ] 新远程备份 metadata 读取正确。
  - [ ] 旧远程备份路径不会被读取。
  - [ ] 保存过的旧 `/TimeTracker` 设置不再被自动改写成 `/Patina`。
  - [ ] 旧远程备份数据不会被自动删除。
- [ ] 确认清理后没有运行时代码引用旧远程备份路径。

### 4.7 安装器、旧卸载项与 autostart 清理

- [ ] 打开 `src-tauri/tauri.conf.json`。
- [ ] 移除 `bundle.windows.nsis.installerHooks`。
- [ ] 打开 `src-tauri/tauri.local.conf.json`。
- [ ] 确认 Local 配置不再引入旧安装 hook。
- [ ] 打开 `src-tauri/tauri.dev.conf.json`。
- [ ] 确认 Dev 配置不再引入旧安装 hook。
- [ ] 删除 `src-tauri/nsis/patina-migration-hooks.nsh`，或如果目录还有非兼容用途，只删除该 hook 文件。
- [ ] 搜索 `legacy_install`。
- [ ] 移除运行时旧 Time Tracker autostart cleanup 入口。
- [ ] 如果 `src-tauri/src/platform/windows/legacy_install.rs` 只服务旧安装兼容，则删除该文件。
- [ ] 如果 `legacy_install` module 只服务旧安装兼容，则从 module tree 中移除。
- [ ] 更新 `src-tauri/build.rs` 中对旧 hook 或旧 identifier 的检查。
- [ ] 更新安装器相关测试或 release check 期望。
- [ ] 确认清理后安装包仍使用 `productName = "Patina"` 和 `mainBinaryName = "Patina"`。

### 4.8 WebView 兼容清理

- [ ] 打开 `src-tauri/src/app/runtime.rs`。
- [ ] 移除 `cleanup_webview_compat_dirs` 的调用。
- [ ] 打开 `src-tauri/src/data/sqlite_pool.rs`。
- [ ] 移除 `cleanup_webview_compat_dirs`。
- [ ] 移除 `remove_dir_all_if_exists`，如果只服务 WebView/旧目录兼容。
- [ ] 移除 `remove_empty_dir_if_exists`，如果只服务 WebView/旧目录兼容。
- [ ] 移除短暂中间实现 `%LOCALAPPDATA%\Patina\WebView\EBWebView` 的清理测试。
- [ ] 保留 `product_webview_data_dir` 指向 `%LOCALAPPDATA%\Patina`。
- [ ] 确认新启动只使用 `%LOCALAPPDATA%\Patina\EBWebView`。

### 4.9 命令、模块与测试清理

- [ ] 搜索并删除旧兼容 command 或 module 导出。
- [ ] 删除只服务旧兼容的测试 fixture。
- [ ] 删除只服务旧兼容的 UI smoke stub。
- [ ] 删除只服务旧兼容的 Rust 单测。
- [ ] 保留新身份路径、新数据库、新备份和新远程备份测试。
- [ ] 确认 `src-tauri/src/lib.rs` 不重新承载厚迁移逻辑。
- [ ] 确认 `src-tauri/src/commands/*` 不承载迁移业务逻辑。

### 4.10 前端本地偏好 key 清理

- [ ] 打开 `src/app/services/updateRelaunchViewStorage.ts`。
- [ ] 移除 `time-tracker:last-active-view`。
- [ ] 移除 `time-tracker:pending-update-relaunch-view`。
- [ ] 打开 `src/features/history/services/historyLayoutPreferenceStorage.ts`。
- [ ] 移除 `time-tracker:history-day-distribution-mode`。
- [ ] 打开 `src/features/tools/services/toolsLayoutPreferenceStorage.ts`。
- [ ] 移除 `time-tracker:tools-section`。
- [ ] 移除 `time-tracker:tools-timer-mode`。
- [ ] 移除 `time-tracker:tools-reminder-mode`。
- [ ] 移除 `time-tracker:tools-reminder-form-mode`。
- [ ] 更新对应前端测试，只保留 `patina:*` 当前 key 的读写与非法值 fallback。
- [ ] 确认清理后运行时代码不再引用 `time-tracker:*`。

## 5. 文档与发布说明清理

### 5.1 Changelog

- [ ] 在 `CHANGELOG.md` 的 `[Unreleased]` 或目标版本下记录清理版本。
- [ ] 如果目标版本确定为 `1.6.0`，将清理内容整理到 `## [1.6.0] - 2026-06-20`。
- [ ] `Release:` 写明这是结束旧 Time Tracker 兼容窗口的清理版本。
- [ ] `App note:` 写明已完成 Patina 数据目录迁移窗口清理。
- [ ] `Changed` 写明清理版本只使用 Patina 当前数据目录。
- [ ] `Removed` 写明移除旧 Time Tracker 自动迁移、旧备份恢复兼容、旧远程备份路径兼容和旧 credential fallback。
- [ ] 不使用 issue-closing 关键词。

### 5.2 用户说明

- [ ] 写明：已经升级到 `1.5.2` 并成功启动过的用户无需任何操作。
- [ ] 写明：`1.6.0` 后不再从 `%APPDATA%\com.timetracker\timetracker.db` 自动迁移。
- [ ] 写明：仍停留在迁移前版本的用户，需要先安装 `1.5.2` 完成迁移，或使用手动恢复路径。
- [ ] 写明：旧 `Time Tracker` 备份文件不再由清理版本直接恢复。
- [ ] 写明：旧远程备份路径不再由清理版本自动读取。
- [ ] 写明：Patina 当前数据目录仍是 `%APPDATA%\Patina`。

### 5.3 长期文档

- [ ] 检查 `docs/versioning-and-release-policy.md` 是否需要更新当前代码版本说明。
- [ ] 如需调整发布流程或版本线，先更新长期文档，再执行发布。
- [ ] 不把本临时执行方案里的细节复制到长期文档，除非它变成长期规则。

## 6. 验证矩阵

### 6.1 自动验证

- [ ] 运行 Rust 边界检查和 Rust 测试：

```bash
npm run check:rust
```

- [ ] 运行前端与 UI smoke：

```bash
npm run check
```

- [ ] 运行完整发布检查：

```bash
npm run release:check
```

- [ ] 如果版本号已经更新到 `1.6.0`，运行指定版本 changelog 校验：

```bash
npm run release:validate-changelog -- 1.6.0
```

- [ ] 运行旧词搜索，确认运行时代码无旧兼容残留：

```bash
rg -n "com\.timetracker|timetracker\.db|TimeTrackerBackup|com\.timetracker\.backup\.webdav\.default" src-tauri src tests scripts
```

- [ ] 运行旧前端偏好 key 搜索，确认运行时代码无旧 key 残留：

```bash
rg -n "time-tracker:" src tests
```

- [ ] 对搜索结果逐条确认：
  - [ ] 运行时代码无旧兼容。
  - [ ] 当前测试无旧兼容 fixture。
  - [ ] 当前脚本无旧兼容 hook。
  - [ ] 只有 changelog 历史、archive 文档或本执行方案保留旧词。

### 6.2 Windows 手动验证：迁移后用户升级

- [ ] 准备一个已经通过 `1.5.2` 完成迁移的用户环境。
- [ ] 确认该环境存在 `%APPDATA%\Patina\patina.db`。
- [ ] 确认该环境不依赖 `%APPDATA%\com.timetracker\timetracker.db`。
- [ ] 安装或运行清理版本。
- [ ] 确认应用正常启动。
- [ ] 确认历史计时数据仍存在。
- [ ] 确认 Settings、Dashboard、History 正常读取数据。
- [ ] 确认重启应用后仍使用 `%APPDATA%\Patina\patina.db`。
- [ ] 确认 WebView cache 仍在 `%LOCALAPPDATA%\Patina\EBWebView`。
- [ ] 确认没有新建 `%APPDATA%\com.ceceliaee.patina`。
- [ ] 确认没有新建 `%LOCALAPPDATA%\com.ceceliaee.patina`。
- [ ] 确认没有新建 `%APPDATA%\com.timetracker`。
- [ ] 确认没有新建 `%LOCALAPPDATA%\com.timetracker`。

### 6.3 Windows 手动验证：全新安装

- [ ] 在干净 Windows 用户 profile 下安装清理版本。
- [ ] 确认安装目录是 `%LOCALAPPDATA%\Patina`。
- [ ] 确认业务数据目录是 `%APPDATA%\Patina`。
- [ ] 确认数据库名是 `patina.db`。
- [ ] 确认 WebView cache 是 `%LOCALAPPDATA%\Patina\EBWebView`。
- [ ] 确认不会创建 `%APPDATA%\com.timetracker`。
- [ ] 确认不会创建 `%LOCALAPPDATA%\com.timetracker`。
- [ ] 确认不会创建用户可见的 `%APPDATA%\com.ceceliaee.patina`。
- [ ] 确认不会创建用户可见的 `%LOCALAPPDATA%\com.ceceliaee.patina`。
- [ ] 新建一条计时数据。
- [ ] 重启后确认数据仍存在。
- [ ] 导出 Patina 备份并确认可恢复。

### 6.4 Windows 手动验证：跳过迁移窗口的旧用户

本项验证的是“不会继续伪装支持”。预期结果不是自动迁移成功，而是行为与发布说明一致。

- [ ] 准备一个只有 `%APPDATA%\com.timetracker\timetracker.db`、没有 `%APPDATA%\Patina\patina.db` 的旧用户环境。
- [ ] 安装或运行清理版本。
- [ ] 确认应用不读取旧 `com.timetracker` 数据库。
- [ ] 确认应用不删除旧 `com.timetracker` 数据库。
- [ ] 确认应用不尝试把旧数据库复制成 `patina.db`。
- [ ] 确认如果应用创建空的 `%APPDATA%\Patina\patina.db`，该行为已在发布说明或手动恢复说明中解释。
- [ ] 确认手动恢复路径仍可被用户理解。

### 6.5 备份与远程备份验证

- [ ] 新 Patina 本地备份可以导出。
- [ ] 新 Patina 本地备份可以恢复。
- [ ] 旧 `TimeTrackerBackup` 本地备份会被清楚拒绝。
- [ ] 新 Patina 远程备份可以写入。
- [ ] 新 Patina 远程备份可以读取。
- [ ] 旧 `Time Tracker` 远程备份路径不会被自动读取。
- [ ] 缺失 credential 时仍返回当前预期空状态。
- [ ] 新 credential 保存后可以读取。
- [ ] 删除 credential 只操作新 target。

## 7. 发布步骤

- [ ] 更新版本文件到目标版本：
  - [ ] `package.json`
  - [ ] `package-lock.json`
  - [ ] `src-tauri/tauri.conf.json`
  - [ ] `src-tauri/Cargo.toml`
  - [ ] `docs/versioning-and-release-policy.md` 第 3 节当前版本说明
- [ ] 更新 `CHANGELOG.md` 到目标版本节。
- [ ] 运行 `npm run release:check`。
- [ ] 检查 `git status --short`，确认只包含本次清理范围。
- [ ] 提交发布准备改动。
- [ ] 提交信息使用：

```text
release: v1.6.0
```

- [ ] 推送到 `origin/main`。
- [ ] 创建并推送 tag：

```text
v1.6.0
```

- [ ] 确认 GitHub Actions 的 `Publish Release` 工作流已触发。
- [ ] 确认 GitHub Release 标题为 `Patina v1.6.0`。
- [ ] 确认 updater manifest 指向 `1.6.0`。

## 8. 完成标准

- [ ] 清理版本只使用 Patina 当前 identity。
- [ ] 清理版本只使用 Patina 当前用户可见目录。
- [ ] 清理版本只使用 `patina.db`。
- [ ] 清理版本不再扫描 `%APPDATA%\com.timetracker`。
- [ ] 清理版本不再读取 `timetracker.db`。
- [ ] 清理版本不再读取旧 credential target。
- [ ] 清理版本不再接受 `TimeTrackerBackup`。
- [ ] 清理版本不再读取旧 `Time Tracker` 远程备份路径。
- [ ] 清理版本不再包含旧 Time Tracker installer hook。
- [ ] 清理版本不再包含旧 Time Tracker autostart cleanup。
- [ ] 清理版本不再包含短暂中间 WebView 路径清理。
- [ ] 已迁移用户从 `1.5.2` 升级到清理版本不丢数据。
- [ ] 全新安装不创建旧目录。
- [ ] 发布说明明确 D+10 后的边界。
- [ ] `npm run release:check` 通过。
- [ ] 本文件所有适用项已勾选。
- [ ] 原迁移方案已归档。
- [ ] 本清理方案已归档。
