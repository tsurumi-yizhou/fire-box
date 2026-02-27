# FireBox IPC 全量实现计划（CONTROL + CAPABILITY，macOS/XPC）

## 摘要
本次按你确认的约束执行：`CONTROL + CAPABILITY` 全量、命令名 `snake_case`、XPC 原生字典序列化、TOFU 弹 Helper 且调用端无感、拒绝策略为短期拒绝（24h）、新增独立 `CapabilityClient.swift`。  
计划目标是把 [service/src/ipc](/Users/yizhou/fire-box/service/src/ipc) 与 [macos/Sources/App](/Users/yizhou/fire-box/macos/Sources/App) 的 IPC 改成可用且一致，并确保 `cmake --build build` 最终无 warning、无 error。

## 公共接口与类型变更（对实现者必须知晓）
- 在 [service/src/providers/mod.rs](/Users/yizhou/fire-box/service/src/providers/mod.rs) 扩展协议类型：新增 `ToolCall`、`Tool`，`ChatMessage` 增加 `tool_calls/tool_call_id`，`CompletionRequest` 增加 `tools`，`CompletionResponse` 的 message 支持工具调用返回。
- 在 provider trait 的流式事件中支持工具调用输出（完整对象，不做细粒度 delta 对外暴露）。
- 在 [service/src/middleware/route.rs](/Users/yizhou/fire-box/service/src/middleware/route.rs) 增加 `RouteStrategy`（`failover`/`random`）并持久化。
- 在 [service/src/middleware/config.rs](/Users/yizhou/fire-box/service/src/middleware/config.rs) 扩展持久化字段：allowlist/deny 状态与过期时间。
- 在 [service/src/ipc/xpc.rs](/Users/yizhou/fire-box/service/src/ipc/xpc.rs) 的响应体统一为 docs 风格：`result.success/result.message`，并携带各 operation 的 payload 字段。
- 新增 [macos/Sources/App/CapabilityClient.swift](/Users/yizhou/fire-box/macos/Sources/App/CapabilityClient.swift)；[macos/Sources/App/ServiceClient.swift](/Users/yizhou/fire-box/macos/Sources/App/ServiceClient.swift) 迁移到新命令与新响应结构。

## 传输与命令合同（XPC 原生字典）
请求顶层：`cmd` + 该命令字段；响应 `body` 内统一包含 `result`。  
命令集合（全部实现）：
| 协议 | cmd | 关键请求字段 | 关键响应字段 |
|---|---|---|---|
| CONTROL | `add_api_key_provider` | `name, provider_type, api_key?, base_url?` | `result, provider_id` |
| CONTROL | `add_oauth_provider` | `name, provider_type` | `result, provider_id, challenge` |
| CONTROL | `complete_oauth` | `provider_id` | `result, credentials` |
| CONTROL | `add_local_provider` | `name, model_path` | `result, provider_id` |
| CONTROL | `list_providers` | - | `result, providers[]` |
| CONTROL | `delete_provider` | `provider_id` | `result` |
| CONTROL | `get_all_models` | `provider_id?` | `result, models[]` |
| CONTROL | `set_model_enabled` | `provider_id, model_id, enabled` | `result` |
| CONTROL | `set_route_rules` | `virtual_model_id, display_name, capabilities, metadata?, targets[], strategy?` | `result` |
| CONTROL | `get_route_rules` | `virtual_model_id?` | `result` + 单条字段或 `rules[]`（扩展：无 id 时返回全量） |
| CONTROL | `get_metrics_snapshot` | - | `result, snapshot` |
| CONTROL | `get_metrics_range` | `start_ms, end_ms` | `result, snapshots[]` |
| CONTROL | `list_connections` | - | `result, connections[]` |
| CONTROL | `get_allowlist` | - | `result, apps[]` |
| CONTROL | `remove_from_allowlist` | `app_path` | `result` |
| CAPABILITY | `list_available_models` | - | `result, models[]` |
| CAPABILITY | `get_model_metadata` | `model_id` | `result, model` |
| CAPABILITY | `complete` | `model_id, messages[], tools[], temperature?, max_tokens?` | `result, completion, usage, finish_reason` |
| CAPABILITY | `create_stream` | `model_id, temperature?, max_tokens?` | `result, stream_id` |
| CAPABILITY | `send_message` | `stream_id, message, tools[]` | `result` |
| CAPABILITY | `receive_stream` | `stream_id, timeout_ms?` | `result, chunk?, done?, usage?, finish_reason?` |
| CAPABILITY | `close_stream` | `stream_id` | `result` |
| CAPABILITY | `embed` | `model_id, inputs[], encoding_format?` | `result, embeddings[], usage` |
| internal | `ping` | - | `result` |

## 实施步骤（按顺序，决策已锁定）
1. 重构 service IPC 层结构。  
在 [service/src/ipc/mod.rs](/Users/yizhou/fire-box/service/src/ipc/mod.rs) 下拆分 `control.rs`、`capability.rs`、`xpc_codec.rs`（或等价命名），把 XPC object <-> Rust typed data 的转换集中，`unsafe` 只留在 codec 边界，清理当前 `unsafe_op_in_unsafe_fn` 警告。

2. 完成 CONTROL 全量 handler。  
在 control handler 中实现 provider 增删查、模型查询与启停、路由规则读写（含 strategy）、metrics snapshot/range、connections、allowlist revocation；并把旧 `set_route_rule/get_metrics` 路径切到新命令名。

3. 实现 allowlist + TOFU（含 Helper）与连接鉴权。  
新增中间件（建议 [service/src/middleware/access.rs](/Users/yizhou/fire-box/service/src/middleware/access.rs)）：  
连接建立时取 `pid/euid`，解析进程路径与显示名；命中 allow 则放行，命中 deny 且未过期则拒绝，未知或 deny 过期则拉起 Helper。Helper 同意写 allow，拒绝写 deny(24h)。`get_allowlist` 仅返回 allow 条目，`remove_from_allowlist` 删除记录（allow/deny 都删）。

4. 实现连接跟踪与 `list_connections`。  
在 XPC 连接生命周期里记录 `connection_id/client_name/requests_count`，并补充 UI 需要的可选字段（路径/时间戳）以兼容现有界面显示。

5. 扩展 route 与 metrics 能力。  
`route` 增加 `strategy` 持久化与读取迁移；`metrics` 增加范围查询数据源（分钟桶 ring buffer，保留最近 24h），`get_metrics_range` 按 `[start_ms, end_ms]` 过滤输出。

6. 完成 CAPABILITY 全量服务端。  
实现模型发现、metadata、complete、stream 四步、embed。  
`receive_stream` 采用长轮询：默认 `timeout_ms=1000`，超时返回 `success=true, done=false, chunk=nil`。  
stream session 维护消息历史与 tools，`send_message` 触发一次生成任务，`close_stream` 终止并清理。

7. 完成 tool calling 端到端（所有 provider）。  
在 [service/src/providers/openai.rs](/Users/yizhou/fire-box/service/src/providers/openai.rs)、[anthropic.rs](/Users/yizhou/fire-box/service/src/providers/anthropic.rs)、[copilot.rs](/Users/yizhou/fire-box/service/src/providers/copilot.rs)、[dashscope.rs](/Users/yizhou/fire-box/service/src/providers/dashscope.rs)、[llamacpp.rs](/Users/yizhou/fire-box/service/src/providers/llamacpp.rs) 统一接入 tools/tool_calls：  
请求侧传入工具定义，响应侧解析工具调用；流式侧对外仅输出完整 tool_calls 对象。

8. 修复 macOS 并发与弃用警告，统一 IPC 客户端实现。  
新增 [macos/Sources/App/XPCTransport.swift](/Users/yizhou/fire-box/macos/Sources/App/XPCTransport.swift)（原生 XPC 值编解码 + 同步 reply 调用），避免 `withCheckedContinuation` 的 Sendable 报错；移除 `xpc_release` 调用以消除 14.4 deprecation warning。  
改造 [ServiceClient.swift](/Users/yizhou/fire-box/macos/Sources/App/ServiceClient.swift) 使用新命令与 `result` 结构。  
新增 [CapabilityClient.swift](/Users/yizhou/fire-box/macos/Sources/App/CapabilityClient.swift) 提供 CAPABILITY 全 API。

9. 启动路径修正。  
在 [service/src/main.rs](/Users/yizhou/fire-box/service/src/main.rs) 接入 macOS XPC listener 启动（阻塞监听放入专用线程/任务），保证服务二进制运行时真正提供 IPC。

10. 清理 warning 并收口。  
Rust 侧移除未使用 import/函数；Swift 侧修复 Sendable/Deprecated 警告；最终以 `cmake --build build` 作为强验收门槛。

## 测试与验收场景
- 构建验收（硬门槛）：`cmake --build build` 输出中不得出现 warning/error。
- CONTROL 协议场景：  
`add_api_key_provider -> list_providers -> get_all_models -> set_model_enabled -> set_route_rules/get_route_rules -> delete_provider` 全链路成功。
- OAuth 场景：  
`add_oauth_provider` 返回 challenge；`complete_oauth` 成功落库并可在 list_providers 查到。
- TOFU 场景：  
未知调用者首次触发 Helper；允许后后续无感；拒绝后 24h 内无弹窗直接拒绝；过期后再次弹窗。
- CAPABILITY 非流式：  
`list_available_models/get_model_metadata/complete/embed` 正常；tool_calls 在请求与响应中贯通。
- CAPABILITY 流式：  
`create_stream -> send_message -> receive_stream(长轮询) -> close_stream` 正常；无数据超时返回空 chunk 非错误。
- 回归场景：  
现有 macOS Dashboard/Providers/Models/Connections 页刷新不崩溃，关键数据可拉取。

## 明确假设与默认值
- 本轮仅输出决策完整计划，不进行落地改码（按你“只做方案不落地”要求）。
- 命令名统一 `snake_case`；XPC 使用原生 dictionary/array/bool/int/string/double，不走 JSON blob。
- deny TTL 固定 24h。
- `receive_stream` 默认超时 1000ms。
- `get_route_rules` 允许 `virtual_model_id` 为空时返回全量 `rules[]`（为现有控制台列表能力提供扩展）。
- 现有脏工作区改动视为基线，后续实现不得回滚你已有变更。
