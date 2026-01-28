pub mod scan;

use crate::error::Result;

pub trait Command {
    fn execute(&self) -> Result<()>;
}
