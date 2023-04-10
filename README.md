[![ci-test-build](https://github.com/cyejing/shuttle/actions/workflows/ci-test-build.yml/badge.svg)](https://github.com/cyejing/shuttle/actions/workflows/ci-test-build.yml)


Shuttle让互联网更简单

## Feature

- tls加密上网
- 内网穿透, 支持动态调整

## Architecture

![architecture](/doc/pic/architecture.png)

## Download
下载可执行文件[Release页面](https://github.com/cyejing/shuttle/releases)

## Quick Start

### 加密上网使用
#### Start Server
``./shuttles example/shuttles.yaml``

配置参数
```yaml
#example/shuttles.yaml
addrs:
  - addr: 127.0.0.1:4843
#    key: xxx
#    cert: xxx
trojan:
  local_addr: 127.0.0.1:80 #nginx
  passwords:
    - sQtfRnfhcNoZYZh1wY9u
```
#### Start Client
``./shuttlec example/shuttlec-proxy.yaml``

配置参数
```yaml
run_type: proxy #运行类型 代理模式
ssl_enable: true
proxy_addr: 127.0.0.1:4080 #本地代理地址
remote_addr: 127.0.0.1:4843 #服务器地址
password: sQtfRnfhcNoZYZh1wY9u #对应服务器密码

```

#### 使用
浏览器设置socks5代理, 代理端口proxy_addr

Enjoy

### 内网穿透使用
#### Start Server
``./shuttles example/shuttles.yaml``

配置参数
```yaml
#example/shuttles.yaml
addrs:
  - addr: 127.0.0.1:4843
#    key: xxx
#    cert: xxx
rathole:
  passwords:
    - 58JCEmvcBkRAk1XkK1iH
```
#### Start Client
``./shuttlec example/shuttlec-rathole.yaml``

配置参数
```yaml
run_type: rathole
ssl_enable: false
remote_addr: 127.0.0.1:4843
password: 58JCEmvcBkRAk1XkK1iH

holes:
  - name: test
    remote_addr: 127.0.0.1:4022
    local_addr: 127.0.0.1:22

```

#### 使用
connect -> remote_addr -> local_addr
