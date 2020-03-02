# redis-canal-rs

## 简介
![Build Status](https://github.com/withlin/redis-canal-rs/workflows/Rust/badge.svg?event=push&branch=master)

redis-canal-rs 是一个redis数据同步工具（支持RDB9解析以及AOF解析工具），支持到redis5.x版本。

## 背景

* Redis数据的跨机房同步
* 异构数据的迁移；比如Redis到mysql，MQ等

## 设计

模拟redis slave,然后去dump redis master的rdb和aof

## 用法

```
pub fn main() -> redis::RedisResult<()> {

    let addr = String::from("localhost:6379");
    let password = "pwd".to_string();
    let offset = -1;
    let db = 0;
    let mut canal = rdb::Canal::new(addr, db, offset, password);
    canal.dump_and_parse()?;
    Ok(())
} 

```

## 接下来要做的东西
- [ ] 断点续传继续完善
- [ ] 代码重构
- [ ] 支持async操作
- [ ] 支持读取数据后输出的过滤

## 感谢
* [rdb-rs](https://github.com/badboy/rdb-rs)