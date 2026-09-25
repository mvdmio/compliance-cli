use crate::failure::Failure;
use crate::output;

/// `compliance skill`: the Agent skill file, as Markdown rather than JSON, because an Agent reads it as a document.
pub fn run() -> Result<(), Failure> {
    output::print_stdout_raw(include_str!("../SKILL.md"));
    Ok(())
}
