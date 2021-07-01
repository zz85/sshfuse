use std::process::Output;

use crate::spinners;
use crate::{
    cmd::{CmdRunner, SshCmd},
    ls::FileMeta,
};
use console::style;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

pub struct RunnerWithSpinner {
    cmd: SshCmd,
    views: MultiProgress,
}

impl RunnerWithSpinner {
    pub fn new(user: &str, target: &str, options: &str) -> Self {
        let views = MultiProgress::new();
        // let trace_bar = get_progress_bar(&views);

        Self {
            views,
            cmd: SshCmd::new(&user, &target, &options),
        }
    }
}

impl CmdRunner for RunnerWithSpinner {
    // use overly generalized view for now
    fn fetch_path(&self, path: &str) -> Option<Vec<FileMeta>> {
        let pb = get_progress_bar(&self.views);
        let cmd_fmt = style(path).dim().bold();
        pb.set_message(format!("Fetching path {}...", cmd_fmt));
        pb.enable_steady_tick(75);

        let o = self.cmd.fetch_path(path);
        pb.finish_with_message(format!("Done: {}", &cmd_fmt));
        o
    }

    fn fetch_file(&self, path: &str) -> Output {
        let pb = get_progress_bar(&self.views);
        let cmd_fmt = style(path).dim().bold();
        pb.set_message(format!("Fetching file {}...", cmd_fmt));
        pb.enable_steady_tick(75);

        let o = self.cmd.fetch_file(path);
        pb.finish_with_message(format!("Done: {}", &cmd_fmt));
        o
    }
}

pub fn get_progress_bar(m: &MultiProgress) -> ProgressBar {
    let pb = m.add(ProgressBar::new(100));

    pb.set_style(
        ProgressStyle::default_bar()
            .tick_strings(spinners::random())
            .template("{elapsed_precise:.bold.dim} {msg} {spinner:.bold.bright}"),
    );

    pb
}

pub fn cmd_view(cmd_runner: &SshCmd, pb: ProgressBar, cmd: String) -> Output {
    let cmd_fmt = style(cmd.clone()).dim().bold();
    pb.set_message(format!("Running ssh command {}...", cmd_fmt));
    pb.enable_steady_tick(75);

    let output = cmd_runner.get_output(&cmd);
    let out = output.expect("output");

    // let std_out = String::from_utf8_lossy(&out.stdout);

    if out.stderr.len() > 0 {
        let err_msg = String::from_utf8_lossy(&out.stderr);

        pb.println(format!("{}...\n{}", cmd_fmt, style(err_msg).red()));
    }

    // pb2.println(format!("[+] finished {}", cmd_fmt));
    // pb2.reset(); // if clearing is needed

    pb.finish_with_message(format!("Done: {}", &cmd_fmt));

    out
}
