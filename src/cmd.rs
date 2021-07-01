use std::{
    io::ErrorKind,
    process::{Command, Output},
    str,
};

use crate::ls::{parse_long_list, FileMeta};

pub trait CmdRunner: Send + Sync {
    fn fetch_path(&self, path: &str) -> Option<Vec<FileMeta>>;
    fn fetch_file(&self, path: &str) -> Output;
}

#[derive(Debug, Clone)]
pub struct SshCmd {
    user: String,
    target: String,
    options: String,
}

impl CmdRunner for SshCmd {
    fn fetch_path(&self, path: &str) -> Option<Vec<FileMeta>> {
        let path = if path.ends_with("/") {
            path.into()
        } else {
            format!("{}/", path)
        };

        let cmd = format!("ls -l {}", path);

        let output = self.get_output(&cmd).expect("output");

        if output.stderr.len() > 0 {
            println!("Error: {}", str::from_utf8(&output.stderr).unwrap());
        }
        let str = &output.stdout;
        let stdout_utf8 = str::from_utf8(&str).unwrap();
        println!("Out: {}", stdout_utf8);

        let dir = parse_long_list(stdout_utf8);

        Some(dir)
    }

    fn fetch_file(&self, path: &str) -> Output {
        // reads the file and poke it into a open file cache
        let cmd = format!("cat {}", path);

        let output = self.get_output(&cmd).expect("output");

        output
    }
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
