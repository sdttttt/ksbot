# Ksbot

一个尽可能简单的RSS订阅bot, 用于Kook, 使用WS通信, 易于部署。
该bot的rss部分设计来自[iovxw/rssbot](https://github.com/iovxw/rssbot).

- [x] RSS 2.0
- [x] Atom 1.0

## 直接进行一个邀请
- [MS-06S](https://www.kookapp.cn/app/oauth2/authorize?id=15283&permissions=268288&client_id=Jttc6p-vEtZoVYVo&redirect_uri=&scope=bot)
- [RX-78-2](https://www.kookapp.cn/app/oauth2/authorize?id=16840&permissions=268288&client_id=EWnSjXXkSnVmfm7-&redirect_uri=&scope=bot)

## Using
```
/rss       - 显示当前订阅的 RSS 列表
/sub       - 订阅一个 RSS: /sub http://example.com/feed.xml
/unsub     - 退订一个 RSS: /unsub http://example.com/feed.xml
/reg       - 设置过滤正则: /reg http://example.com/feed.xml (华为|蒂法)
```

代码基本就三部分，网络事件运行时和RSS订阅事件，和kv数据库.

## build

需要 `rustc 1.68` 以上的版本

```
cargo build --release
```

### Deploy

```
ksbot -t <token>
```
