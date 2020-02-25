use crate::filter::Filter;
use crate::formatter;
use crate::parser::RdbParser;
use redis;
use redis::{cmd, Connection};
use std::collections::HashMap;
use std::io::BufReader;
use std::io::Error;

pub type CanalError = Error;

pub type CanalResult<T> = Result<T, CanalError>;

pub type CanalOk = CanalResult<()>;

struct Config {
    addr: String,
    port: i8,
    conn: Connection,
    repl_master: bool,
}

struct Canal {
    cfg: Config,
    db: u8,
    replid: String,
    offset: i64,
    redisInfo: HashMap<String, HashMap<String, String>>,
}

impl Canal {
    pub fn new_canal(&mut self, canal: Canal) -> Self {
        Canal {
            cfg: canal.cfg,
            db: canal.db,
            replid: canal.replid,
            offset: canal.offset,
            redisInfo: canal.redisInfo,
        }
    }

    fn version(&mut self) -> String {
        let mut version: String = String::from("");
        match self.redisInfo.get("Server") {
            Some(server) => match server.get("redis_version") {
                Some(v) => {
                    version = v.to_string();
                }
                None => {}
            },
            None => {}
        }
        version
    }

    fn real_master(&mut self) -> (String, String) {
        let mut host: String = String::from("");
        let mut port: String = String::from("");
        // self.redisInfo.get("Replication").unwrap()
        match self.redisInfo.get("Replication") {
            Some(server) => {
                match server.get("master_host") {
                    Some(rs) => {
                        host = rs.to_string();
                    }
                    None => {}
                }
                match server.get("master_port") {
                    Some(rs) => {
                        port = rs.to_string();
                    }
                    None => {}
                }
                match server.get("master_replid") {
                    Some(rs) => {
                        self.replid = rs.to_string();
                    }
                    None => {}
                }
            }
            None => {}
        }
        (host, port)
    }

    fn is_master(&mut self) -> bool {
        let mut role: String = String::from("");
        match self.redisInfo.get("Replication") {
            Some(rs) => match rs.get("role") {
                Some(rs) => {
                    role = rs.to_string();
                }
                None => {}
            },
            None => {}
        }
        role == "master"
    }

    fn replconf(&mut self) -> redis::RedisResult<()> {
        let version = self.version();
        // general connection handling
        let client = redis::Client::open("redis://10.200.100.219:6379/")?;

        let mut con = client.get_connection()?;

        if version.is_empty() {
            //should be return the error
            println!("get version error");
        }
        if version > String::from("4.0.0") {
            let mut res: String = cmd("REPLCONF")
                .arg("listening")
                .arg(self.cfg.port)
                .query(&mut self.cfg.conn)?;

            if res != "OK" {
                //should return the error
                println!("replconf listening port failed");
            }
            res = cmd("REPLCONF")
                .arg("capa")
                .arg("psync2")
                .query(&mut self.cfg.conn)?;

            if res != "OK" {
                //should return the error
                println!("replconf capa psync2 failed");
            }
            if self.replid.is_empty() {
                self.replid = String::from("?");
            }

            cmd("psync")
                .arg(&self.replid)
                .arg(self.offset)
                .query(&mut self.cfg.conn)?;
        }
        Ok(())
    }

    fn info(&mut self) -> redis::RedisResult<()> {
        cmd("info").query(&mut self.cfg.conn)?;
        let val = self.cfg.conn.recv_response()?;
        let result: String = format!("{:?}", val);
        let s: Vec<String> = result.split("\n").map(|s| s.to_string()).collect();
        let mut selection = String::from("");
        for x in s.iter() {
            let line = x.trim();
            if !line.is_empty() {
                if x.starts_with("#") {
                    selection = String::from(&x[1..]);
                    continue;
                }
            }
            let mut contentlist: Vec<String> = String::from(line)
                .split(":")
                .map(|s| s.to_string())
                .collect();

            if contentlist.len() < 2 {
                continue;
            }
            let mut map = HashMap::new();
            map.insert(contentlist.remove(0), contentlist.remove(1));
            self.redisInfo.insert(selection.to_owned(), map);
        }
        Ok(())
    }

    fn dump_and_parse(&mut self) -> redis::RedisResult<()> {
        self.replconf()?;
        Ok(())
    }

    fn handler(&mut self) -> redis::RedisResult<()> {
        let mut flag: bool = true;
        while flag {
            // set h x *3\r\n$3\r\nset\r\n$1\r\nx\r\n
            let val = self.cfg.conn.recv_response()?;
            // let err =String::from("-");
            // let integers  =String::from(":");
            // let bulk_string =String::from("$");
            // let sample_string =String::from("+");
            // let arrays =String::from("*");
            // let mut filter = redis_canal_rs::Sample
            // match val  {
            //     redis::Value::Data(ref vd) =>{
            //         String::from(&vd[1..(*vd).len()]).starts_with("FULLRESYNC");

            //     }
            //     redis::Value::Bulk(ref b) =>{
            //         (*b).iter().map(|x|x);
            //     },
            //     _ => (),
            //     // err => (),
            //     // integers => (),
            //     // err => (),
            //     // sample_string => ({
            //     //     let result = format!("{:?}", val);
            //     //     if result.starts_with("FULLRESYNC") {
            //     //         unimplemented!();
            //     //     }
            //     // }),
            //     // err => (),
            //     //  _ => (),

            // };
        }
        Ok(())
    }
}
