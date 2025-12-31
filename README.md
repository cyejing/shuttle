[![ci-test-build](https://github.com/cyejing/shuttle/actions/workflows/ci-test-build.yml/badge.svg)](https://github.com/cyejing/shuttle/actions/workflows/ci-test-build.yml)
[![Version](https://img.shields.io/crates/v/shuttle-station)](https://crates.io/crates/shuttle-station)
[![Documentation](https://img.shields.io/badge/docs-release-brightgreen.svg?style=flat)](https://docs.rs/shuttle-station)
[![License](https://img.shields.io/crates/l/shuttle-station)](https://github.com/cyejing/shuttle/blob/master/LICENSE)

Connect to networks without pain

## Feature

- 加密上网
- 内网穿透
- 客户端代理支持 socks5/http

## Architecture

         ┌──────┐        ┌──────┐
         │ user │        │ user │
         └──┬───┘        └──┬───┘
            │               │
            │          ┌────▼─────┐
            │          │  local   │
            │          │  client  │
            │          └────┬─────┘
            │               │
    ┌───────▼───────────────▼────────┐
    │                                │
    │     public shuttle server      │
    │                                │
    └───────┬───────────────┬────────┘
            │               │
            │               │
       ┌────▼─────┐    ┌────▼─────┐
       │   LAN    │    │ internet │
       │  client  │    └──────────┘
       └──────────┘

## Download

下载可执行文件[Release 页面](https://github.com/cyejing/shuttle/releases)

## Quick Start

### 加密上网使用

#### Start Server

`./shuttle server -c examples/server.yaml`

配置参数

```yaml
#example/server.yaml
listen: 0.0.0.0:4845
tls:
  cert: examples/server.crt # 最好使用正式域名证书的方式
  key: examples/server.key # 最好使用正式域名证书的方式
auth:
  type: password
  password: your_password # 修改为复杂密码
masquerade: # 伪装 http 请求返回内容
  type: string
  string:
    content: hello stupid world
    headers:
      content-type: text/plain
      custom-stuff: ice cream so good
    statusCode: 200
```

#### Start Client

`./shuttle examples/client-proxy.yaml`

配置参数

```yaml
server: localhost:4845
tls:
  insecure: true
proxy: 
  listen: 0.0.0.0:1082
  auth: your_password
  mode: trojan
```

#### 使用

浏览器设置 socks5 代理, 代理端口 proxy_addr

Enjoy

### 内网穿透使用

#### Start Server

`./shuttle examples/server.yaml`

配置参数

```yaml
#example/server.yaml
listen: 0.0.0.0:4845
tls:
  cert: examples/server.crt # 最好使用正式域名证书的方式
  key: examples/server.key # 最好使用正式域名证书的方式
rathole: # 内网穿透使用
  passwords:
    - your_password_hole
masquerade: # 伪装 http 请求返回内容
  type: string
  string:
    content: hello stupid world
    headers:
      content-type: text/plain
      custom-stuff: ice cream so good
    statusCode: 200
```

#### Start Client

`./shuttle examples/client-rathole.yaml`

配置参数

```yaml
server: localhost:4845 # 服务器地址
tls:
  insecure: true # 不校验证书
hole: # 可选 开启内网穿透功能
  auth: your_password_hole
  holes:
    - name: ssh
      remote_addr: 127.0.0.1:4022 # 开启服务器端口
      local_addr: 127.0.0.1:22 # 穿透本地端口
```

#### 使用

connect -> remote_addr -> local_addr

## License

GNU General Public License v3.0
