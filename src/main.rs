use argh::FromArgs;
use std::path::PathBuf;

mod cmd;
use cmd::SshCmd;
mod display;
mod ls;
mod mount;
mod spinners;

use display::RunnerWithSpinner;

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

    /// display spinners
    #[argh(option)]
    pub spinner: Option<bool>,
}

fn main() {
    let args = argh::from_env::<FuseOption>();
    println!("{:?}", args);

    let user = args.user;
    let target = args.target;
    let options = args.options.unwrap_or_default();

    let cmd_runner = SshCmd::new(&user, &target, &options);
    let spinner_runner = RunnerWithSpinner::new(&user, &target, &options);

    if args.spinner.unwrap_or(true) {
        mount::mount(spinner_runner)
    } else {
        mount::mount(cmd_runner)
    }
}
