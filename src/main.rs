use anyhow::bail;

use simplelog::Config as LogConfig;
use std::path::PathBuf;
use structopt::StructOpt;

use log::*;
use simplelog::*;

use crate::conf::Config;
use crate::fetch::init_rss_client;
use crate::runtime::BotNetworkRuntime;

mod api;
mod conf;
mod db;
mod fetch;
mod rss_event;
mod runtime;
mod runtime_event;
mod utils;
mod ws;

#[derive(Debug, StructOpt)]
#[structopt(name = "ksbot", about = "A simple Kook RSS bot.")]
struct Args {
    #[structopt(short, long)]
    pub token: Option<String>,

    #[structopt(parse(from_os_str))]
    pub conf_path: Option<PathBuf>,
}

#[tokio::main]
async fn main() {
    match main1().await {
        Ok(()) => unreachable!(),
        Err(e) => error!("程序错误：{}", e),
    }
}

async fn main1() -> Result<(), anyhow::Error> {
    init_log();

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
    let args = Args::from_args();

    match (&args.token, &args.conf_path) {
        // 优先以配置为准
        (Some(_), Some(path)) | (None, Some(path)) => Ok(Config::try_from(path.as_path())?),

        (Some(token), None) => {
            let mut c = Config::default();
            c.token = token.to_owned();
            Ok(c)
        }

        (None, None) => bail!("token和配置文件至少得设置一个。"),
    }
}

fn init_log() {
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
}
