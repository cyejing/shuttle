# Shuttle 项目 Agent 指南

## 项目概述

Shuttle 是一个用 Rust 编写的网络代理和内网穿透工具，主要功能包括：

- **加密上网**：通过 TLS 加密的代理服务，支持 socks5/http 协议
- **内网穿透**：将内网服务暴露到公网
- **WebSocket 支持**：支持通过 WebSocket 进行代理连接
- **流量统计**：提供流量监控和统计功能

## 技术栈

- **语言**: Rust (Edition 2024)
- **异步运行时**: Tokio
- **网络框架**: 
  - `borer-core` (核心网络库，位于 `../borer/borer-core`)
  - `axum` (WebSocket 和 HTTP 服务)
  - `tokio-rustls` (TLS 支持)
- **序列化**: serde, serde_yaml, serde_json
- **日志**: tracing, log
- **CLI**: clap

## 项目结构

```
shuttle/
├── src/
│   ├── main.rs           # 程序入口，CLI 参数解析和启动逻辑
│   ├── lib.rs            # 库入口，模块声明
│   ├── client.rs         # 客户端逻辑（代理和内网穿透）
│   ├── server.rs         # 服务端逻辑（连接处理和分发）
│   ├── config.rs         # 配置文件解析和验证
│   ├── auth.rs           # 认证处理（密码、用户名密码等）
│   ├── websocket.rs      # WebSocket 服务端实现
│   ├── setup.rs          # 日志和追踪设置
│   ├── store.rs          # 服务端状态存储
│   └── rathole/          # 内网穿透模块
│       ├── mod.rs        # 模块入口和客户端逻辑
│       ├── cmd/          # 命令协议实现
│       │   ├── dial.rs   # 拨号命令
│       │   ├── exchange.rs # 数据交换命令
│       │   ├── hole.rs   # 穿透隧道命令
│       │   ├── ping.rs   # 心跳命令
│       │   ├── resp.rs   # 响应命令
│       │   └── unknown.rs # 未知命令处理
│       ├── context.rs    # 连接上下文
│       ├── dispatcher.rs # 命令分发器
│       └── frame.rs      # 协议帧处理
├── examples/             # 示例配置文件
├── tests/                # 集成测试
└── static/               # Web 静态文件
```

## 核心架构

### 运行模式

项目支持两种运行模式：

1. **Server 模式**：启动服务端，监听连接
   - TLS 接受器处理加密连接
   - 协议检测（Trojan、Rathole、伪装流量）
   - 认证和授权
   - 流量统计服务

2. **Client 模式**：启动客户端
   - 代理服务（Direct、Trojan、WebSocket 模式）
   - 内网穿透客户端

### 连接流程

#### 代理流程（加密上网）

```
用户应用 -> 本地代理监听 -> 客户端 -> TLS/WebSocket -> 服务端 -> 目标服务器
```

1. 客户端监听本地代理端口（socks5/http）
2. 接收用户请求，建立到服务端的加密连接
3. 服务端认证后，将流量转发到目标服务器

#### 内网穿透流程

```
外部用户 -> 服务端监听端口 -> Rathole 隧道 -> 客户端 -> 内网服务
```

1. 客户端连接服务端，建立控制通道
2. 发送 Hole 命令注册穿透规则
3. 服务端监听指定端口，收到连接后通过隧道转发到客户端
4. 客户端转发到内网服务

### 协议检测

服务端通过 peek 连接头部来检测协议类型：

- **Trojan**: 56 字节的 SHA224 哈希 + CRLF
- **Rathole**: 同样格式，但在 rathole 密码库中查找
- **Masquerade**: 不匹配以上两种，返回伪装内容

## 关键组件

### 配置系统 (`config.rs`)

- 使用 `serde_yaml` 解析 YAML 配置
- 配置验证和默认值处理
- 密码哈希预计算（SHA224）

**重要配置项**：
- `ClientConfig`: 客户端配置
  - `server`: 服务端地址
  - `proxy`: 代理配置（监听地址、认证、模式）
  - `hole`: 内网穿透配置
  - `tls`: TLS 配置
- `ServerConfig`: 服务端配置
  - `listen`: 监听地址
  - `tls`: TLS 证书配置
  - `auth`: 认证配置
  - `rathole`: 内网穿透密码配置
  - `masquerade`: 伪装配置

### 认证系统 (`auth.rs`)

支持多种认证方式：
- **Password**: 单密码认证
- **Userpass**: 用户名密码认证
- **Http**: HTTP 认证（未实现）
- **Command**: 命令认证（未实现）

所有密码使用 SHA224 哈希存储和传输。

### Rathole 协议 (`rathole/`)

内网穿透使用自定义协议，基于命令模式：

- **Ping**: 心跳保活
- **Hole**: 注册穿透规则
- **Exchange**: 数据交换
- **Dial**: 拨号命令
- **Resp**: 响应命令

命令通过 Dispatcher 分发处理。

### WebSocket 支持 (`websocket.rs`)

提供两种 WebSocket 端点：
- `/fly`: Trojan over WebSocket
- `/clients`: 流量统计 WebSocket 推送
- `/stat`: 统计页面

## 开发指南

### 构建和运行

```bash
# 构建项目
cargo build --release

# 运行服务端
./shuttle server -c examples/server.yaml

# 运行客户端
./shuttle client -c examples/client.yaml
```

### 测试

```bash
# 运行所有测试
cargo test

# 运行特定测试
cargo test --test rathole
```

### 检查

- 检查：`make check`
- 全量特性检查：`make check-all`
- 测试：`cargo test --all`
- 静态分析：`cargo clippy --all-features -- -D warnings`

### 代码风格

- 使用 Rust 2024 Edition
- 遵循标准 Rust 命名约定
- 使用 `anyhow` 进行错误处理
- 使用 `tracing` 进行日志记录
- 异步代码使用 `tokio`

### 添加新功能

1. **添加新的代理模式**：
   - 在 `config.rs` 的 `ProxyMode` 枚举中添加新模式
   - 在 `client.rs` 的 `proxy_handle` 函数中实现处理逻辑
   - 使用 `borer-core` 提供的 Dial trait

2. **添加新的认证方式**：
   - 在 `config.rs` 的 `AuthType` 枚举中添加新类型
   - 在 `auth.rs` 的 `AuthHandler` 中实现认证逻辑

3. **添加新的 Rathole 命令**：
   - 在 `rathole/cmd/` 下创建新命令模块
   - 实现 `Command` trait
   - 在 `dispatcher.rs` 中添加分发逻辑

## 依赖关系

### 外部依赖

- `borer-core`: 核心网络库，提供：
  - Trojan 协议实现
  - TLS 连接处理
  - 代理连接抽象
  - 流量统计
  - WebSocket 适配器

### 内部模块依赖

```
main.rs
├── client.rs
│   ├── config.rs
│   ├── rathole/mod.rs
│   └── borer-core (dial, proxy)
├── server.rs
│   ├── config.rs
│   ├── auth.rs
│   ├── store.rs
│   ├── rathole/dispatcher.rs
│   └── borer-core (trojan, stream, masquerade)
└── websocket.rs
    ├── config.rs
    ├── auth.rs
    └── borer-core (dial, proto, stream)
```

## 常见问题

### 配置文件路径

- 默认客户端配置: `client.yaml` 或 `examples/client.yaml`
- 默认服务端配置: `server.yaml` 或 `examples/server.yaml`
- 可通过 `-c` 参数指定配置文件路径

### TLS 配置

- 服务端需要提供证书和私钥
- 客户端可配置 `insecure: true` 跳过证书验证
- 推荐使用正式域名证书

### 日志配置

- 日志目录默认为 `logs/`
- 可在配置文件中通过 `logs` 字段指定

## 性能考虑

- 使用 Tokio 异步运行时，支持高并发
- 连接处理使用 spawn 独立任务
- Rathole 使用指数退避重连机制
- 流量统计使用内存存储

## 安全注意事项

- 所有密码使用 SHA224 哈希
- 支持 TLS 加密传输
- 支持伪装流量以避免检测
- 认证失败会记录日志

## 扩展点

1. **协议扩展**: 可通过扩展 `rathole/cmd/` 添加新命令
2. **认证扩展**: `AuthHandler` 支持多种认证方式
3. **伪装扩展**: `MasqueradeConfig` 支持多种伪装方式
4. **统计扩展**: 可通过 `traffic_stats` 配置启用统计服务

## 相关资源

- [GitHub 仓库](https://github.com/cyejing/shuttle)
- [Release 下载](https://github.com/cyejing/shuttle/releases)
- [borer-core 库](../borer/borer-core)
