# Fire Box

带认证和监控的有状态 LLM API 网关服务。Rust core + 平台原生层（Swift/C++）。

## 约束

- 只允许使用已有依赖，禁止引入新依赖
- 每次修改完运行 `cargo build`、`cargo test`、`cargo check`、`cargo clippy`，确保无 warning
- 禁止使用 shell 脚本（.sh 文件）

## 目录结构

```
.
├── generated/           (build.rs 自动生成：core.h + libcore.a)
├── core/                (Rust library crate，独立 Cargo 项目)
├── macos/               (macOS native layer，SPM 构建)
├── linux/               (Linux native layer，Meson 构建)
└── windows/             (Windows native layer, 待实现)
```

## 架构

**三层架构**：
1. **Native Layer**：各平台服务（COM/XPC）处理应用请求、GUI 监控面板、用户交互
2. **Rust Core**：IPC 服务器、认证、指标收集、Provider 代理、keyring 存储
3. **LLM Providers**：OpenAI、Anthropic、DashScope、GitHub Copilot 等（远程）

**FFI 层**：使用简单的 C FFI 导出核心函数（`fire_box_start`、`fire_box_stop`、`fire_box_reload`）。
`core/build.rs` 每次编译时自动生成 C 头文件（`generated/core.h`）并将静态库复制到 `generated/libcore.a`。

详见 [core/AGENTS.md](core/AGENTS.md) 了解完整的 IPC API、认证流程、配置管理等细节。

## 构建

分步构建 Rust 核心和平台原生层：

```sh
# 1. 构建 Rust core（自动生成 generated/core.h + generated/libcore.a）
cd core && cargo build

# 2a. macOS: 构建 Swift 可执行文件（SPM）
cd macos && swift build

# 2b. Linux: 构建 C daemon（Meson）
cd linux && meson setup builddir && meson compile -C builddir
```

**生命周期管理**：
- GUI 应用和服务进程的生命周期是独立的
- 服务在 GUI 启动时自动启动（后台线程）
- GUI 退出时服务继续运行
- 用户必须通过 GUI 中的"Stop Service"按钮手动停止服务

## 质量保证

- ✅ `cargo build`: 编译通过，无警告
- ✅ `cargo check`: 无错误
- ✅ `cargo clippy`: 无警告
- ✅ `cargo test`: 17 单元测试通过

运行完整的质量检查：

```sh
cd core && cargo build && cargo check && cargo clippy && cargo test --lib
```

## 模块文档

- [core/AGENTS.md](core/AGENTS.md) — 核心 Rust 库的详细设计文档
- [macos/](macos/) — macOS 原生层（Swift/XPC）
- [linux/](linux/) — Linux 原生层
- [windows/](windows/) — Windows 原生层（C++/COM）

-- 自动记录：状态由开发代理在工作区修改后写入。
