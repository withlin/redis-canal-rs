#[doc(hidden)]
pub use types::{/* error and result types */ RdbError, RdbOk, RdbResult, Type, ZiplistEntry};

extern crate regex;
extern crate serde_json  as serialize;
extern crate lzf;

pub mod constants;
pub mod parser;
pub mod types;
pub mod filter;
pub mod formatter;
mod helper;

use formatter::Formatter;
use constants::*;
use filter::*;
use parser::RdbParser;
use std::io::Read;

pub fn parse<T: Read, F: Formatter, L: Filter>(input: T, formatter: F, filter: L) -> RdbOk {
    let mut parser = RdbParser::new(input, formatter, filter);
    parser.parse()
}
