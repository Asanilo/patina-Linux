# Agent 协作与验证门禁修复

> 状态：实施与自动验证完成，已归档。改动基于 `2193acc` 的工作树，未提交或推送。范围仅限 `feature/patinad-daemon`；未切换或修改 `main`，未创建上游贡献分支。

## 目标与 owner

- `scripts` 与 CI：消除多行 import 漏检，让普通自动测试进入统一门禁，覆盖 daemon 实验分支 push。
- Settings feature services：拥有诊断和远端备份的页面流程；组件/hooks 保留呈现和 React 状态，platform 保留环境访问。不新增 shared 抽象或兼容壳，不改变产品行为。
- 长期文档：统一 Linux fork、`main` 产品主线、daemon 实验分支与未来上游贡献分支的定位；按任务读取，减少重复规则。
- daemon 当前清单：逐项关联候选版本、证据、缺口及重跑条件；不重新宣称旧证据代表新版本验收。

## 非目标

- 不调整版本、不构建安装包、不安装、不操作生产数据或服务、不推送、不发布。
- 不修改本地上游 `/home/arinp22/code/patina-windows`；仅用于分析 Linux PR 的讨论边界。
- 本轮子代理由用户临时授权，按独立文件范围并行；不修改用户全局 agent 配置或永久扩大授权。

## 验收

1. AST 门禁覆盖多行 import/export、动态 import，回归用例能识别旧扫描器的漏检。
2. 六条已知 Settings 跨层依赖按 owner 修复，相关行为测试通过。
3. 普通测试递归发现、失败向上传播；专项入口保留，默认链不重复执行同一测试。
4. Verify 包含 `main` 与 `feature/patinad-daemon` push、PR 和手动触发。
5. 文档链接、分支语义、测试入口和验收索引一致；`npm run check:full` 通过。

## 证据

- 基线：`2193acc`，工作树干净；本地 `main` 为 `a13a64a`。
- 本地上游快照：`80204c7`（`release: prepare v1.9.8`）；不等于本轮联网确认远端 HEAD。
- `npm run check:full` 退出 0：54 个 TypeScript 测试文件（含 38 项浏览器回归）、637 Rust passed / 14 ignored、Clippy `-D warnings`、AST/naming/Rust 边界、生产构建及 bundle 预算通过。普通测试从旧链的 50 个增加到 54 个，没有重复；新增四个文件分别是两项遗漏测试、Settings service 行为回归与 runner 自测。
- AST 自测覆盖多行/type/re-export/动态字面量依赖及注释误报；runner 回归覆盖递归发现、精确例外、全排除拒绝、失败短路与子进程错误；Settings 新增 10 项诊断失败隔离、凭据保存/回滚及顺序测试。
- 完整门禁使用获准的本地执行环境运行浏览器、Node 子进程和回环测试；未启动真实环境 opt-in 验收。过程日志为 `/tmp/patina-agent-workflow-check.log`，是可清理的本机证据，不作为永久可访问产物承诺。
- 文档 UTF-8、文件链接/锚点、分支定位与命令入口检查通过；`git diff --check` 通过。代码交叉审查修复了旧测试对手工命令清单的依赖，文档审查修复了普通 Linux 贡献示例指向错误 upstream 的问题。
- 未改版本、未构建安装包、未安装、未操作生产服务或数据、未提交、未推送、未发布。CI 触发配置已修改，尚未在远端运行。

## 上游讨论结论

Linux `main` 是参考实现，新的上游贡献应从指定上游 commit 起步并适配其当前接口。跨发行版通用与跨桌面追踪分开验收；可以先以同一 GNOME provider 在 Debian 系与 Fedora 系验证，再扩展 KDE。用户尚未确认首批版本/环境、包格式或维护者接受范围，本轮未开始移植。
