# Fire Box

带认证和监控的有状态 LLM API 网关服务。Rust core + 平台原生层（Swift/C++）。

## 约束

- 只允许使用已有依赖，禁止引入新依赖
- 每次修改完运行 `cargo build`、`cargo test`、`cargo check`、`cargo clippy`，确保无 warning

## 目录结构

```
.
├── Cargo.toml           (workspace root)
├── core/                (Rust library crate)
├── macos/               (macOS native layer, 待实现)
├── linux/               (Linux native layer, 待实现)
└── windows/             (Windows native layer, 待实现)
```

## 架构

**三层架构**：
1. **Native Layer**：各平台服务（COM/XPC）处理应用请求、GUI 监控面板、用户交互
2. **Rust Core**：IPC 服务器、认证、指标收集、Provider 代理、keyring 存储
3. **LLM Providers**：OpenAI、Anthropic、DashScope、GitHub Copilot 等（远程）

详见 [core/AGENTS.md](core/AGENTS.md) 了解完整的 IPC API、认证流程、配置管理等细节。

## 质量保证

- ✅ `cargo build`: 编译通过，无警告
- ✅ `cargo check`: 无错误
- ✅ `cargo clippy`: 无警告
- ✅ `cargo test`: 17 单元测试通过

运行完整的质量检查：

```sh
cargo build && cargo check && cargo clippy && cargo test --lib
```

## 模块文档

- [core/AGENTS.md](core/AGENTS.md) — 核心 Rust 库的详细设计文档
- [macos/](macos/) — macOS 原生层（Swift/XPC）
- [linux/](linux/) — Linux 原生层
- [windows/](windows/) — Windows 原生层（C++/COM）

-- 自动记录：状态由开发代理在工作区修改后写入。
