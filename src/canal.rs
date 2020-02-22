use redis;
use redis::{cmd, Connection};
use std::collections::HashMap;
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
        if version.is_empty() {
            //should be return the error
            println!("get version error");
        }
        if version > String::from("4.0.0") {
            cmd("REPLCONF")
                .arg("listening")
                .arg(self.cfg.port)
                .query(&mut self.cfg.conn)?;

            let mut val = self.cfg.conn.recv_response()?;
            if format!("{:?}", val) != "OK" {
                //should return the error
                println!("replconf listening port failed");
            }
            cmd("REPLCONF")
                .arg("capa")
                .arg("psync2")
                .query(&mut self.cfg.conn)?;

            val = self.cfg.conn.recv_response()?;
            if format!("{:?}", val) != "OK" {
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

    // fn info(&mut self) -> redis::RedisResult<()> {
    //     cmd("info").query(&mut self.cfg.conn)?;
    //     let mut val = self.cfg.conn.recv_response()?;
    //     let result = format!("{:?}", val);
    //     let str_list: &str = &result[..];
    //     let s:Vec<_> = str_list.split("\n").collect();
    //     s.iter().for_each(|x|{
    //         let line =String::from(*x).trim();
    //         if line.is_empty() {

    //         }else{

    //         }
    //     });
    //     // for i in {
    //     //     let line =str_list[i];
    //     // }
    //     Ok(())
    // }
}
