pub mod cryogenics;
pub mod pcontroller;
pub mod procinfo;
pub mod terminals;

pub mod common {
    use clap::Parser;
    use serde::{Deserialize, Serialize};

    pub const PORT: u32 = 6666;

    #[derive(Serialize, Deserialize)]
    #[serde(tag = "type")]
    pub enum DaemonMessage {
        Command(Args),
        Kill,
    }

    #[derive(Parser, Debug, Serialize, Deserialize)]
    pub enum Args {
        DaemonKill,
        DaemonLogs,
        DaemonRestart,
        DaemonStart,
        DaemonStatus,
        Freeze(FreezeArgs),
        Groups,
        Oblivion(OblivionArgs),
        Pause(PauseArgs),
        Processes(ProcessesArgs),
        Redirect(RedirectArgs),
        Resume(ResumeArgs),
        Rewind(RewindArgs),
        Rot13(Rot13Args),
        ColorizeStderr(ColorizeStderrArgs),
        Sessions,
        Spawn(SpawnArgs),
        Takeover(TakeoverArgs),
        Terminals,
        TerminalSizes,
        Thaw(ThawArgs),
        UnmapChild,
        Which,
        WriteStdin(WriteStdinArgs),
    }

    #[derive(clap::Args, Debug, Serialize, Deserialize)]
    pub struct FreezeArgs {
        pub pid: i32,
    }

    #[derive(clap::Args, Debug, Serialize, Deserialize)]
    pub struct OblivionArgs {
        pub ttys: Vec<i32>,
    }

    #[derive(clap::Args, Debug, Serialize, Deserialize)]
    pub struct PauseArgs {
        pub pid: i32,
    }

    #[derive(clap::Args, Debug, Serialize, Deserialize)]
    pub struct ProcessesArgs {
        pub pid: Option<i32>,
    }

    #[derive(clap::Args, Debug, Serialize, Deserialize)]
    pub struct RedirectArgs {
        pub pid: i32,
        #[arg(long)]
        pub tty: String,
    }

    #[derive(clap::Args, Debug, Serialize, Deserialize)]
    pub struct ResumeArgs {
        pub pid: i32,
    }

    #[derive(clap::Args, Debug, Serialize, Deserialize)]
    pub struct RewindArgs {
        pub pid: i32,
    }

    #[derive(clap::Args, Debug, Serialize, Deserialize)]
    pub struct Rot13Args {
        pub pid: i32,
    }

    #[derive(clap::Args, Debug, Serialize, Deserialize)]
    pub struct ColorizeStderrArgs {
        pub pid: i32,
    }

    #[derive(clap::Args, Debug, Serialize, Deserialize)]
    pub struct SpawnArgs {
        #[arg(long)]
        pub tty: String,
        #[arg(long)]
        pub cmd: String,
        #[arg(long)]
        pub uid: Option<u32>,
    }

    #[derive(clap::Args, Debug, Serialize, Deserialize)]
    pub struct TakeoverArgs {
        pub pid: i32,
        /// pause the program after taking it over, to inspect program state
        #[arg(long)]
        pub pause: bool,
        #[arg(long)]
        pub bin: Option<String>,
    }

    #[derive(clap::Args, Debug, Serialize, Deserialize)]
    pub struct ThawArgs {
        pub path: String,
    }

    #[derive(clap::Args, Debug, Serialize, Deserialize)]
    pub struct WriteStdinArgs {
        pub pid: i32,
        #[arg(long)]
        pub message: String,
    }
}
