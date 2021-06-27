use argh::FromArgs;
use std::{
    path::PathBuf,
    sync::mpsc::{self, TryRecvError},
    thread,
    time::Duration,
};

use console::style;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

mod cmd;
use cmd::SshCmd;

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
    let cmd_fmt2 = cmd_fmt.clone();
    pb.set_message(format!("Running ssh command {}...", cmd_fmt));

    let (tx, rx) = mpsc::channel();

    let pb2 = pb.clone();
    let spinner_thread = thread::spawn(move || {
        loop {
            match rx.try_recv() {
                Ok(()) => {
                    break;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    break;
                }
            }
            thread::sleep(Duration::from_millis(70));
            pb2.inc(1);
        }

        // pb2.println(format!("[+] finished {}", cmd_fmt));
        // pb2.reset(); // if clearing is needed
        pb2.finish_with_message(format!("Competed {}", &cmd_fmt));
    });

    let output = cmd_runner.get_output(&cmd);
    let out = output.expect("output");

    if !out.stderr.is_empty() {
        let err_msg = String::from_utf8_lossy(&out.stderr);
        pb.println(format!("Error: {}", style(err_msg).red()));
    }

    let std_out = String::from_utf8_lossy(&out.stdout);

    tx.send(()).expect("inform spinner");

    spinner_thread.join().unwrap();

    pb.println(format!("{}", cmd_fmt2));
    pb.println(std_out);
}

fn main() {
    let args = argh::from_env::<FuseOption>();
    println!("{:?}", args);

    let user = args.user;
    let target = args.target;
    let options = args.options.unwrap_or_default();

    let cmd_runner_a = SshCmd::new(&user, &target, &options);
    let cmd_runner_b = SshCmd::new(&user, &target, &options);

    let m = MultiProgress::new();
    let pb1 = m.add(ProgressBar::new(100));
    let pb2 = m.add(ProgressBar::new(100));

    let cmd1 = thread::spawn(move || cmd_view(cmd_runner_a, pb1, "ls -l /".into()));
    let cmd2 =
        thread::spawn(move || cmd_view(cmd_runner_b, pb2, format!("ls -l /home/{}", user.clone())));

    cmd1.join().unwrap();
    cmd2.join().unwrap();
}
