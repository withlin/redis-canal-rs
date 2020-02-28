use crate::filter::*;
use crate::formatter;
use crate::parse;
use crate::parser::RdbParser;
use redis;
use redis::{cmd, Connection};
use std::collections::HashMap;
use std::io::prelude::*;
use std::io::BufReader;
use std::io::Error;
use std::net::TcpStream;
use std::rc::Rc;

pub type CanalError = Error;

pub type CanalResult<T> = Result<T, CanalError>;

pub type CanalOk = CanalResult<()>;

pub struct Canal {
    pub conn: TcpStream,
    pub repl_master: bool,
    pub db: u8,
    pub replid: String,
    pub offset: i64,
    pub redisInfo: Rc<Option<redis::InfoDict>>,
}

impl Canal {
    pub fn new(addr: String, db: u8, offset: i64) -> Self {
        Canal {
            conn: TcpStream::connect(addr).expect("error conntion"),
            repl_master: false,
            db: db,
            replid: String::from(""),
            offset: offset,
            redisInfo: Rc::new(None),
        }
    }

    pub fn version(&mut self) -> String {
        if let Some(info) = &*self.redisInfo {
            let x: Option<String> = info.get("redis_version").unwrap();
            return format!("{:?}", x);
        };
        "".to_string()
    }

    pub fn is_master(&mut self) -> bool {
        if let Some(info) = &*self.redisInfo {
            let x: Option<String> = info.get("redis_version").unwrap();
            let c = format!("{:?}", x);
            return c == "master";
        }
        false
    }

    fn send_port(&mut self) -> redis::RedisResult<()> {
        let mut port = redis::cmd("REPLCONF");
        port.arg("listening-port");
        port.arg(self.conn.local_addr()?.port());
        self.conn
            .write(port.get_packed_command().as_slice())
            .expect("error conntion");
        println!(
            "Current tcp client listen port:{:?}",
            self.conn.local_addr()?.port()
        );
        let mut b = [0; 4108];
        self.conn.read(&mut b).expect("error conntion");
        let c = redis::parse_redis_value(&b)?;
        let res: String = redis::from_redis_value(&c)?;
        if res != "OK" {
            println!("replconf listening port failed");
        }
        Ok(())
    }

    fn send_ip(&mut self) -> redis::RedisResult<()> {
        let mut ip = redis::cmd("REPLCONF");
        ip.arg("ip-address");
        ip.arg(format! {"{}",self.conn.local_addr()?.ip()});
        self.conn
            .write(ip.get_packed_command().as_slice())
            .expect("error conntion");
        let mut b = [0; 4108];
        self.conn.read(&mut b).expect("error conntion");
        let c = redis::parse_redis_value(&b)?;
        let res: String = redis::from_redis_value(&c)?;
        if res != "OK" {
            println!("replconf listening port failed");
        }
        Ok(())
    }

    fn send_capa(&mut self) -> redis::RedisResult<()> {
        let mut capa = redis::cmd("REPLCONF");
        capa.arg("capa");
        capa.arg("psync2");
        self.conn
            .write(capa.get_packed_command().as_slice())
            .expect("error conntion");
        let mut b = [0; 4108];
        self.conn.read(&mut b).expect("error conntion");
        let c = redis::parse_redis_value(&b)?;
        let res: String = redis::from_redis_value(&c)?;
        if res != "OK" {
            println!("replconf listening port failed");
        }
        Ok(())
    }

    fn send_psync(&mut self) -> redis::RedisResult<()> {
        let mut capa = redis::cmd("psync");
        capa.arg("?");
        capa.arg("-1");
        self.conn
            .write(capa.get_packed_command().as_slice())
            .expect("error conntion");

        let mut b = [0; 4180];
        self.conn.read(&mut b).expect("error conntion");
        let c = redis::parse_redis_value(&b)?;
        let res: String = redis::from_redis_value(&c)?;
        println!("{:?}", res);
        Ok(())
    }
    pub fn replconf(&mut self) -> redis::RedisResult<()> {
        let version = self.version();

        if version.is_empty() {
            //should be return the error
            println!("get version error");
        }

        if version > String::from("4.0.0") {
            let filter = Simple::new();
            self.send_ip()?;
            self.send_port()?;
            self.send_capa()?;
            self.send_psync()?;
            parse(&mut self.conn, formatter::JSON::new(), filter)?;
        }
        Ok(())
    }

    pub fn info(&mut self) -> redis::RedisResult<()> {
        self.conn
            .write(redis::cmd("info").get_packed_command().as_slice())?;
        let mut b = [0; 4108];
        self.conn.read(&mut b)?;
        let info: redis::InfoDict = redis::from_redis_value(&redis::parse_redis_value(&b)?)?;
        self.redisInfo = Rc::new(Some(info));
        Ok(())
    }

    pub fn dump_and_parse(&mut self) -> redis::RedisResult<()> {
        Ok(())
    }

    pub fn handler(&mut self) -> redis::RedisResult<()> {
        Ok(())
    }
}
