const DEFAULT_DATABASE_PATH: &str = "__bot.db";

struct Database {}

impl Database {
    pub fn open(path: Option<String>) -> Database {
        let path = path.unwrap_or_else(|| DEFAULT_DATABASE_PATH.to_owned());

        Database {}
    }
}
