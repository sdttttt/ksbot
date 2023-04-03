# Ksbot

一个尽可能简单的RSS订阅bot, 用于Kook, 使用WS通信, 易于部署。
该bot的rss部分设计来自[iovxw/rssbot](https://github.com/iovxw/rssbot).

> ⚠ **目前只支持RSS2.0**

代码基本就三部分，网络事件运行时和RSS订阅事件，和kv数据库.

## Using
```
/rss       - 显示当前订阅的 RSS 列表
/sub       - 订阅一个 RSS: /sub http://example.com/feed.xml
/unsub     - 退订一个 RSS: /unsub http://example.com/feed.xml
```

## build

需要 `rustc 1.68` 以上的版本

```
cargo build --release
```

### Deploy

```
ksbot -t <token>
```
