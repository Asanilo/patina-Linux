# Patina Web Sync for Firefox / Zen

这是 Patina 的 Firefox/Zen 浏览器扩展，用于把当前活动网页同步到本机 Patina。

## 使用前

- 安装并运行 Patina 桌面端。
- 在 Patina 设置中开启网页同步。
- 记下网页同步的端口和 Token，默认端口是 `12345`。

## 在 Zen / Firefox 临时加载

1. 打开 `about:debugging#/runtime/this-firefox`。
2. 点击“临时载入附加组件”。
3. 选择本目录里的 `manifest.json`。
4. 打开扩展选项页，填写 Patina 设置页中的端口和 Token。
5. 打开一个普通网站页面，点击扩展弹窗里的“同步当前页”。

## 同步内容

扩展只同步当前活动网页的网址、标题和网站图标 URL。

扩展不会读取网页正文、表单内容、截图、剪贴板或浏览历史库。
