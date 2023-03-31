use anyhow::bail;

use simplelog::Config as LogConfig;
use std::{env, path::Path};

use log::*;

use crate::conf::Config;
use crate::fetch::init_rss_client;
use crate::runtime::BotNetworkRuntime;
use simplelog::*;
mod api;
mod conf;
mod db;
mod fetch;
mod rss_event;
mod runtime;
mod runtime_event;
mod utils;
mod ws;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    CombinedLogger::init(vec![
        TermLogger::new(
            LevelFilter::Debug,
            LogConfig::default(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        ),
        WriteLogger::new(
            LevelFilter::Info,
            LogConfig::default(),
            std::fs::File::create("bot.log").unwrap(),
        ),
    ])
    .unwrap();

    let conf = parse_conf()?;

    init_rss_client(None);

    let mut ksbot_runtime = rss_event::KsbotRuntime::new();
    let mut network_runtime = BotNetworkRuntime::init(conf.bot_conf(), &mut ksbot_runtime).await;
    info!("ksbot starting ...");

    tokio::select! {
        result = network_runtime.run() => {
            match  result {
                Ok(_) => {}
                Err(e) => bail!("意外退出：{}", e),
            }
        },
    }

    Ok(())
}

fn parse_conf() -> Result<Config, anyhow::Error> {
    let conf_path = {
        let args = env::args();

        if args.len() <= 1 {
            bail!("must have config file path")
        }

        match args.last() {
            Some(t) => t,
            None => bail!("must have config file path"),
        }
    };

    Config::try_from(Path::new(&conf_path))
}
