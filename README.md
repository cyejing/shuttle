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

![architecture](/docs/pic/architecture.png)

## Download

下载可执行文件[Release 页面](https://github.com/cyejing/shuttle/releases)

## Quick Start

### 加密上网使用

#### Start Server

`./shuttles examples/shuttles.yaml`

配置参数

```yaml
#example/shuttles.yaml
addrs:
  - addr: 0.0.0.0:4845
    cert: examples/server.crt # 最好使用正式域名证书的方式
    key: examples/server.key
trojan:
  local_addr: 127.0.0.1:80 #nginx伪装
  passwords:
    - sQtfRnfhcNoZYZh1wY9u
```

#### Start Client

`./shuttlec examples/shuttlec-proxy.yaml`

配置参数

```yaml
run_type: proxy #运行类型 代理模式
ssl_enable: true
invalid_certs: true
proxy_addr: 127.0.0.1:4080 #本地代理地址
remote_addr: 127.0.0.1:4845 #服务器地址, 最好是域名
password: sQtfRnfhcNoZYZh1wY9u #对应服务器密码
```

#### 使用

浏览器设置 socks5 代理, 代理端口 proxy_addr

Enjoy

### 内网穿透使用

#### Start Server

`./shuttles examples/shuttles.yaml`

配置参数

```yaml
#example/shuttles.yaml
addrs:
  - addr: 0.0.0.0:4845
    cert: examples/server.crt
    key: examples/server.key
rathole:
  passwords:
    - 58JCEmvcBkRAk1XkK1iH
```

#### Start Client

`./shuttlec examples/shuttlec-rathole.yaml`

配置参数

```yaml
run_type: rathole
ssl_enable: true
remote_addr: 127.0.0.1:4845
password: 58JCEmvcBkRAk1XkK1iH

holes:
  - name: ssh
    remote_addr: 127.0.0.1:4022
    local_addr: 127.0.0.1:22
```

#### 使用

connect -> remote_addr -> local_addr

## License

GNU General Public License v3.0
