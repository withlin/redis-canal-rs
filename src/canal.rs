use crate::filter::*;
use crate::formatter;
use crate::parse;
use crate::parser::RdbParser;
use byteorder::ByteOrder;
use byteorder::ReadBytesExt;
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
    pub password: String,
    pub redisInfo: Rc<Option<redis::InfoDict>>,
}

impl Canal {
    pub fn new(addr: String, db: u8, offset: i64,password: String) -> Self {
        Canal {
            conn: TcpStream::connect(addr).expect("error conntion"),
            repl_master: false,
            db: db,
            password: password,
            replid: String::from(""),
            offset: offset,
            redisInfo: Rc::new(None),
        }

    }

    pub fn login_by_password(&mut self) -> redis::RedisResult<()>{
        let mut  auth = redis::cmd("AUTH");
        auth.arg(format!("{}",self.password));
        self.conn
        .write(auth.get_packed_command().as_slice())
        .expect("auth connection error");
        let mut b = [0; 4108];
        self.conn.read(&mut b).expect("auth connection read error");
        let c = redis::parse_redis_value(&b)?;
        let res: String = redis::from_redis_value(&c)?;
        if res != "OK" {
            println!("auth connection read error:{}",res);
        }
        Ok(())
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

    pub fn send_port(&mut self) -> redis::RedisResult<()> {
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

    pub fn send_ip(&mut self) -> redis::RedisResult<()> {
        let mut ip = redis::cmd("REPLCONF");
        ip.arg("ip-address");
        //format! {"{}",self.conn.local_addr()?.ip()}
        ip.arg("10.200.100.219");
        self.conn
            .write(ip.get_packed_command().as_slice())
            .expect("error conntion");
        let mut b = [0; 4108];
        self.conn.read(&mut b).expect("error conntion");
        let c = redis::parse_redis_value(&b)?;
        let res: String = redis::from_redis_value(&c)?;
        if res != "OK" {
            println!("replconf ip-address failed");
        }
        Ok(())
    }

    pub fn send_psync2(&mut self) -> redis::RedisResult<()> {
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
            println!("replconf psync2 failed");
        }
        Ok(())
    }

    pub fn send_eof(&mut self) -> redis::RedisResult<()> {
        let mut eof = redis::cmd("REPLCONF");
        eof.arg("capa");
        eof.arg("eof");
        self.conn
            .write(eof.get_packed_command().as_slice())
            .expect("error conntion");
        let mut b = [0; 4108];
        self.conn.read(&mut b).expect("error conntion");
        let c = redis::parse_redis_value(&b)?;
        let res: String = redis::from_redis_value(&c)?;
        if res != "OK" {
            println!("replconf eof failed");
        }
        Ok(())
    }

    pub fn send_psync(&mut self) -> redis::RedisResult<()> {
        let mut psync = redis::cmd("psync");
        psync.arg("?");
        psync.arg("-1");
        self.conn
            .write(psync.get_packed_command().as_slice())
            .expect("error conntion");

        let mut b = [0; 4180];
        self.conn.read(&mut b).expect("error conntion");
        let c = redis::parse_redis_value(&b)?;
        let res: String = redis::from_redis_value(&c)?;
        println!("{:?}", res);
        Ok(())
    }
    fn replconf(&mut self) -> redis::RedisResult<()> {
        let version = self.version();

        if version.is_empty() {
            //should be return the error
            println!("get version error");
        }

        if version > String::from("4.0.0") {
            self.send_port()?;
            self.send_ip()?;
            self.send_eof()?;
            self.send_psync2()?;
            self.send_psync()?;
            
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

    // fn get_res_to_string(&mut self) -> String {

    // }

    pub fn handler(&mut self) -> redis::RedisResult<()> {
        self.replconf()?;

        loop {
            let buf = self.conn.read_u8()?;
            println!("{:?}", buf as char);
            let buf_array = [buf];
            buf_array.to_vec();
            let a = format!("{:?}", buf as char);
            println!("{}",a);
            match buf as char {
                '+' => {
                    let mut b = [0; 4180];
                    self.conn.read(&mut b).expect("error conntion");
                    let mut v = Vec::new();
                    v.extend_from_slice(&buf_array);
                    v.extend_from_slice(&b.to_vec());
                    let c = redis::parse_redis_value(&v)?;
                    let res: String = redis::from_redis_value(&c)?;
                    println!("{:?}",res);
                    if res.contains("FULLRESYNC") {
                        let filter = Simple::new();
                        parse(&mut self.conn, formatter::JSON::new(), filter)?;
                    }
                    if res.contains("CONTINUE") {
                        let ss: Vec<&str> = res.split(" ").collect();
                        if ss.len() != 2 {
                            //will return the error
                            println!("error CONTINUE resp {:?}", res);
                            break;
                        }
                        self.replid = ss[1].to_string();
                    }
                }
                '*'  => {
                    println!("进来了！");
                },
                _ => (),
            };
            if 1 == 2 {
                break;
            }
        }
        Ok(())
    }
}
