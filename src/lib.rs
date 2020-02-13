#[doc(hidden)]
pub use types::{/* error and result types */ RdbError, RdbOk, RdbResult, Type, ZiplistEntry};

extern crate lzf;
extern crate regex;
extern crate serde_json as serialize;

pub mod constants;
pub mod filter;
pub mod formatter;
mod helper;
pub mod parser;
pub mod types;

use constants::*;
use filter::*;
use formatter::Formatter;
use parser::RdbParser;
use std::io::Read;

pub fn parse<T: Read, F: Formatter, L: Filter>(input: T, formatter: F, filter: L) -> RdbOk {
    let mut parser = RdbParser::new(input, formatter, filter);
    parser.parse()
}
