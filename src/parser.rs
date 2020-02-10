use crate::constants::constant;
use crate::types::RdbOk;

use byteorder::{BigEndian, LittleEndian, ReadBytesExt};
use std::io::{Cursor, Error, ErrorKind, Read};
// use std::sync::Mutex;
// use redis_canal_rs::constants;

pub struct RdbParser<T: Read> {
    // input : Option<Mutex<Box<T>>>,
    input: T,
}

impl<T: Read> RdbParser<T> {
    // pub fn new(input:T)->Self{
    //     RdbParser{input:Some(Mutex::new(Box::new(input)))}
    // }
    pub fn new(input: T) -> Self {
        RdbParser { input: input }
    }

    // pub fn prase() -> RdbOk {}
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

// #[cfg(test)]
// fn test_parse(){
//     use super::*;

//     #[cfg(test)]
//     fn test_read(){
//         read_length_with_encoding();
//         assert!(1,1);
//     }
// }
