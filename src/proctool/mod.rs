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
        Pause(PauseArgs),
        Resume(ResumeArgs),
        Takeover(TakeoverArgs),
    }

    #[derive(clap::Args, Debug, Serialize, Deserialize)]
    pub struct PauseArgs {
        pub pid: i32,
    }

    #[derive(clap::Args, Debug, Serialize, Deserialize)]
    pub struct ResumeArgs {
        pub pid: i32,
    }

    #[derive(clap::Args, Debug, Serialize, Deserialize)]
    pub struct TakeoverArgs {
        pub pid: i32,
        /// pause the program after taking it over, to inspect program state
        #[arg(long)]
        pub pause: bool,
    }
}
