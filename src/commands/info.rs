use crate::database;
use anyhow::Result;

pub struct InfoResult {
    pub database_uri: String,
    pub git_sha: String,
}

pub fn command_info(db: &Box<dyn database::StingyDatabase>) -> Result<InfoResult> {
    Ok(InfoResult {
        database_uri: db.get_uri(),
        git_sha: env!("VERGEN_GIT_SHA").to_string(),
    })
}

#[cfg(test)]
mod info_tests {
    use super::*;
    use crate::database::open_stingy_testing_database;

    #[test]
    fn info_test() {
        let db = open_stingy_testing_database();
        let info = command_info(&db).unwrap();
        // By hardcoding ":memory:", this test is aware that we're testing on
        // an in-memory SQLite database. Not great, but fine in a quick test.
        assert_eq!(info.database_uri, "file://:memory:");
        assert_eq!(info.git_sha.len(), 40)
    }
}
