pub mod common {
    use clap::Parser;
    use serde::{Deserialize, Serialize};

    pub const PORT: u32 = 6666;

    #[derive(Parser, Debug, Serialize, Deserialize)]
    pub enum Args {
        DaemonLogs(DaemonLogsArgs),
        Pause(PauseArgs),
        Resume(ResumeArgs),
    }

    #[derive(clap::Args, Debug, Serialize, Deserialize)]
    pub struct DaemonLogsArgs {}

    #[derive(clap::Args, Debug, Serialize, Deserialize)]
    pub struct PauseArgs {
        pub pid: i32,
    }

    #[derive(clap::Args, Debug, Serialize, Deserialize)]
    pub struct ResumeArgs {
        pub pid: i32,
    }
}
