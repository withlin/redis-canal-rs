#[doc(hidden)]
pub use types::{/* error and result types */ RdbError, RdbOk, RdbResult, Type, ZiplistEntry};

pub mod constants;
pub mod parser;
pub mod types;

use constants::constant;
