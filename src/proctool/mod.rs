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
        DaemonKill(DaemonKillArgs),
        DaemonLogs(DaemonLogsArgs),
        DaemonStart(DaemonStartArgs),
        DaemonStatus(DaemonStatusArgs),
        Pause(PauseArgs),
        Resume(ResumeArgs),
    }

    #[derive(clap::Args, Debug, Serialize, Deserialize)]
    pub struct DaemonKillArgs {}

    #[derive(clap::Args, Debug, Serialize, Deserialize)]
    pub struct DaemonLogsArgs {}

    #[derive(clap::Args, Debug, Serialize, Deserialize)]
    pub struct DaemonStartArgs {}

    #[derive(clap::Args, Debug, Serialize, Deserialize)]
    pub struct DaemonStatusArgs {}

    #[derive(clap::Args, Debug, Serialize, Deserialize)]
    pub struct PauseArgs {
        pub pid: i32,
    }

    #[derive(clap::Args, Debug, Serialize, Deserialize)]
    pub struct ResumeArgs {
        pub pid: i32,
    }
}
