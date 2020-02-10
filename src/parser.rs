use crate::constants::{constant, version};
use crate::types::{RdbOk, RdbResult};

use byteorder::{BigEndian, LittleEndian, ReadBytesExt};
use std::io::{Cursor, Error, ErrorKind, Read};
// use std::sync::Mutex;
// use redis_canal_rs::constants;

#[inline]
fn other_error(desc: &'static str) -> Error {
    Error::new(ErrorKind::Other, desc)
}

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

// #[cfg(test)]
// fn test_parse(){
//     use super::*;

//     #[cfg(test)]
//     fn test_read(){
//         read_length_with_encoding();
//         assert!(1,1);
//     }
// }
