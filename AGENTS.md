# Fire Box

带认证和监控的有状态 LLM API 网关服务。Rust core + 平台原生层（Swift/C++）。
只允许使用已有依赖，禁止引入新依赖。
每次修改完运行 `cargo build`、`cargo test`、`cargo check`、`cargo clippy`，确保无 warning。

## 当前状态（2026-02-13）

### 架构概述

**Cargo Workspace** 结构：
```
Cargo.toml              (workspace root)
crates/
  core/                 (fire-box-core, library crate)
  daemon/               (fire-box, binary crate)
native/
  macos/                (Swift: XPC + SwiftUI, 待实现)
  windows/              (C++: COM + WinUI, 待实现)
```

**三层架构**：
1. **Native Layer**（Swift/C++ per platform）：系统服务注册、COM/XPC 处理本地 App 请求、GUI 监控面板、用户审批弹窗、配置管理界面
2. **Rust Core**（`fire-box-core`）：IPC 服务器、认证管理、指标收集、Provider 通讯、keyring 配置存储
3. **LLM Providers**（远程）：OpenAI、Anthropic、DashScope、GitHub Copilot 等

**通讯路径**（全部通过 interprocess local socket）：
- **App → Native Layer**：通过 COM（Windows）/ XPC（macOS）
- **Native Layer → Rust Core**：通过 local socket（Windows 命名管道 `\\.\pipe\fire-box-ipc`；Unix UDS `/tmp/fire-box-ipc.sock`）发送 HTTP 请求
- **Rust Core → Native Layer**：通过同一 local socket 返回 HTTP 响应（包括 SSE 事件流）
- **Rust Core → LLM Provider**：通过 HTTPS

**IPC Server**（Native Layer 专用，所有请求必须通过 Native Layer）：
  - **对话与认证**：
    - `POST /ipc/v1/chat` — 转发 App 的对话请求
    - `POST /ipc/v1/auth/decide` — 用户审批/拒绝
    - `GET  /ipc/v1/metrics` — 获取监控指标快照
    - `GET  /ipc/v1/apps` — 列出已注册 App
    - `POST /ipc/v1/apps/{id}/revoke` — 撤销 App 授权
  - **配置管理**（所有配置持久化到 OS keyring）：
    - `GET  /ipc/v1/providers` — 列出所有 providers
    - `POST /ipc/v1/providers` — 添加 provider（自动存储 credentials 到 keyring）
    - `PUT  /ipc/v1/providers/{tag}` — 更新 provider
    - `DELETE /ipc/v1/providers/{tag}` — 删除 provider
    - `GET  /ipc/v1/models` — 列出模型映射
    - `POST /ipc/v1/models` — 添加模型映射
    - `DELETE /ipc/v1/models/{tag}` — 删除模型映射
    - `GET  /ipc/v1/settings` — 获取服务设置
    - `PUT  /ipc/v1/settings` — 更新服务设置
  - **事件流**：
    - `GET  /ipc/v1/events` — SSE 事件流（auth_required / metrics_update / request_log / oauth_open_url），通过 local socket 长连接推送

**认证流程**：
1. App 通过 COM/XPC 发出请求到 Native Layer
2. Native Layer 转发到 Core 的 IPC
3. Core 检查 App 是否已授权
4. 未授权：Core 通过 local socket 推送 `auth_required` 事件 → Native 弹出审批窗
5. 用户批准后，Native 调用 `POST /ipc/v1/auth/decide`
6. 此后 App 可持续调用，直至被撤销

**OAuth 设备码流程**（DashScope）：
3. Core 在 preflight 阶段检测到需要设备码认证
2. Core 发起 device code 请求，获取 `verification_uri` 和 `user_code`
3. Core 通过 local socket 推送 `oauth_open_url` 事件（含 provider、url、user_code）→ Native 弹出通知
4. 用户点击通知在浏览器中打开授权页面，输入 user_code
5. Core 轮询 token 端点，获取 access_token 后缓存并继续

**GitHub Copilot 认证流程**（无自动 OAuth）：
1. 用户需预先配置 GitHub token（通过本地配置文件或 keyring）
2. Token 获取位置（按优先级）：
  - 本地配置：`$LOCALAPPDATA/github-copilot/hosts.json` 或 `apps.json`（Windows）
  - 本地配置：`$XDG_CONFIG_HOME/github-copilot/hosts.json` 或 `apps.json`（Unix）
  - OS keyring：`provider:Copilot:api_key`（自动缓存已验证的 token）
3. Core 用 GitHub token 换取 Copilot 短寿命会话 token（`api.github.com/copilot_internal/v2/token`）
4. 会话 token 缓存在内存中，过期自动刷新
5. 对话请求发送到 `api.githubcopilot.com/chat/completions`，附带 VS Code 专有 headers（Vscode-Sessionid、Vscode-Machineid、Editor-Version）
6. Token 被撤销时，尝试从本地配置重新加载；若失败，返回错误提示用户更新 token

注：GitHub 已对 device code endpoint 启用反机器人保护，无法通过编程方式自动 OAuth。未来计划：在用户同意时由 Native 层打开授权网页并回填 token（后续实现）。目前请手动获取 token：https://github.com/settings/tokens（需 'read:user' scope）

### 核心模块

- **`crates/core/src/lib.rs`** — 核心入口，`CoreState` 共享状态，`run()` 无参数启动，从 keyring 加载配置
- **`crates/core/src/auth.rs`** — App 认证/授权管理（注册、审批、撤销、按模型限制），通过 keyring 持久化授权状态
- **`crates/core/src/keystore.rs`** — OS keyring 抽象（存取 provider API key / auth token / App 授权 / 全部配置）
- **`crates/core/src/metrics.rs`** — 实时监控指标（token 用量、请求数、连接数、分模型/Provider/App 统计）
- **`crates/core/src/ipc.rs`** — IPC 服务器（Axum HTTP over interprocess local socket + SSE 事件推送 + 配置管理端点），自定义 `LocalSocketListener` 桥接 interprocess 与 Axum
- **`crates/core/src/provider.rs`** — Provider 客户端（协议编码、Fallback、流式/非流式转发，从 keyring 读取 credentials）
- **`crates/core/src/protocol.rs`** — 统一请求/响应类型
- **`crates/core/src/protocols/`** — 协议编解码（openai.rs、anthropic.rs、dashscope.rs、copilot.rs），dashscope 使用 oauth2 crate 生成 PKCE，copilot 从本地配置读取 GitHub token + 令牌交换
- **`crates/core/src/config.rs`** — 运行时配置类型（从 keyring 加载/保存），类型别名导入 keystore 结构
- **`crates/core/src/models.rs`** — 模型元数据（从 models.dev 加载）
- **`crates/core/src/filesystem.rs`** — 内存文件存储
- **`crates/core/src/session.rs`** — 会话管理
- **`crates/daemon/src/main.rs`** — 无参数服务入口

### 配置管理

**所有配置存储在系统安全存储（Windows Credential Manager / macOS Keychain / Linux Secret Service）中，无配置文件。**

Native Layer 通过 IPC 配置端点管理配置：

- **Providers**：每个 provider 的元数据（tag、type、base_url、oauth_creds_path）和 credentials（API key / auth token）分别存储
- **Models**：模型标签 → provider 映射列表
- **Settings**：运行时设置（log_level、ipc_pipe 名称）

**示例：添加 OpenAI Provider（通过 IPC）**：
```json
POST /ipc/v1/providers
{
  "tag": "OpenAI",
  "type": "openai",
  "base_url": "https://api.openai.com/v1",
  "credential": "sk-..."
}
```

Credential 自动存储到 OS keyring，后续请求从 keyring 读取。

**示例：添加模型映射**：
```json
POST /ipc/v1/models
{
  "tag": "gpt-4",
  "provider_mappings": [
    { "provider": "OpenRouter", "model_id": "openai/gpt-4" },
    { "provider": "OpenAI", "model_id": "gpt-4" }
  ]
}
```

**Keyring 结构**：
- Service Name: `"fire-box"`
- Entries:
  - `provider:OpenAI:api_key` → API key
  - `provider:Anthropic:auth_token` → Auth token
  - `provider:Copilot:api_key` → GitHub token（从本地配置自动缓存）
  - `app_authorizations` → JSON 序列化的 App 授权列表
  - `providers_config` → JSON 序列化的 ProviderInfo 列表
  - `models_config` → JSON 序列化的模型映射
  - `service_settings` → JSON 序列化的服务设置

## 质量保证

- ✅ `cargo build`: 编译通过，无警告
- ✅ `cargo check`: 无错误
- ✅ `cargo clippy`: 无警告
- ✅ `cargo test`: 17 单元测试通过，4 集成测试（`--ignored`）

### 单元测试

```sh
cargo test
```

17 个单元测试覆盖：auth、metrics、models、protocols（dashscope、copilot）、session、provider。

### 集成测试

集成测试需要真实 credentials（`--ignored` 标记）：

```sh
cargo test --test protocol -- --nocapture --ignored
```

**环境变量要求**（OpenAI / Anthropic）：
- `OPENAI_API_KEY` — OpenAI API key
- `OPENAI_BASE_URL` — OpenAI base URL（可选，默认官方）
- `ANTHROPIC_AUTH_TOKEN` — Anthropic auth token
- `ANTHROPIC_BASE_URL` — Anthropic base URL（可选，默认官方）

**GitHub Copilot 测试**：需预先配置 GitHub token（参见上文"GitHub Copilot 认证流程"）：

```sh
cargo test --test copilot -- --nocapture --ignored
```

**DashScope OAuth 设备码测试**：从零授权启动，打印授权 URL 和 user code，需手动复制到浏览器：

```sh
cargo test --test dashscope -- --nocapture --ignored
```

测试会输出（DashScope）：
```
╔════════════════════════════════════════════════════════════════╗
║ DashScope OAuth Authorization Required                        ║
╠════════════════════════════════════════════════════════════════╣
║ Provider:   Copilot-Test                                       ║
║ URL:        https://github.com/login/device                    ║
║ User Code:  ABCD-1234                                          ║
╚════════════════════════════════════════════════════════════════╝

👉 Copy the URL above to your browser and enter the user code.
```

## 使用方式

1. **首次启动**：`./fire-box`（无需参数，自动从 keyring 加载配置，空配置时正常启动）
2. **IPC 服务器启动**：监听 interprocess local socket（Windows: `\\.\pipe\fire-box-ipc`；Unix: `/tmp/fire-box-ipc.sock`）
3. **Native Layer**：
   - 连接 IPC socket 并订阅 SSE 事件（`GET /ipc/v1/events`，通过 local socket 长连接）
   - 通过配置管理端点添加 providers 和 models（存储到 keyring）
   - 接收 COM/XPC 请求并转发到 Core IPC
4. **运行时流程**：
   - App 通过 COM/XPC 发出请求到 Native Layer
   - Native Layer 转：从本地配置/环境变量读取 token → 换取会话 token → 发送请求
   - Core 验证认证、选择 provider、转发到 LLM
   - DashScope OAuth 设备码流程：Core 通过 local socket SSE 推送 `oauth_open_url` 事件 → Native 弹出通知引导用户打开浏览器
   - GitHub Copilot OAuth 设备码流程：同 DashScope，使用 VS Code Client ID 认证后自动换取 Copilot 会话 token
5. **配置更新**：所有配置变更通过 IPC 端点，自动持久化到 keyring

-- 自动记录：状态由开发代理在工作区修改后写入。
