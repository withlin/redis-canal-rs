# redis-canal-rs

## 简介
![Build Status](https://github.com/withlin/redis-canal-rs/workflows/Rust/badge.svg?event=push&branch=master)

redis-canal-rs 是一个redis数据同步工具（支持RDB9解析以及AOF解析工具），支持到redis5.x版本。


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

## 感谢
* [rdb-rs](https://github.com/badboy/rdb-rs)