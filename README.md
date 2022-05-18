[![ci-test-build](https://github.com/cyejing/shuttle/actions/workflows/ci-test-build.yml/badge.svg)](https://github.com/cyejing/shuttle/actions/workflows/ci-test-build.yml)


Shuttle让互联网更简单

## Feature

- tls加密通信通道
- 内网穿透, 支持动态调节

## Architecture

![architecture](/doc/pic/architecture.png)


### 加密上网使用
#### Start Server
``./shuttles -c example/shuttles.yaml``

配置参数
```yaml
#example/shuttles.yaml
addrs:
  - addr: 127.0.0.1:4843
#    key: xxx
#    cert: xxx
  - addr: 127.0.0.1:4880
trojan:
  local_addr: 127.0.0.1:80 #nginx
  passwords:
    - sQtfRnfhcNoZYZh1wY9u
```
#### Start Client
``./shuttlec -c example/shuttlec-socks.yaml``

配置参数
```yaml
run_type: socks #运行类型socks 代理
name: socks
ssl_enable: true
sock_addr: 127.0.0.1:4080 #本地socks5代理
remote_addr: cyejing.cn:4843 #服务器地址
password: sQtfRnfhcNoZYZh1wY9u #对应服务器密码

```

#### 浏览器设置socks5代理
Enjoy

### 内网穿透使用
#### Start Server
``./shuttles -c example/shuttles.yaml``

配置参数
```yaml
#example/shuttles.yaml
addrs:
  - addr: 127.0.0.1:4843
#    key: xxx
#    cert: xxx
  - addr: 127.0.0.1:4880
rathole:
  passwords:
    - 58JCEmvcBkRAk1XkK1iH
```
#### Start Client
``./shuttlec -c example/shuttlec-rathole.yaml``

配置参数
```yaml
run_type: rathole
name: unique-name
ssl_enable: false
remote_addr: 127.0.0.1:4880
password: 58JCEmvcBkRAk1XkK1iH

rats:
  - name: test
    remote_addr: 127.0.0.1:4022
    local_addr: 127.0.0.1:22

```

#### Enjoy
tcp -> remoteAddr -> localAddr
