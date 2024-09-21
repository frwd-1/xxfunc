use eyre::{eyre, Result};
use std::process::Command;

pub struct ModuleRunner;

impl ModuleRunner {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    pub fn execute(&self, binary_path: &str, input: Vec<u8>) -> Result<()> {
        let input_string =
            String::from_utf8(input).map_err(|e| eyre!("Invalid UTF-8 input: {}", e))?;

        let status = Command::new(binary_path)
            .arg(input_string)
            .status()
            .map_err(|e| eyre!("Failed to execute binary: {}", e))?;

        if status.success() {
            Ok(())
        } else {
            Err(eyre!("Binary execution failed with status: {:?}", status))
        }
    }
}
