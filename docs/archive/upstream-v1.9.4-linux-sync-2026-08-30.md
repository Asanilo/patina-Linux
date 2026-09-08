# 上游 v1.9.4 Linux 选择性同步执行单

## 目标

基于官方 `upstream/main`（`v1.9.4`）审查共同祖先后的变化，只同步符合 Linux-first 产品边界、能由当前 owner 独立承接且具有明确可靠性收益的改动。

## 本轮范围

1. Data owner：拒绝并清理缺少必需字段的旧首屏快照，避免升级后白屏。
2. Web activity bridge platform owner：监听端口暂时占用时进行有界退避重试，并在设置变化时取消旧重试。
3. Shared motion / component owner：移除 `framer-motion` 运行时依赖，改用现有 Quiet Pro CSS token 与原生元素，并补齐弹窗焦点归属，降低闪烁和额外 bundle 成本。

## 非目标

- 不 merge `upstream/main`，不整批 cherry-pick Windows runtime、installer、updater 或 release 配置。
- 不在本轮引入活动导入、应用/网站详情、网页/分类趋势、定时备份/导出、累计提醒、挂件常驻状态或侧栏文字模式。
- 不只解除“启动时最小化”的 UI 禁用状态；上游实现同时重构了启动来源、设置预读和窗口首帧，Linux 需要独立生命周期批次才能避免手动启动闪窗或开关语义失真。
- 不修改已发布的 Linux 版本标签，也不把 Linux release 版本直接改成官方 `1.9.4`。
- 不合并 `feature/patinad-daemon`。

## Owner 与边界

- 快照结构校验留在 `features/data/services`，持久化删除通过现有 gateway。
- 端口重试属于 `platform/web_activity_bridge` 的外部环境边界，HTTP 业务处理保持不变。
- 动效规则使用共享 token/CSS；页面只替换现有运行时动画调用，不新增页面私有动效系统。

## 验收

- 补充命中改动的前端和 Rust 回归测试。
- 运行 `npm run check`。
- 涉及 Rust runtime，追加 `npm run check:rust`。
- 检查 `git diff --check`，确认 Linux 专属 API、诊断、扩展和存储实现未被上游文件覆盖。

## 完成结果

- Data 快照按 Linux 当前 `DataAppOption` 契约校验必需字段；损坏或过期 payload 会被拒绝并删除。
- 浏览器桥接端口绑定使用有界指数退避；端口释放后可恢复，设置 generation 变化会取消旧重试。
- `framer-motion` 依赖、调用和 smoke stub 已移除；进度反馈改用已有 motion token 的 CSS transition，弹窗补齐初始焦点、Tab 循环、嵌套层级和焦点恢复。
- `npm run check` 的全部非浏览器步骤通过；真实浏览器 smoke 在允许 loopback 的环境中 `25` 项通过。
- `npm run build` 与 `npm run check:bundle` 通过；bundle 门禁不再要求已经移除的 motion chunk，主 chunk 为弹窗焦点管理增加 `1 KiB` 预算。
- `npm run check:rust` 通过：Rust `301` 项通过、`1` 项忽略，Clippy 零警告。
- “静默启动解耦”经边界审查后延期，没有制造只能改 UI、运行时不生效的半实现。

## 后续批次

后续按“数据可信度与高频分析优先”的顺序单独评估：

1. 启动来源与窗口首帧生命周期，并在完整实现后解耦开机自启和静默启动。
2. 完整记录目录与排除统计一致性。
3. SQLite 活动读模型、应用/网站详情、网页和分类趋势。
4. 通用活动导入及批次删除。
5. 定时备份/导出与 WebDAV 安全收口。
6. 跨页面快捷分类与网站快捷分类。
7. 累计活动提醒、挂件常驻状态和侧栏文字模式。
