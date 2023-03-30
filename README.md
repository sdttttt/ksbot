# Ksbot

一个尽可能简单的 RSS 订阅机器人, 在Kook上。

RSS解析部分的代码，很大程度参考了iovxw/rssbot的代码,特别感谢.

## TODO

Rust没有特别好的Kook库，只能我来写机器人的运行时了。


### 机器人运行时

-   [x] ws 通信
-   [x] 通信状态机实现
-   [x] 重启恢复会话
-   [x] 事件抽象


### 功能

-   [ ] 实现订阅.
-   [ ] 支持 RSS2.0
-   [ ] 实现推送
-   [ ] 定时推送
-   [ ] 关键词过滤
-   [ ] 分组订阅
