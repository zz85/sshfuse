use crate::cmd::SshCmd;
use crate::spinners;
use console::style;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

pub fn get_progress_bar(m: &MultiProgress) -> ProgressBar {
    let pb = m.add(ProgressBar::new(100));

    pb.set_style(
        ProgressStyle::default_bar()
            .tick_strings(spinners::random())
            .template("{elapsed_precise:.bold.dim} {msg} {spinner:.bold.bright}"),
    );

    pb
}

pub fn cmd_view(cmd_runner: SshCmd, pb: ProgressBar, cmd: String) {
    let cmd_fmt = style(cmd.clone()).dim().bold();
    pb.set_message(format!("Running ssh command {}...", cmd_fmt));
    pb.enable_steady_tick(75);

    let output = cmd_runner.get_output(&cmd);
    let out = output.expect("output");

    let std_out = String::from_utf8_lossy(&out.stdout);
    let err_msg = String::from_utf8_lossy(&out.stderr);

    pb.println(format!(
        "{}\n{}{}\n",
        cmd_fmt,
        std_out,
        style(err_msg).red()
    ));

    // pb2.println(format!("[+] finished {}", cmd_fmt));
    // pb2.reset(); // if clearing is needed

    pb.finish_with_message(format!("Competed {}", &cmd_fmt));
}
