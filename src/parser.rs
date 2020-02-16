use byteorder::ByteOrder;
use byteorder::{BigEndian, LittleEndian, ReadBytesExt};
use lzf;
use std::io::Error as IoError;
use std::io::ErrorKind as IoErrorKind;
use std::io::{Cursor, Read};
use std::{f64, str};

use crate::filter::Filter;
use crate::formatter::Formatter;
use crate::helper;
use helper::read_exact;

#[doc(hidden)]
use crate::constants::{constant, encoding, encoding_type, module, op_code, version};

#[doc(hidden)]
pub use crate::types::{
    EncodingType, /* error and result types */
    RdbError, RdbOk, RdbResult, Type, ZiplistEntry,
};

pub struct RdbParser<R: Read, F: Formatter, L: Filter> {
    input: R,
    formatter: F,
    filter: L,
    last_expiretime: Option<u64>,
}

struct StreamGroupPendingEntry {
    id: [u8; 16],
    d_time: u64,
    d_count: u64,
}

impl Clone for StreamGroupPendingEntry {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            d_time: self.d_time,
            d_count: self.d_count,
        }
    }
}

struct ConsumerGroup {
    name: Vec<u8>,
    last_entry_id: String,
    pending: Vec<StreamGroupPendingEntry>,
    consumers: Vec<StreamConsumer>,
}

struct StreamConsumer {
    name: Vec<u8>,
    seen_time: u64,
    pending: Vec<Vec<u8>>,
}

#[inline]
fn other_error(desc: &'static str) -> IoError {
    IoError::new(IoErrorKind::Other, desc)
}

// fn read_length_with_encoding(&)
pub fn read_length_with_encoding<T: Read>(input: &mut T) -> Result<(u64, bool), IoError> {
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

fn read_ziplist_metadata<R: Read>(input: &mut R) -> RdbResult<(u32, u32, u16)> {
    let zlbytes = input.read_u32::<LittleEndian>()?;
    let zltail = input.read_u32::<LittleEndian>()?;
    let zllen = input.read_u16::<LittleEndian>()?;

    Ok((zlbytes, zltail, zllen))
}

impl<R: Read, F: Formatter, L: Filter> RdbParser<R, F, L> {
    pub fn new(input: R, formatter: F, filter: L) -> RdbParser<R, F, L> {
        RdbParser {
            input: input,
            formatter: formatter,
            filter: filter,
            last_expiretime: None,
        }
    }

    pub fn parse(&mut self) -> RdbOk {
        verify_magic(&mut self.input)?;
        verify_version(&mut self.input)?;

        self.formatter.start_rdb();

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
                            self.read_type(&key, next_op)?
                        } else {
                            self.skip_object(next_op)?
                        }
                    } else {
                        self.skip_key_and_object(next_op)?
                    }

                    self.last_expiretime = None;
                }
            }
        }

        Ok(())
    }

    fn read_linked_list(&mut self, key: &[u8], typ: Type) -> RdbOk {
        let mut len = read_length(&mut self.input)?;

        match typ {
            Type::List => {
                self.formatter
                    .start_list(key, len, self.last_expiretime, EncodingType::LinkedList);
            }
            Type::Set => {
                self.formatter
                    .start_set(key, len, self.last_expiretime, EncodingType::LinkedList);
            }
            _ => panic!("Unknown encoding type for linked list"),
        }

        while len > 0 {
            let blob = read_blob(&mut self.input)?;
            self.formatter.list_element(key, &blob);
            len -= 1;
        }

        match typ {
            Type::List => self.formatter.end_list(key),
            Type::Set => self.formatter.end_set(key),
            _ => panic!("Unknown encoding type for linked list"),
        }

        Ok(())
    }

    //     $zset
    // First, the size of the sorted set size is read from the stream using $length
    // Step 2, size strings are read from the stream using $string
    // Step 3, read 1 byte as length, and then read the length bytes as string. then converted this string to double.
    // The list is then re-constructed using step 2 and step 3.

    // $zset2
    // First, the size of the sorted set size is read from the stream using $length
    // Step 2, size strings are read from the stream using $string
    // Step 3, read 8 byte as double.
    // The list is then re-constructed using step 2 and step 3.

    fn read_sorted_set(&mut self, key: &[u8], entyp: EncodingType) -> RdbOk {
        let mut set_items = read_length(&mut self.input)?;

        self.formatter.start_sorted_set(
            key,
            set_items,
            self.last_expiretime,
            EncodingType::Hashtable,
        );

        while set_items > 0 {
            match entyp {
                EncodingType::ZSET => {
                    let val = read_blob(&mut self.input)?;
                    let score_length = self.input.read_u8()?;
                    let score = match score_length {
                        253 => f64::NAN,
                        254 => f64::INFINITY,
                        255 => f64::NEG_INFINITY,
                        _ => {
                            let tmp = read_exact(&mut self.input, score_length as usize)?;
                            unsafe { str::from_utf8_unchecked(&tmp) }
                                .parse::<f64>()
                                .unwrap()
                        }
                    };
                    self.formatter.sorted_set_element(key, score, &val);

                    set_items -= 1;
                }
                EncodingType::ZSET2 => {
                    let val = read_blob(&mut self.input)?;
                    let mut buf = [0; 8];
                    self.input.read(&mut buf)?;
                    let score = LittleEndian::read_f64(&buf);

                    self.formatter.sorted_set_element(key, score, &val);

                    set_items -= 1;
                }
                _ => (), //noting to do
            };
        }

        self.formatter.end_sorted_set(key);

        Ok(())
    }

    fn read_hash(&mut self, key: &[u8]) -> RdbOk {
        let mut hash_items = read_length(&mut self.input)?;

        self.formatter.start_hash(
            key,
            hash_items,
            self.last_expiretime,
            EncodingType::Hashtable,
        );

        while hash_items > 0 {
            let field = read_blob(&mut self.input)?;
            let val = read_blob(&mut self.input)?;

            self.formatter.hash_element(key, &field, &val);

            hash_items -= 1;
        }

        self.formatter.end_hash(key);

        Ok(())
    }

    fn read_ziplist_entry<T: Read>(&mut self, ziplist: &mut T) -> RdbResult<ZiplistEntry> {
        // 1. 1 or 5 bytes length of previous entry
        let byte = ziplist.read_u8()?;
        if byte == 254 {
            let mut bytes = [0; 4];
            match ziplist.read(&mut bytes) {
                Ok(4) => (),
                Ok(_) => {
                    return Err(other_error(
                        "Could not read 4 bytes to skip after ziplist length",
                    ))
                }
                Err(e) => return Err(e),
            };
        }

        let length: u64;
        let number_value: i64;

        // 2. Read flag or number value
        let flag = ziplist.read_u8()?;

        match (flag & 0xC0) >> 6 {
            0 => length = (flag & 0x3F) as u64,
            1 => {
                let next_byte = ziplist.read_u8()?;
                length = (((flag & 0x3F) as u64) << 8) | next_byte as u64;
            }
            2 => {
                length = ziplist.read_u32::<BigEndian>()? as u64;
            }
            _ => {
                match (flag & 0xF0) >> 4 {
                    0xC => number_value = ziplist.read_i16::<LittleEndian>()? as i64,
                    0xD => number_value = ziplist.read_i32::<LittleEndian>()? as i64,
                    0xE => number_value = ziplist.read_i64::<LittleEndian>()? as i64,
                    0xF => match flag & 0xF {
                        0 => {
                            let mut bytes = [0; 3];
                            match ziplist.read(&mut bytes) {
                                Ok(3) => (),
                                Ok(_) => {
                                    return Err(other_error(
                                        "Could not read enough bytes for 24bit number",
                                    ))
                                }
                                Err(e) => return Err(e),
                            };

                            let number: i32 = (((bytes[2] as i32) << 24)
                                ^ ((bytes[1] as i32) << 16)
                                ^ ((bytes[0] as i32) << 8)
                                ^ 48)
                                >> 8;

                            number_value = number as i64;
                        }
                        0xE => {
                            number_value = ziplist.read_i8()? as i64;
                        }
                        _ => {
                            number_value = (flag & 0xF) as i64 - 1;
                        }
                    },
                    _ => {
                        panic!("Flag not handled: {}", flag);
                    }
                }

                return Ok(ZiplistEntry::Number(number_value));
            }
        }

        // 3. Read value
        let rawval = read_exact(ziplist, length as usize)?;
        Ok(ZiplistEntry::String(rawval))
    }

    fn read_ziplist_entry_string<T: Read>(&mut self, reader: &mut T) -> RdbResult<Vec<u8>> {
        let entry = self.read_ziplist_entry(reader)?;
        match entry {
            ZiplistEntry::String(val) => Ok(val),
            ZiplistEntry::Number(val) => Ok(val.to_string().into_bytes()),
        }
    }

    fn read_list_ziplist(&mut self, key: &[u8]) -> RdbOk {
        let ziplist = read_blob(&mut self.input)?;
        let raw_length = ziplist.len() as u64;

        let mut reader = Cursor::new(ziplist);
        let (_zlbytes, _zltail, zllen) = read_ziplist_metadata(&mut reader)?;

        self.formatter.start_list(
            key,
            zllen as u64,
            self.last_expiretime,
            EncodingType::Ziplist(raw_length),
        );

        for _ in 0..zllen {
            let entry = self.read_ziplist_entry_string(&mut reader)?;
            self.formatter.list_element(key, &entry);
        }

        let last_byte = reader.read_u8()?;
        if last_byte != 0xFF {
            return Err(other_error("Invalid end byte of ziplist"));
        }

        self.formatter.end_list(key);

        Ok(())
    }

    fn read_hash_ziplist(&mut self, key: &[u8]) -> RdbOk {
        let ziplist = read_blob(&mut self.input)?;
        let raw_length = ziplist.len() as u64;

        let mut reader = Cursor::new(ziplist);
        let (_zlbytes, _zltail, zllen) = read_ziplist_metadata(&mut reader)?;

        assert!(zllen % 2 == 0);
        let zllen = zllen / 2;

        self.formatter.start_hash(
            key,
            zllen as u64,
            self.last_expiretime,
            EncodingType::Ziplist(raw_length),
        );

        for _ in 0..zllen {
            let field = self.read_ziplist_entry_string(&mut reader)?;
            let value = self.read_ziplist_entry_string(&mut reader)?;
            self.formatter.hash_element(key, &field, &value);
        }

        let last_byte = reader.read_u8()?;
        if last_byte != 0xFF {
            return Err(other_error("Invalid end byte of ziplist"));
        }

        self.formatter.end_hash(key);

        Ok(())
    }

    fn read_sortedset_ziplist(&mut self, key: &[u8]) -> RdbOk {
        let ziplist = read_blob(&mut self.input)?;
        let raw_length = ziplist.len() as u64;

        let mut reader = Cursor::new(ziplist);
        let (_zlbytes, _zltail, zllen) = read_ziplist_metadata(&mut reader)?;

        self.formatter.start_sorted_set(
            key,
            zllen as u64,
            self.last_expiretime,
            EncodingType::Ziplist(raw_length),
        );

        assert!(zllen % 2 == 0);
        let zllen = zllen / 2;

        for _ in 0..zllen {
            let entry = self.read_ziplist_entry_string(&mut reader)?;
            let score = self.read_ziplist_entry_string(&mut reader)?;
            let score = str::from_utf8(&score).unwrap().parse::<f64>().unwrap();
            self.formatter.sorted_set_element(key, score, &entry);
        }

        let last_byte = reader.read_u8()?;
        if last_byte != 0xFF {
            return Err(other_error("Invalid end byte of ziplist"));
        }

        self.formatter.end_sorted_set(key);

        Ok(())
    }

    fn read_quicklist_ziplist(&mut self, key: &[u8]) -> RdbOk {
        let ziplist = read_blob(&mut self.input)?;

        let mut reader = Cursor::new(ziplist);
        let (_zlbytes, _zltail, zllen) = read_ziplist_metadata(&mut reader)?;

        for _ in 0..zllen {
            let entry = self.read_ziplist_entry_string(&mut reader)?;
            self.formatter.list_element(key, &entry);
        }

        let last_byte = reader.read_u8()?;
        if last_byte != 0xFF {
            return Err(other_error("Invalid end byte of ziplist (quicklist)"));
        }

        Ok(())
    }

    fn read_zipmap_entry<T: Read>(&mut self, next_byte: u8, zipmap: &mut T) -> RdbResult<Vec<u8>> {
        let elem_len;
        match next_byte {
            253 => elem_len = zipmap.read_u32::<LittleEndian>().unwrap(),
            254 | 255 => panic!("Invalid length value in zipmap: {}", next_byte),
            _ => elem_len = next_byte as u32,
        }

        read_exact(zipmap, elem_len as usize)
    }

    fn read_hash_zipmap(&mut self, key: &[u8]) -> RdbOk {
        let zipmap = read_blob(&mut self.input)?;
        let raw_length = zipmap.len() as u64;

        let mut reader = Cursor::new(zipmap);

        let zmlen = reader.read_u8()?;

        let mut length: i32;
        let size;
        if zmlen <= 254 {
            length = zmlen as i32;
            size = zmlen
        } else {
            length = -1;
            size = 0;
        }

        self.formatter.start_hash(
            key,
            size as u64,
            self.last_expiretime,
            EncodingType::Zipmap(raw_length),
        );

        loop {
            let next_byte = reader.read_u8()?;

            if next_byte == 0xFF {
                break; // End of list.
            }

            let field = self.read_zipmap_entry(next_byte, &mut reader)?;

            let next_byte = reader.read_u8()?;
            let _free = reader.read_u8()?;
            let value = self.read_zipmap_entry(next_byte, &mut reader)?;

            self.formatter.hash_element(key, &field, &value);

            if length > 0 {
                length -= 1;
            }

            if length == 0 {
                let last_byte = reader.read_u8()?;

                if last_byte != 0xFF {
                    return Err(other_error("Invalid end byte of zipmap"));
                }
                break;
            }
        }

        self.formatter.end_hash(key);

        Ok(())
    }

    fn read_set_intset(&mut self, key: &[u8]) -> RdbOk {
        let intset = read_blob(&mut self.input)?;
        let raw_length = intset.len() as u64;

        let mut reader = Cursor::new(intset);
        let byte_size = reader.read_u32::<LittleEndian>()?;
        let intset_length = reader.read_u32::<LittleEndian>()? as u64;

        self.formatter.start_set(
            key,
            intset_length,
            self.last_expiretime,
            EncodingType::Intset(raw_length),
        );

        for _ in 0..intset_length {
            let val = match byte_size {
                2 => reader.read_i16::<LittleEndian>()? as i64,
                4 => reader.read_i32::<LittleEndian>()? as i64,
                8 => reader.read_i64::<LittleEndian>()?,
                _ => panic!("unhandled byte size in intset: {}", byte_size),
            };

            self.formatter.set_element(key, val.to_string().as_bytes());
        }

        self.formatter.end_set(key);

        Ok(())
    }

    fn read_quicklist(&mut self, key: &[u8]) -> RdbOk {
        let len = read_length(&mut self.input)?;

        self.formatter
            .start_set(key, 0, self.last_expiretime, EncodingType::Quicklist);
        for _ in 0..len {
            self.read_quicklist_ziplist(key)?;
        }
        self.formatter.end_set(key);

        Ok(())
    }

    fn read_type(&mut self, key: &[u8], value_type: u8) -> RdbOk {
        match value_type {
            encoding_type::STRING => {
                let val = read_blob(&mut self.input)?;
                self.formatter.set(key, &val, self.last_expiretime);
            }
            encoding_type::LIST => self.read_linked_list(key, Type::List)?,
            encoding_type::SET => self.read_linked_list(key, Type::Set)?,
            encoding_type::ZSET => self.read_sorted_set(key, EncodingType::ZSET)?,
            encoding_type::HASH => self.read_hash(key)?,
            encoding_type::ZSET2 => self.read_sorted_set(key, EncodingType::ZSET2)?,
            encoding_type::MODULE => (), //not support it
            encoding_type::MODULE2 => self.read_moudle()?,
            encoding_type::HASH_ZIPMAP => self.read_hash_zipmap(key)?,
            encoding_type::LIST_ZIPLIST => self.read_list_ziplist(key)?,
            encoding_type::SET_INTSET => self.read_set_intset(key)?,
            encoding_type::ZSET_ZIPLIST => self.read_sortedset_ziplist(key)?,
            encoding_type::HASH_ZIPLIST => self.read_hash_ziplist(key)?,
            encoding_type::LIST_QUICKLIST => self.read_quicklist(key)?,
            encoding_type::STEAMLISTPACKS => self.skip_steam()?,

            _ => panic!("Value Type not implemented: {}", value_type),
        };

        Ok(())
    }

    fn skip_steam(&mut self) -> RdbOk {
        let mut listpacks = read_length(&mut self.input)?;
        while listpacks > 0 {
            read_blob(&mut self.input)?;
            read_blob(&mut self.input)?;
            listpacks -= 1;
        }
        read_length(&mut self.input)?;
        let entry1 = read_length(&mut self.input)?;
        let entry2 = read_length(&mut self.input)?;
        let _ = format!("{}-{}", entry1, entry2);
        let mut cgroups = read_length(&mut self.input)?;
        let entrys: &mut Vec<StreamGroupPendingEntry> = &mut Vec::new();
        let mut consumer_groups: Vec<ConsumerGroup> = Vec::new();
        while cgroups > 0 {
            let cgname = read_blob(&mut self.input)?;
            let entry1 = read_length(&mut self.input)?;
            let entry2 = read_length(&mut self.input)?;
            let laste_cg_entry_id = format!("{}-{}", entry1, entry2);
            let pending = read_length(&mut self.input)?;
            for _ in 0..pending {
                let mut buf = [0; 16];
                self.input.read(&mut buf)?;
                let mut dtime_buf = [0; 8];
                self.input.read(&mut dtime_buf)?;
                let dtime = LittleEndian::read_u64(&dtime_buf);
                let dcount = read_length(&mut self.input)?;
                entrys.push(StreamGroupPendingEntry {
                    d_count: dcount,
                    d_time: dtime,
                    id: buf,
                });
            }
            let consumers = read_length(&mut self.input)?;
            let mut centrys: Vec<StreamConsumer> = Vec::new();
            for _ in 0..consumers {
                let cname = read_blob(&mut self.input)?;
                let mut buf = [0; 8];
                self.input.read(&mut buf)?;
                let stime = LittleEndian::read_u64(&buf);
                let cpending = read_length(&mut self.input)?;
                let mut pending_ids: Vec<Vec<u8>> = Vec::new();
                for _ in 0..cpending {
                    let mut eid = vec![0; 16];
                    self.input.read(&mut eid)?;
                    pending_ids.push(eid);
                }
                centrys.push(StreamConsumer {
                    name: cname,
                    seen_time: stime,
                    pending: pending_ids,
                });
            }

            consumer_groups.push(ConsumerGroup {
                name: cgname,
                pending: entrys.to_vec(),
                consumers: centrys,
                last_entry_id: laste_cg_entry_id,
            });
            cgroups -= 1;
        }
        Ok(())
    }

    fn read_moudle(&mut self) -> RdbOk {
        read_length(&mut self.input)?;
        self.skip_moudle()?;
        Ok(())
    }

    fn skip_moudle(&mut self) -> RdbOk {
        let mut optcode = read_length(&mut self.input)?;
        while optcode != module::ModuleOpcodeEOF as u64 {
            match optcode {
                optcode if optcode == module::ModuleOpcodeSInt as u64 => {
                    read_length(&mut self.input)?;
                }
                optcode if optcode == module::ModuleOpcodeUInt as u64 => {
                    read_length(&mut self.input)?;
                }
                optcode if optcode == module::ModuleOpcodeFloat as u64 => {
                    read_length(&mut self.input)?;
                }
                optcode if optcode == module::ModuleOpcodeString as u64 => {
                    read_blob(&mut self.input)?;
                }
                optcode if optcode == module::ModuleOpcodeDouble as u64 => {
                    read_length(&mut self.input)?;
                }
                _ => println!("Unknown module opcode: {}", optcode),
            }
            optcode = read_length(&mut self.input)?;
        }

        Ok(())
    }

    fn skip(&mut self, skip_bytes: usize) -> RdbResult<()> {
        let mut buf = vec![0; skip_bytes];
        self.input.read_exact(&mut buf)
    }

    fn skip_blob(&mut self) -> RdbResult<()> {
        let (len, is_encoded) = read_length_with_encoding(&mut self.input)?;
        let skip_bytes;

        if is_encoded {
            skip_bytes = match len {
                encoding::INT8 => 1,
                encoding::INT16 => 2,
                encoding::INT32 => 4,
                encoding::LZF => {
                    let compressed_length = read_length(&mut self.input)?;
                    let _real_length = read_length(&mut self.input)?;
                    compressed_length
                }
                _ => panic!("Unknown encoding: {}", len),
            }
        } else {
            skip_bytes = len;
        }

        self.skip(skip_bytes as usize)
    }

    fn skip_object(&mut self, enc_type: u8) -> RdbResult<()> {
        let blobs_to_skip = match enc_type {
            encoding_type::STRING
            | encoding_type::HASH_ZIPMAP
            | encoding_type::LIST_ZIPLIST
            | encoding_type::SET_INTSET
            | encoding_type::ZSET_ZIPLIST
            | encoding_type::HASH_ZIPLIST => 1,
            encoding_type::LIST | encoding_type::SET | encoding_type::LIST_QUICKLIST => {
                read_length(&mut self.input)?
            }
            encoding_type::ZSET | encoding_type::HASH => read_length(&mut self.input)? * 2,
            _ => panic!("Unknown encoding type: {}", enc_type),
        };

        for _ in 0..blobs_to_skip {
            self.skip_blob()?
        }

        Ok(())
    }

    fn skip_key_and_object(&mut self, enc_type: u8) -> RdbResult<()> {
        self.skip_blob()?;
        self.skip_object(enc_type)?;
        Ok(())
    }
}
