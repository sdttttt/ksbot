# ChangeLog

### 2023-3-31

- 编写了rss以及http client的测试
- 数据库设计

### 2023-3-30

- 今天移植了Ress的解析代码
- 修复了一些通信状态机的bug
- http client的也写好了，基本都是使用的iovxw/rssbot的代码.

明天尝试测试一下Rss可不可以工作, 还有数据库部分和订阅行为工作部分没做。

### 2023-3-29

**今天开始写ChangeLog了**

- 基本已经完成了整个机器人的KookWS的通信运行时，基本可以正常使用，通信的状态机偶尔会有点小Bug，后面会修复的啦。

以后也会考虑把运行时抽象出去，方便别人开发吧。
明天应该可以开始搞Rss的部分。