# Patina 远程推送 Worker

这是 Patina 远程推送的最小 Cloudflare Worker 示例。

它通过 `WebSocket` 接收 Patina 的 `snapshot` 消息，并用内存保存每台机器的最新状态。

## 路由

- `/ws`：Patina 使用的 `WebSocket` 接收端。
- `/state`：读取最新状态的 JSON 接口。

## 部署

1. 点击文档里的 `Deploy to Cloudflare`。
2. 在部署页填写 `REMOTE_STATUS_BRIDGE_TOKEN`。
3. 部署完成后，记下 Worker 域名。
4. 在 Patina 里把接收地址设成 `wss://<your-worker-host>/ws`。
5. 把 Patina 的 `Token` 设成和 `REMOTE_STATUS_BRIDGE_TOKEN` 相同的值。

也可以手动部署：

```bash
npm install
wrangler secret put REMOTE_STATUS_BRIDGE_TOKEN
npm run deploy
```

## 本地开发

```bash
npm install
cp .dev.vars.example .dev.vars
npm run dev
```

`.dev.vars` 里的 `REMOTE_STATUS_BRIDGE_TOKEN` 要和本地测试客户端发送的 `Token` 一致。

## 说明

这个示例只保留当前状态，不保留历史记录。它不使用 `D1`、`KV`、`Durable Objects` 或 `Grafana Live`。
