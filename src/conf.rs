use std::path::Path;

use anyhow::{bail, Ok};
use tini::Ini;

const MAIN_SECTION: &str = "Main";
const MAIN_NAME_FIELD: &str = "Name";
const MAIN_TOKEN_FIELD: &str = "Token";

const BOT_STORE_FILE_PATH: &str = "__bot.json";
#[derive(Debug)]
pub struct Config {
    pub name: String,
    pub token: String,
}

impl Config {
    pub fn bot_conf(&self) -> BotConfig {
        BotConfig::new(self.token.to_owned(), BOT_STORE_FILE_PATH.to_owned())
    }
}

impl TryFrom<&Path> for Config {
    type Error = anyhow::Error;

    fn try_from(path: &Path) -> Result<Self, Self::Error> {
        if path.exists() == false {
            bail!("config file does not exist")
        }

        let ini_conf = Ini::from_file(path)?;

        let name = match ini_conf.get(MAIN_SECTION, MAIN_NAME_FIELD) {
            Some(n) => n,
            None => bail!("error config file."),
        };

        let token = match ini_conf.get(MAIN_SECTION, MAIN_TOKEN_FIELD) {
            Some(t) => t,
            None => bail!("error config file."),
        };

        Ok(Config { name, token })
    }
}

#[derive(Debug)]
pub struct BotConfig {
    pub token: String,
    pub store_path: String,
}

impl BotConfig {
    pub fn new(token: String, store_path: String) -> Self {
        Self { token, store_path }
    }
}
