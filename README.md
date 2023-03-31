# Ksbot

一个尽可能简单的 RSS 订阅机器人, 在Kook上，使用WebSocket通信。
RSS解析部分的代码移植来自ress，很大程度参考了[iovxw/rssbot](https://github.com/iovxw/rssbot)解析部分的设计,真的非常感谢.

> **⚠ 目前只支持RSS2.0**

这个机器人的部署最好在自己的机器上部署，有特意设计成嵌入式KV数据库的模式.
所以只要有配置文件就可以直接跑。

## Desscription

Rust没有特别好的Kook库，只能自己造机器人的运行时。
代码基本就三部分，机器人运行时和RSS事件实现，和数据库部分.

## Database

因为用的是KV数据库，所以数据库的设计比较简单。

- feed::{URL_HASH}            = {URL}          # 订阅源
- channel::feed::{CHANNEL_ID} = {URL_HASH;...}   # 频道对应的订阅源: 分号隔开URL_HASH
- feed::channel::{URL_HASH}   = {CHANNEL_ID;...}   # 订阅源对应的频道: 分号隔开CHANNEL_ID
- feed::date::{URL_HASH}      = {DATE}         # 订阅源最新文章时间

订阅链接和聊天频道是多对多的关系。
添加订阅后，自动添加工作任务。
新文章改变之后推送到频道中。
