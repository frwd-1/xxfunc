use eyre::Result;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, OpenFlags};
use std::{fs::OpenOptions, path::Path};

pub type ModuleId = i64;

// Enum to represent module states
#[derive(Debug, Clone, Copy)]
pub enum ModuleState {
    Started,
    Stopped,
}

// database for storing and retrieving wasm modules
#[derive(Debug, Clone)]
pub struct ModuleDatabase {
    pool: Pool<SqliteConnectionManager>,
}

impl ModuleDatabase {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let _ =
            OpenOptions::new().read(true).write(true).create(true).truncate(true).open(&path)?;

        let manager =
            SqliteConnectionManager::file(path).with_flags(OpenFlags::SQLITE_OPEN_READ_WRITE);
        let pool = Pool::new(manager)?;

        let conn = pool.get()?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS modules (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                binary BLOB NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS module_states (
                module_id INTEGER PRIMARY KEY,
                state TEXT NOT NULL,
                FOREIGN KEY(module_id) REFERENCES modules(id)
            )",
            [],
        )?;

        Ok(ModuleDatabase { pool })
    }

    pub fn insert(&self, name: &str, binary: &[u8]) -> Result<()> {
        let conn = self.pool.get().unwrap();
        conn.execute("INSERT INTO modules (name, binary) VALUES (?1, ?2)", params![name, binary])?;
        let module_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO module_states (module_id, state) VALUES (?1, ?2)",
            params![module_id, ModuleState::Stopped.to_string()],
        )?;
        Ok(())
    }

    pub fn get(&self, id: ModuleId) -> Result<Option<Vec<u8>>> {
        let conn = self.pool.get()?;

        let mut stmt = conn.prepare("SELECT binary FROM modules WHERE id = ?1")?;
        let mut rows = stmt.query(rusqlite::params![id])?;

        if let Some(row) = rows.next()? {
            let binary: Vec<u8> = row.get(0)?;
            Ok(Some(binary))
        } else {
            Ok(None)
        }
    }

    pub fn delete(&self, name: &str) -> Result<()> {
        let mut conn = self.pool.get()?;
        let tx = conn.transaction()?;

        let module_id: i64 =
            tx.query_row("SELECT id FROM modules WHERE name = ?1", params![name], |row| {
                row.get(0)
            })?;

        tx.execute("DELETE FROM module_states WHERE module_id = ?1", params![module_id])?;
        tx.execute("DELETE FROM modules WHERE id = ?1", params![module_id])?;

        tx.commit()?;
        Ok(())
    }

    pub fn set_state(&self, name: &str, state: ModuleState) -> Result<()> {
        let conn = self.pool.get().unwrap();
        let rows_affected = conn.execute(
            "UPDATE module_states
             SET state = ?1
             WHERE module_id = (SELECT id FROM modules WHERE name = ?2)",
            params![state.to_string(), name],
        )?;
        if rows_affected == 0 {
            return Err(eyre::eyre!("Module not found"));
        }
        Ok(())
    }

    pub fn get_modules_by_state(&self, state: ModuleState) -> Result<Vec<ModuleId>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT module_id
             FROM module_states
             WHERE state = ?1",
        )?;

        let rows = stmt.query_map(params![state.to_string()], |row| row.get(0))?;
        let mut modules = Vec::new();

        for id in rows {
            modules.push(id?);
        }

        Ok(modules)
    }

    // Function to create a test database and insert a WASM binary file
    pub fn create_test_db() -> Result<Self> {
        let temp_file = tempfile::NamedTempFile::new()?.into_temp_path().with_extension("db");
        let db = ModuleDatabase::open(&temp_file)?;
        let bytes = include_bytes!("../../examples/minimal/wasm_output/output.wasm");
        db.insert("test_module", bytes)?;
        Ok(db)
    }
}

impl std::fmt::Display for ModuleState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModuleState::Started => write!(f, "Started"),
            ModuleState::Stopped => write!(f, "Stopped"),
        }
    }
}

impl std::str::FromStr for ModuleState {
    type Err = eyre::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Started" => Ok(ModuleState::Started),
            "Stopped" => Ok(ModuleState::Stopped),
            _ => Err(eyre::eyre!("Invalid module state")),
        }
    }
}
