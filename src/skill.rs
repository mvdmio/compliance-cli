use crate::failure::Failure;
use crate::output;

pub fn run() -> Result<(), Failure> {
    output::print_stdout_raw(include_str!("../SKILL.md"));
    Ok(())
}
