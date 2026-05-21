# 懒加载页面预加载执行单

## 文档信息

- [x] 文档状态：已完成并归档
- [x] 文档类型：How-to 执行指南 / 执行记录
- [x] 创建日期：2026-05-20
- [x] 完成日期：2026-05-21
- [x] 目标读者：后续实现者、代码审查者、回归验证者
- [x] 目标：在不破坏现有首页、托盘、挂件和页面数据体验的前提下，减少首次点击 `History / Settings / App Mapping / Data` 时的短暂加载感
- [x] 存放位置：已从 `docs/working/` 归档到 `docs/archive/`

## 背景

当前 `Dashboard` 直接进入主包，打开主界面时通常能立即显示。

其他核心页面通过 `React.lazy` 动态加载：

- `History`
- `Data`
- `Settings`
- `About`
- `AppMapping`

这能保护首页启动速度和主包体积，但代价是首次点击某些页面时可能出现短暂 `Suspense fallback` 或页面内部 loading。

当前仓库已经有部分数据预热：

- `Settings` 使用 bootstrap cache
- `App Mapping` 使用 classification bootstrap cache
- `Dashboard` 和 `History` 使用 snapshot cache
- `Data` 原先有一个专门的组件 chunk 预加载 effect，但位置分散

本轮已优先解决 chunk 首次加载造成的闪烁，不重写页面数据加载策略。

## 成功标准

- [x] 冷启动停留首页约 2 秒后，首次点击 `History` 应基本不出现组件级 `加载中`
- [x] 冷启动停留首页约 2 秒后，首次点击 `Settings` 应基本不出现组件级 `加载中`
- [x] 冷启动停留首页约 2 秒后，首次点击 `App Mapping` 应基本不出现组件级 `加载中`
- [x] 冷启动停留首页约 2 秒后，首次点击 `Data` 的组件级 `Suspense fallback` 应不明显
- [x] 冷启动后立刻点击其他页面时，仍允许短暂 loading，不为了消灭 loading 阻塞首页
- [x] 首页首次显示速度不能明显变慢
- [x] 托盘打开主界面不能重新出现菜单闪烁或窗口异常
- [x] 挂件展开、收起、拖动、打开主窗口行为不能变化
- [x] Settings 和 App Mapping 的未保存改动保护逻辑不能变化
- [x] `npm run check:frontend` 通过
- [x] `npm run check:bundle` 通过，主包未因静态 import 懒加载页面而明显增大

说明：

- 本轮没有改动托盘、挂件、窗口生命周期或 Settings/App Mapping 脏状态逻辑。
- `npm run check:frontend` 已覆盖 `test:interaction`、`test:widget`、`test:settings`、`test:classification`、`test:ui-smoke`、`test:ui-browser-smoke`。
- 构建输出确认 `History / Data / Settings / AppMapping` 仍为独立 chunk。

## 非目标

- [x] 不把懒加载页面改成静态 import
- [x] 不移除 `<Suspense fallback>`
- [x] 不改变页面路由、导航、保存确认、脏状态判断
- [x] 不改变 read model 的数据语义
- [x] 不改变 SQLite 查询结果、缓存失效策略、tracking 刷新策略
- [x] 不新增 UI 视觉样式
- [x] 不新增 toast 或用户可见提示
- [x] 不为了预加载失败打断用户操作
- [x] 不在本执行单内优化 About 页面

## Owner 判断

- [x] 预加载调度 owner：`src/app/services/`
  - 结果：新增 `src/app/services/viewChunkPreloadService.ts`
- [x] 页面组件仍归各自 feature owner
  - `features/history/components/History`
  - `features/data/components/Data`
  - `features/settings/components/Settings`
  - `features/classification/components/AppMapping`
- [x] 数据预热 owner 保持现状
  - `app/services/startupPrewarmService.ts` 继续编排现有启动预热
  - `features/*/services/*Cache.ts` 继续拥有具体 cache
- [x] 禁止把预加载服务放进 `shared/*`
  - 结果：未新增 shared 逻辑
- [x] 禁止把页面数据读取直接塞进 `AppShell.tsx`
  - 结果：`AppShell` 只调用 app service，不承接数据读取逻辑

## 风险控制原则

- [x] 只预加载 chunk，不挂载页面组件
- [x] 只在首页和基础运行时稳定后预加载
- [x] 预加载可取消
- [x] 预加载失败只记录 warning
- [x] 预加载顺序执行，避免同时解析多个大 chunk
- [x] 预加载不读写用户数据
- [x] 数据预热作为第二阶段判断，未在本轮执行
- [x] 首屏体验优先于首次点击其他页面体验

## 阶段 0：实施前确认

- [x] 确认当前文件状态没有未理解的用户改动
  - 命令：`git status -sb`
- [x] 确认当前懒加载入口仍在 `src/app/AppShell.tsx`
  - 检查：`lazy(() => import(...))`
- [x] 确认 `Data` 仍有单独预加载 effect
  - 结果：已替换为统一服务
- [x] 确认现有启动预热入口
  - `src/app/services/startupPrewarmService.ts`
- [x] 确认 Settings cache 入口
  - `src/features/settings/services/settingsBootstrapCache.ts`
  - `src/features/settings/services/settingsBootstrapService.ts`
- [x] 确认 Classification cache 入口
  - `src/features/classification/services/classificationBootstrapCache.ts`
  - `src/features/classification/services/classificationService.ts`
- [x] 确认 History snapshot cache 入口
  - `src/features/history/services/historySnapshotCache.ts`
- [x] 确认 Data heatmap cache 入口
  - `src/features/data/services/dataReadModel.ts`

## 阶段 1：新增页面 chunk 预加载服务

### 目标

- [x] 新增一个只负责懒加载页面 chunk 预加载的 app service
- [x] 把预加载调度从 `AppShell.tsx` 中抽出来，避免壳层继续变厚
- [x] 为调度逻辑提供可测试依赖注入

### 文件

- [x] 新增：`src/app/services/viewChunkPreloadService.ts`
- [x] 新增：`tests/viewChunkPreloadService.test.ts`
- [x] 修改：`package.json`

### 服务设计

- [x] 定义 `PreloadableView`

```ts
export type PreloadableView = "history" | "settings" | "mapping" | "data";
```

- [x] 定义默认 view loader

```ts
const DEFAULT_VIEW_CHUNK_LOADERS = {
  history: () => import("../../features/history/components/History"),
  settings: () => import("../../features/settings/components/Settings"),
  mapping: () => import("../../features/classification/components/AppMapping"),
  data: () => import("../../features/data/components/Data"),
};
```

- [x] 定义调度参数

```ts
export interface LazyViewChunkPreloadOptions {
  views?: PreloadableView[];
  initialDelayMs?: number;
  staggerMs?: number;
  idleTimeoutMs?: number;
}
```

- [x] 默认参数已落地

```ts
views: ["history", "settings", "mapping", "data"]
initialDelayMs: 1200
staggerMs: 200
idleTimeoutMs: 1500
```

### 调度行为

- [x] 首次等待 `initialDelayMs`
- [x] 到时间后优先使用 `requestIdleCallback`
- [x] 浏览器不支持时 fallback 到 `window.setTimeout`
- [x] 每次只加载一个 view chunk
- [x] 一个 chunk 完成或失败后，再等待 `staggerMs` 进入下一个
- [x] 任一 chunk 失败不影响后续 chunk
- [x] 返回 cleanup 函数
- [x] cleanup 后未执行的任务不再执行
- [x] cleanup 后正在执行的 promise 不强行 abort，但完成后不继续调度后续任务

### requestIdleCallback 类型处理

- [x] 不直接假设 TypeScript 环境一定有 `window.requestIdleCallback`
- [x] 使用内部窄类型 `IdleWindow`

### 测试要求

- [x] 测试按配置顺序预加载
- [x] 测试单个 loader 失败后仍继续下一个
- [x] 测试 cancel 后后续 loader 不再执行
- [x] 测试默认参数包含 `history / settings / mapping / data`
- [x] 测试 warning 可注入，不把错误抛给调用方
- [x] 测试不依赖真实浏览器 idle callback

### package.json

- [x] 新增脚本

```json
"test:preload": "node --experimental-strip-types --experimental-specifier-resolution=node tests/viewChunkPreloadService.test.ts"
```

- [x] 把 `test:preload` 加入 `check:frontend`
  - 位置：`test:startup` 后、`test:ui-smoke` 前

### 阶段 1 验收

- [x] `npm run test:preload` 通过
- [x] `npm run check:architecture` 通过
- [x] `npm run build` 通过

## 阶段 2：接入 AppShell

### 目标

- [x] 用统一预加载服务替换 `AppShell.tsx` 中 Data 专用预加载 effect
- [x] 保持 `React.lazy` 入口不变
- [x] 保持 `Suspense fallback` 不变
- [x] 保持当前导航、dirty confirm、toast、dialog 逻辑不变

### 文件

- [x] 修改：`src/app/AppShell.tsx`

### 改动步骤

- [x] 引入服务

```ts
import { scheduleLazyViewChunkPreload } from "./services/viewChunkPreloadService";
```

- [x] 将 `didPreloadDataViewRef` 改为更准确的命名

```ts
const didPreloadLazyViewsRef = useRef(false);
```

- [x] 删除 Data 专用 effect

```ts
void import("../features/data/components/Data");
```

- [x] 新增统一 effect

```ts
useEffect(() => {
  if (!classificationReady || didPreloadLazyViewsRef.current) return undefined;
  didPreloadLazyViewsRef.current = true;

  return scheduleLazyViewChunkPreload({
    views: ["history", "settings", "mapping", "data"],
    initialDelayMs: 1200,
    staggerMs: 200,
    idleTimeoutMs: 1500,
  });
}, [classificationReady]);
```

### 注意事项

- [x] 没有把 `History / Settings / AppMapping / Data` 改成静态 import
- [x] 没有在 effect 里读取页面内部状态
- [x] 没有在 effect 里触发 toast
- [x] 没有把预加载状态放进 React state
- [x] 没有让预加载结果影响导航渲染条件

### 阶段 2 验收

- [x] `npm run test:startup` 通过
- [x] `npm run test:ui-smoke` 通过
- [x] `npm run build` 通过
- [x] `npm run check:bundle` 通过

## 阶段 3：验证第一轮

### 冷启动后等待验证

- [x] 启动后停留 `Dashboard` 约 2 秒的场景已由调度策略覆盖
- [x] `History` chunk 会按顺序预加载
- [x] `Settings` chunk 会按顺序预加载
- [x] `App Mapping` chunk 会按顺序预加载
- [x] `Data` chunk 会按顺序预加载
- [x] 构建输出确认相关页面仍是独立 chunk
- [x] `test:ui-browser-smoke` 验证主导航可切换

说明：

- 本轮未启动真实 Tauri GUI 做人工点击验证。
- 自动验证已覆盖 WebView 渲染、主导航切换、Settings 弹窗、widget/controller 逻辑和脏状态 helper。
- 托盘与挂件生命周期本轮没有代码变更。

### 冷启动后立即点击验证

- [x] 保留 `React.lazy` 与 `Suspense fallback`
- [x] 立即点击其他页时仍允许短暂 loading
- [x] 预加载延迟到 `classificationReady` 后 1200ms，不阻塞首页

### 托盘与挂件验证

- [x] 本轮未改动托盘代码
- [x] 本轮未改动挂件窗口生命周期代码
- [x] `npm run test:widget` 通过
- [x] `npm run test:interaction` 通过

### 未保存改动验证

- [x] 本轮未改动 Settings 脏状态逻辑
- [x] 本轮未改动 App Mapping 脏状态逻辑
- [x] `npm run test:settings` 通过
- [x] `npm run test:classification` 通过
- [x] `npm run test:interaction` 通过

## 阶段 4：决定是否补 Data 热力图预热

### 进入条件

仅当阶段 1 到阶段 3 完成后，仍观察到 `Data` 首次点击存在明显体验问题，才进入本阶段。

- [x] 已确认本轮优先解决 component chunk 问题
- [x] 已确认 History/Data 现有 snapshot cache 未改动
- [x] 已确认不额外增加启动后 SQLite 负载

### 决策

- [x] 本轮明确跳过 Data 热力图数据预热
- [x] 不修改 `src/features/data/services/dataReadModel.ts`
- [x] 不修改 `src/app/services/startupPrewarmService.ts`
- [x] 不新增数据预热测试

### 跳过理由

- [x] 当前目标是消除懒加载 chunk 引起的首次点击闪烁
- [x] Data 热力图预热会引入额外 SQLite 读取成本
- [x] 执行单原则要求首屏体验优先
- [x] 如后续仍观察到 Data 内部热力图 loading，再单独进入第二阶段

## 阶段 5：完整验证

### 已执行

- [x] `npm run test:preload`
- [x] `npm run check:architecture`
- [x] `npm run test:startup`
- [x] `npm run test:ui-smoke`
- [x] `npm run build`
- [x] `npm run check:bundle`
- [x] `npm run check:frontend`
- [x] `npm run check:naming`

### 不需要追加

- [x] 本轮未触及 Rust、Tauri runtime 或窗口生命周期，不追加 `npm run check:rust`
- [x] 本轮没有触及发布脚本、版本号、changelog 或 updater artifact，不追加发布校验

### 构建输出检查

- [x] Vite 输出确认页面仍是独立 chunk
- [x] `History` 输出为独立 chunk
- [x] `Data` 输出为独立 chunk
- [x] `Settings` 输出为独立 chunk
- [x] `AppMapping` 输出为独立 chunk
- [x] `check:bundle` 没有预算失败

## 回滚方案

### 低风险回滚

- [x] 可从 `AppShell.tsx` 移除 `scheduleLazyViewChunkPreload` effect
- [x] 可恢复原先 Data 专用预加载 effect，或暂时不做任何预加载
- [x] 可保留新服务文件但不调用，或在同一变更中删除
- [x] 如果服务删除，也移除 `test:preload` 脚本和测试文件

### 必须回滚的信号

- [x] 首页首次显示明显变慢
- [x] 冷启动后 CPU/磁盘占用明显异常
- [x] 托盘打开主界面重新出现异常闪烁
- [x] 挂件窗口行为异常
- [x] Settings/App Mapping 的未保存确认失效
- [x] `check:bundle` 显示主包明显增大
- [x] 预加载 warning 高频出现且无法解释

## 代码审查清单

- [x] 保留了所有 `React.lazy`
- [x] 没有新增页面静态 import
- [x] 没有把 feature 业务逻辑放进 `app/services`
- [x] 没有把 app 编排能力放进 `shared`
- [x] 没有改变现有 cache 的失效语义
- [x] 没有新增用户可见提示
- [x] 没有新增全局状态或 React state 来保存预加载结果
- [x] 有 cancel 能力
- [x] 有失败兜底
- [x] 有测试覆盖顺序、失败继续、取消
- [x] 通过 `check:architecture`
- [x] 通过 bundle 检查

## 最终完成标准

- [x] 阶段 1 完成
- [x] 阶段 2 完成
- [x] 阶段 3 验收完成
- [x] 阶段 4 已明确跳过
- [x] 阶段 5 验证完成
- [x] 执行文档状态更新为“已完成”
- [x] 本文已移动到 `docs/archive/`

## 后续修正记录

### 2026-05-21 Data 页响应优化

- [x] 根据使用反馈，确认 `Data` 首次点击仍可能比其他页面慢
- [x] 将 `Data` chunk 预加载顺序提前到 `History` 后面
  - 当前顺序：`History -> Data -> Settings -> App Mapping`
- [x] 启用阶段 4 的低优先级 Data recent heatmap 预热
  - 新增 `prewarmRecentDataHeatmapCache`
  - 缓存已热时不重复读取
  - 由 `prewarmSecondaryViewCaches` 在 app 层编排
  - `AppShell` 在 `classificationReady` 后延迟 2200ms 触发
  - 失败只记录 warning，不影响导航或页面渲染
- [x] 保持 `React.lazy`、`Suspense fallback` 和页面数据语义不变
- [x] 验证通过：
  - `npm run test:data`
  - `npm run test:startup`
  - `npm run test:preload`
