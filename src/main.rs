use argh::FromArgs;
use std::{path::PathBuf, thread};

use indicatif::MultiProgress;

mod cmd;
use cmd::SshCmd;
mod display;
use display::{cmd_view, get_progress_bar};
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
    let pb1 = get_progress_bar(&m);
    let pb2 = get_progress_bar(&m);
    let pb3 = get_progress_bar(&m);

    let cmd1 = thread::spawn(move || cmd_view(cmd_runner_a, pb1, "ls -l /".into()));
    let cmd2 =
        thread::spawn(move || cmd_view(cmd_runner_b, pb2, format!("ls -l /home/{}", user.clone())));
    let cmd3 =
        thread::spawn(move || cmd_view(cmd_runner_c, pb3, "ls -l /boohoo/does/not/exist".into()));

    cmd1.join().unwrap();
    cmd2.join().unwrap();
    cmd3.join().unwrap();
}
