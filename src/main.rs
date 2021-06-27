use argh::FromArgs;
use std::{path::PathBuf, thread};

use console::style;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

mod cmd;
use cmd::SshCmd;

mod ls;

mod spinners;

#[derive(FromArgs, Debug)]
/// Fuse options
struct FuseOption {
    /// ssh user
    #[argh(option)]
    pub user: String,

    /// ssh target host
    #[argh(option)]
    pub target: String,

    /// ssh options
    #[argh(option)]
    pub options: Option<String>,

    /// mount path
    #[argh(option)]
    pub dir: Option<PathBuf>,
}

fn cmd_view(cmd_runner: SshCmd, pb: ProgressBar, cmd: String) {
    pb.set_style(
        ProgressStyle::default_bar()
            .tick_strings(spinners::random())
            .template("{elapsed_precise:.bold.dim} {msg} {spinner:.bold.bright}"),
    );

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

fn main() {
    let args = argh::from_env::<FuseOption>();
    println!("{:?}", args);

    let user = args.user;
    let target = args.target;
    let options = args.options.unwrap_or_default();

    let cmd_runner_a = SshCmd::new(&user, &target, &options);
    let cmd_runner_b = cmd_runner_a.clone();
    let cmd_runner_c = cmd_runner_a.clone();

    let m = MultiProgress::new();
    let pb1 = m.add(ProgressBar::new(100));
    let pb2 = m.add(ProgressBar::new(100));
    let pb3 = m.add(ProgressBar::new(100));

    let cmd1 = thread::spawn(move || cmd_view(cmd_runner_a, pb1, "ls -l /".into()));
    let cmd2 =
        thread::spawn(move || cmd_view(cmd_runner_b, pb2, format!("ls -l /home/{}", user.clone())));
    let cmd3 =
        thread::spawn(move || cmd_view(cmd_runner_c, pb3, "ls -l /boohoo/does/not/exist".into()));

    cmd1.join().unwrap();
    cmd2.join().unwrap();
    cmd3.join().unwrap();
}
