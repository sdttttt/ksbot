use anyhow::bail;
use std::{env, path::Path};

use crate::conf::Config;
use crate::runtime::BotRuntime;

mod api;
mod conf;
mod event_hook;
mod fetch;
mod rss_event;
mod runtime;
mod ws;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    println!("ksbot starting...");

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

    let conf = Config::try_from(Path::new(&conf_path))?;

    let mut hook = rss_event::KsbotRuntime::new();

    let mut runtime = BotRuntime::init(conf.bot_conf(), &mut hook).await;
    match runtime.run().await {
        Ok(_) => {}
        Err(e) => bail!("意外退出：{}", e),
    }
    Ok(())
}
