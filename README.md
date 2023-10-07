# Ksbot

一个尽可能简单的RSS订阅bot, 用于Kook, 使用WS通信, 易于部署。
该bot的rss部分设计来自[iovxw/rssbot](https://github.com/iovxw/rssbot).

- [x] RSS 2.0
- [x] Atom 1.0

## 直接进行一个邀请

服务器过期了, 没了呜呜呜


## Using
```
@机器人 rss       - 显示当前订阅的 RSS 列表
@机器人 sub       - 订阅一个 RSS: @机器人 sub http://example.com/feed.xml
@机器人 unsub     - 退订一个 RSS: @机器人 unsub http://example.com/feed.xml
@机器人 reg       - 设置过滤正则: @机器人 reg http://example.com/feed.xml (华为|蒂法)
```

代码基本就三部分，网络事件运行时和RSS订阅事件，和kv数据库.

## build

需要 `rustc 1.68` 以上的版本

```
cargo build --release
```

### Deploy

```
# 编译困难，或者无法使用Release中的二进制 Docker
docker pull ghcr.io/sdttttt/ksbot:master
docker run -d --name ksbot-master -e TOKEN=<token> ksbot:master

# 二进制：
ksbot -t <token>
```

### Contribution

源代码说明:
- `/api` kook API
- `/fetch` RSS 序列化
- `network_frame.rs` ws消息序列化
- `network_runtime.rs` 机器人网络的运行时, kook的ws状态管理都在这里完成.
- `runtime.rs` 机器人的运行逻辑. 包括命令处理, 机器人的内部状态还有定时任务.
- `push.rs` 消息推送
- `db.rs` 持久化
