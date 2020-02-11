use crate::constants::{constant, version,op_code,encoding};
use crate::types::{RdbOk, RdbResult};
use crate::helper;

use byteorder::{BigEndian, LittleEndian, ReadBytesExt};
use std::io::{Cursor, Error, ErrorKind, Read};
use helper::read_exact;
use lzf;
use crate::Formatter;
use crate::Filter;
// use std::sync::Mutex;
// use redis_canal_rs::constants;
#[inline]
fn other_error(desc: &'static str) -> Error {
    Error::new(ErrorKind::Other, desc)
}

pub struct RdbParser<T: Read, F: Formatter, L: Filter> {
    // input : Option<Mutex<Box<T>>>,
    input: T,
    formatter: F,
    filter: L,
    last_expiretime: Option<u64>,
}

impl<T: Read, F: Formatter, L: Filter> RdbParser<T,F,L> {
    // pub fn new(input:T)->Self{
    //     RdbParser{input:Some(Mutex::new(Box::new(input)))}
    // }
    pub fn new(input: T,format: F,filter: L) -> Self {
        RdbParser { input: input, formatter:format,filter:filter,last_expiretime: None,}
    }

    pub fn parse(&mut self) -> RdbOk {
        let mut last_database: u64 = 0;
        loop {
            let next_op = self.input.read_u8()?;

            match next_op {
                op_code::SELECTDB => {
                    last_database = read_length(&mut self.input)?;
                    if self.filter.matches_db(last_database) {
                        self.formatter.start_database(last_database);
                    }
                }
                op_code::EOF => {
                    self.formatter.end_database(last_database);
                    self.formatter.end_rdb();

                    let mut checksum = Vec::new();
                    let len = self.input.read_to_end(&mut checksum)?;
                    if len > 0 {
                        self.formatter.checksum(&checksum);
                    }
                    break;
                }
                op_code::EXPIRETIME_MS => {
                    let expiretime_ms = self.input.read_u64::<LittleEndian>()?;
                    self.last_expiretime = Some(expiretime_ms);
                }
                op_code::EXPIRETIME => {
                    let expiretime = self.input.read_u32::<BigEndian>()?;
                    self.last_expiretime = Some(expiretime as u64 * 1000);
                }
                op_code::RESIZEDB => {
                    let db_size = read_length(&mut self.input)?;
                    let expires_size = read_length(&mut self.input)?;

                    self.formatter.resizedb(db_size, expires_size);
                }
                op_code::AUX => {
                    let auxkey = read_blob(&mut self.input)?;
                    let auxval = read_blob(&mut self.input)?;

                    self.formatter.aux_field(&auxkey, &auxval);
                }
                _ => {
                    if self.filter.matches_db(last_database) {
                        let key = read_blob(&mut self.input)?;

                        if self.filter.matches_type(next_op) && self.filter.matches_key(&key) {
                            // self.read_type(&key, next_op)?;
                        } else {
                            // self.skip_object(next_op)?;
                        }
                    } else {
                        // self.skip_key_and_object(next_op)?;
                    }

                    self.last_expiretime = None;
                }
            }
        }
        Ok(())
    }
}

// fn read_length_with_encoding(&)
pub fn read_length_with_encoding<T: Read>(input: &mut T) -> Result<(u64, bool), Error> {
    let mut length = 0u64;
    let mut is_encoded = false;

    let enc_type = input.read_u8()?;

    match (enc_type & 0xC0) >> 6 {
        constant::RDB_ENCVAL => {
            is_encoded = true;
            length = (enc_type & 0x3F) as u64;
        }
        constant::RDB_6BITLEN => {
            length = (enc_type & 0x3F) as u64;
        }
        constant::RDB_14BITLEN => {
            let next_byte = input.read_u8()?;
            length = (((enc_type & 0x3F) as u64) << 8) | next_byte as u64;
        }
        constant::RDB_32BITLEN => {
            length = input.read_u32::<BigEndian>()? as u64;
        }
        constant::RDB_64BITLEN => {
            length = input.read_u64::<BigEndian>()?;
        }
        _ => {
            length = input.read_u64::<BigEndian>()?;
        }
    }
    Ok((length, is_encoded))
}

pub fn read_length<R: Read>(input: &mut R) -> RdbResult<u64> {
    let (length, _) = read_length_with_encoding(input)?;
    Ok(length)
}

pub fn verify_magic<R: Read>(input: &mut R) -> RdbOk {
    let mut magic = [0; 5];
    match input.read(&mut magic) {
        Ok(5) => (),
        Ok(_) => return Err(other_error("Could not read enough bytes for the magic")),
        Err(e) => return Err(e),
    };

    if magic == constant::RDB_MAGIC.as_bytes() {
        Ok(())
    } else {
        Err(other_error("Invalid magic string"))
    }
}

pub fn verify_version<R: Read>(input: &mut R) -> RdbOk {
    let mut version = [0; 4];
    match input.read(&mut version) {
        Ok(4) => (),
        Ok(_) => return Err(other_error("Could not read enough bytes for the version")),
        Err(e) => return Err(e),
    };

    let version = (version[0] - 48) as u32 * 1000
        + (version[1] - 48) as u32 * 100
        + (version[2] - 48) as u32 * 10
        + (version[3] - 48) as u32;

    let is_ok = version >= version::SUPPORTED_MINIMUM && version <= version::SUPPORTED_MAXIMUM;

    if is_ok {
        Ok(())
    } else {
        Err(other_error("Version not supported"))
    }
}


pub fn read_blob<R: Read>(input: &mut R) -> RdbResult<Vec<u8>> {
    let (length, is_encoded) = read_length_with_encoding(input)?;

    if is_encoded {
        let result = match length {
            encoding::INT8 => helper::int_to_vec(input.read_i8()? as i32),
            encoding::INT16 => helper::int_to_vec(input.read_i16::<LittleEndian>()? as i32),
            encoding::INT32 => helper::int_to_vec(input.read_i32::<LittleEndian>()? as i32),
            encoding::LZF => {
                let compressed_length = read_length(input)?;
                let real_length = read_length(input)?;
                let data = read_exact(input, compressed_length as usize)?;
                lzf::decompress(&data, real_length as usize).unwrap()
            }
            _ => panic!("Unknown encoding: {}", length),
        };

        Ok(result)
    } else {
        read_exact(input, length as usize)
    }
}

// #[cfg(test)]
// fn test_parse(){
//     use super::*;

//     #[cfg(test)]
//     fn test_read(){
//         read_length_with_encoding();
//         assert!(1,1);
//     }
// }
