//! Descriptor inheritance for the shared std-only kernel lock.
pub(crate) use crate::resources::lock::*;
use std::process::Command;
impl LongLock {
    pub(crate) fn inherit_to(&self, command: &mut Command) -> std::io::Result<()> {
        if let Some(file) = self.duplicate()? {
            inherit_file(command, file);
        }
        Ok(())
    }
}

pub(crate) fn inherit_file(command: &mut Command, duplicate: std::fs::File) {
    use command_fds::CommandFdExt;
    command.preserved_fds(vec![duplicate.into()]);
}
