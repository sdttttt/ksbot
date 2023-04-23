use anyhow::bail;
use tracing::instrument::WithSubscriber;

use std::path::PathBuf;
use structopt::StructOpt;

use tracing::{debug, error, info, warn, Level};

use crate::api::http::init_kook_client;
use crate::conf::Config;
use crate::fetch::init_rss_client;
use crate::network_runtime::BotNetworkRuntime;

mod api;
mod conf;
mod data;
mod db;
mod fetch;
mod network_frame;
mod network_runtime;
mod push;
mod runtime;
mod utils;

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
    init_tracing()?;

    let conf = parse_conf()?;

    init_rss_client(None);
    init_kook_client(conf.bot_conf());

    let mut ksbot_runtime = runtime::KsbotRuntime::new();
    let mut network_runtime = BotNetworkRuntime::init(conf.bot_conf());

    info!("ksbot starting ...");

    tokio::select! {
        r = ksbot_runtime.subscribe(network_runtime.subscribe_event()) => {
            if let Err(e) = r {error!("ksbot 意外退出: {:?}", e);}
        },
        r = network_runtime.connect() => {
            if let Err(e)  = r {
                error!("ksbot network 意外退出: {:?}", e);
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
            let c = Config {
                token: token.to_owned(),
                ..Default::default()
            };
            Ok(c)
        }

        (None, None) => bail!("token和配置文件至少得设置一个。"),
    }
}

fn init_tracing() -> anyhow::Result<()> {
    use tracing_subscriber::fmt::writer::MakeWriterExt;
    use tracing_subscriber::prelude::*;

    let appender = tracing_appender::rolling::hourly("", "bog.log");

    let (non_blocking, _guard) = tracing_appender::non_blocking(appender);

    let sub = tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::Layer::new()
                .with_writer(std::io::stdout.with_max_level(Level::INFO))
                .with_file(true)
                .with_line_number(true)
                .with_thread_ids(true)
                .with_target(false)
                .compact(),
        )
        .with(
            tracing_subscriber::fmt::Layer::new()
                .with_writer(non_blocking.with_max_level(Level::INFO))
                .with_file(true)
                .with_line_number(true)
                .with_thread_ids(true)
                .with_target(false)
                .json(),
        );

    tracing::subscriber::set_global_default(sub)?;
    Ok(())
}
