pub mod common {
    use clap::Parser;
    use serde::{Deserialize, Serialize};

    pub const PORT: u32 = 6666;

    #[derive(Parser, Debug, Serialize, Deserialize)]
    pub struct Args {
        // TODO: real subcommand
        pub command: String,
        pub pid: i32,
    }
}
