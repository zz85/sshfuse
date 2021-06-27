use std::{
    io::ErrorKind,
    process::{Command, Output},
};

#[derive(Debug, Clone)]
pub struct SshCmd {
    user: String,
    target: String,
    options: String,
}

impl SshCmd {
    pub fn new(user: &str, target: &str, options: &str) -> Self {
        let user = user.into();
        let target = target.into();
        let options = options.into();

        Self {
            user,
            target,
            options,
        }
    }

    fn get_full_cmd(&self, cmd: &str) -> String {
        format!(
            "ssh {} {}@{} -- {}",
            self.options, self.user, self.target, cmd
        )
    }

    pub fn get_output(&self, cmd: &str) -> Result<Output, std::io::Error> {
        let raw_cmd = self.get_full_cmd(cmd);
        let mut parts = raw_cmd.split_whitespace();
        let cmd = parts
            .next()
            .ok_or(std::io::Error::new(ErrorKind::Other, "can't split cmd"))?;
        let args = parts.collect::<Vec<_>>();

        let process = Command::new(cmd).args(&args).output();

        process
    }
}
