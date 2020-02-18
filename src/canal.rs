use std::net::TcpStream;
use std::collections::HashMap;
use std::io::Error;



pub type CanalError = Error;

pub type CanalResult<T> = Result<T, CanalError>;

pub type CanalOk = CanalResult<()>;

struct Config{
    addr: String,
    conn: TcpStream,
    repl_master: bool,
    keepalive: u32,
}

struct Canal {
    cfg: Config,
    db: u8,
    replid: String,
    offset: i64,
    redisInfo: HashMap<String,HashMap<String,String>>
}

impl Canal {

    // pub fn new(&mut self) -> Self {
    //     Default::default()
    // }

    pub fn new_canal(&mut self,canal: Canal) -> Self {
        Canal{
            cfg:canal.cfg,
            db:canal.db,
            replid:canal.replid,
            offset:canal.offset,
            redisInfo:canal.redisInfo,
        }
    }

    fn version(&mut self) -> String{
        let mut  version:String = String::from("");
        match self.redisInfo.get("Server") {
            Some(server) => {
                match server.get("redis_version"){
                    Some(v) =>{
                        version = v.to_string();
                    },
                    None => {},
                }
            },
            None => {},
        }
        version
    }

    fn real_master(&mut self) -> (String,String) {
        let mut  host:String = String::from("");
        let mut  port:String = String::from("");
        match self.redisInfo.get("Replication") {
            Some(server) => {
                match server.get("master_host"){
                    Some(rs) => {
                        host = rs.to_string();
                    },
                    None => {},
                }
                match server.get("master_port"){
                    Some(rs) => {
                        port = rs.to_string();
                    },
                    None => {},
                }
                match server.get("master_replid"){
                    Some(rs) => {
                        self.replid = rs.to_string();
                    },
                    None => {},
                }
            },
            None => {},
        }
        (host,port)
    }

    fn is_master(&mut self) -> bool {
        let mut  role:String = String::from("");
        match self.redisInfo.get("Replication"){
            Some(rs) => {
                match rs.get("role"){
                    Some(rs) => {
                        role = rs.to_string();
                    },
                    None => {},
                }
            },
            None => {},
        }
        role == "master"
    }

    // fn replconf(&mut self) -> CanalOk {
    //     let version = self.version();
    //    if version.is_empty() {
    //        Err("get version error")
    //    }
    //    if version.parse::<i32>().unwrap() >  400 {
           
    //    }
    // }


}



// fn test(){
//     let mut stream = TcpStream::connect("127.0.0.1:34254").unwrap();
//     stream.read(buf: &mut [u8])
// }
