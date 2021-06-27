use argh::FromArgs;
use std::{path::PathBuf, thread, time::Duration};

use console::style;
use indicatif::{ProgressBar, ProgressStyle};

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

fn main() {
    let args = argh::from_env::<FuseOption>();
    println!("{:#?}", args);

    let pb = ProgressBar::new(100);
    pb.set_style(
        ProgressStyle::default_bar()
            .tick_strings(spinners::random())
            .template("{elapsed_precise:.bold.dim}  {spinner:.bold.bright} {msg}"),
    );
    for i in 0..100 {
        thread::sleep(Duration::from_millis(70));
        pb.tick();
        pb.println(format!("[+] finished #{}", i));
        pb.set_message(format!("mooo cow {}", i));
    }
    // pb.reset();
    // ends iwth a
    // pb.finish_with_message("done");
    pb.println(format!("{}", style("Error").red()));
    pb.reset();

    let output = SshCmd::new(
        &args.user,
        &args.target,
        &args.options.unwrap_or_default(),
        "ls -l /",
    )
    .get_output();
    let out = output.expect("output");
    println!("{:#?}", out);
}
