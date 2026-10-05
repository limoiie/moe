//! 平台级 SQLite：Conversation 存储（ADR-0005；按 Namespace 隔离）。

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use moe_core::conversation::{Conversation, Message, Role};
use rusqlite::Connection;

pub struct Db {
    conn: Connection,
}

/// `data_dir/moe/moe.db`。
pub fn db_path() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("moe").join("moe.db"))
}

impl Db {
    pub fn open_default() -> Result<Self, String> {
        let path = db_path().ok_or_else(|| "no data dir".to_string())?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|err| err.to_string())?;
        }
        Self::open(&path)
    }

    pub fn open(path: &std::path::Path) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|err| err.to_string())?;
        Self::from_conn(conn)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self, String> {
        Self::from_conn(Connection::open_in_memory().map_err(|err| err.to_string())?)
    }

    fn from_conn(conn: Connection) -> Result<Self, String> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS conversations (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 namespace TEXT NOT NULL,
                 title TEXT NOT NULL,
                 created_unix INTEGER NOT NULL,
                 updated_unix INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS messages (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 conversation_id INTEGER NOT NULL,
                 role TEXT NOT NULL,
                 content TEXT NOT NULL,
                 created_unix INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_messages_conversation
                 ON messages(conversation_id);",
        )
        .map_err(|err| err.to_string())?;
        Ok(Self { conn })
    }

    /// 创建会话，返回 id；`updated_unix` 初始为 `now`。
    pub fn create_conversation(
        &self,
        namespace: &str,
        title: &str,
        now: SystemTime,
    ) -> Result<String, String> {
        let ts = unix_secs(now) as i64;
        self.conn
            .execute(
                "INSERT INTO conversations (namespace, title, created_unix, updated_unix)
                 VALUES (?1, ?2, ?3, ?3)",
                rusqlite::params![namespace, title, ts],
            )
            .map_err(|err| err.to_string())?;
        Ok(self.conn.last_insert_rowid().to_string())
    }

    /// 追加消息（append-only）并 touch 会话的 `updated_unix`。
    pub fn append_message(
        &self,
        conversation_id: &str,
        role: Role,
        content: &str,
        now: SystemTime,
    ) -> Result<(), String> {
        let ts = unix_secs(now) as i64;
        self.conn
            .execute(
                "INSERT INTO messages (conversation_id, role, content, created_unix)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![conversation_id, role.as_str(), content, ts],
            )
            .map_err(|err| err.to_string())?;
        self.conn
            .execute(
                "UPDATE conversations SET updated_unix = ?1 WHERE id = ?2",
                rusqlite::params![ts, conversation_id],
            )
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    /// 会话列表：`query` 模糊搜标题；按最近更新倒序；Namespace 隔离。
    pub fn conversations(
        &self,
        namespace: &str,
        query: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Conversation>, String> {
        let limit = limit as i64;
        let mut rows = Vec::new();

        let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<Conversation> {
            let id: i64 = row.get(0)?;
            Ok(Conversation {
                id: id.to_string(),
                namespace: row.get(1)?,
                title: row.get(2)?,
                updated_unix: row.get::<_, i64>(3)? as u64,
            })
        };

        match query.map(str::trim).filter(|q| !q.is_empty()) {
            Some(q) => {
                let mut stmt = self
                    .conn
                    .prepare(
                        "SELECT id, namespace, title, updated_unix FROM conversations
                         WHERE namespace = ?1 AND title LIKE '%' || ?2 || '%'
                         ORDER BY updated_unix DESC, id DESC LIMIT ?3",
                    )
                    .map_err(|err| err.to_string())?;
                let iter = stmt
                    .query_map(rusqlite::params![namespace, q, limit], map_row)
                    .map_err(|err| err.to_string())?;
                for row in iter {
                    rows.push(row.map_err(|err| err.to_string())?);
                }
            }
            None => {
                let mut stmt = self
                    .conn
                    .prepare(
                        "SELECT id, namespace, title, updated_unix FROM conversations
                         WHERE namespace = ?1
                         ORDER BY updated_unix DESC, id DESC LIMIT ?2",
                    )
                    .map_err(|err| err.to_string())?;
                let iter = stmt
                    .query_map(rusqlite::params![namespace, limit], map_row)
                    .map_err(|err| err.to_string())?;
                for row in iter {
                    rows.push(row.map_err(|err| err.to_string())?);
                }
            }
        }
        Ok(rows)
    }

    /// 会话内的消息（按时间正序）。
    pub fn messages(&self, conversation_id: &str) -> Result<Vec<Message>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT role, content FROM messages WHERE conversation_id = ?1 ORDER BY id ASC",
            )
            .map_err(|err| err.to_string())?;
        let iter = stmt
            .query_map(rusqlite::params![conversation_id], |row| {
                let role: String = row.get(0)?;
                Ok(Message {
                    role: Role::parse(&role),
                    content: row.get(1)?,
                })
            })
            .map_err(|err| err.to_string())?;
        let mut messages = Vec::new();
        for row in iter {
            messages.push(row.map_err(|err| err.to_string())?);
        }
        Ok(messages)
    }
}

fn unix_secs(now: SystemTime) -> u64 {
    now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    fn db() -> Db {
        Db::open_in_memory().unwrap()
    }

    #[test]
    fn conversations_round_trip_and_order_by_recency() {
        let db = db();
        let a = db.create_conversation("ai", "解释闭包", at(100)).unwrap();
        let b = db.create_conversation("ai", "写一段排序", at(200)).unwrap();
        // 追加消息会 touch a → a 最近更新
        db.append_message(&a, Role::User, "解释闭包", at(300))
            .unwrap();

        let list = db.conversations("ai", None, 10).unwrap();
        assert_eq!(
            list.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            [a.as_str(), b.as_str()]
        );
        assert_eq!(list[0].title, "解释闭包");
        assert_eq!(list[0].updated_unix, 300);
    }

    #[test]
    fn messages_are_ordered_and_namespace_isolated() {
        let db = db();
        let ai = db.create_conversation("ai", "hi", at(0)).unwrap();
        let other = db.create_conversation("echo", "hi", at(0)).unwrap();
        db.append_message(&ai, Role::User, "问", at(1)).unwrap();
        db.append_message(&ai, Role::Assistant, "答", at(2))
            .unwrap();

        let msgs = db.messages(&ai).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, Role::User);
        assert_eq!(msgs[1].role, Role::Assistant);
        assert_eq!(msgs[1].content, "答");

        let ai_list = db.conversations("ai", None, 10).unwrap();
        let echo_list = db.conversations("echo", None, 10).unwrap();
        assert_eq!(ai_list.len(), 1);
        assert_eq!(ai_list[0].id, ai);
        assert_eq!(echo_list.len(), 1);
        assert_eq!(echo_list[0].id, other);
    }

    #[test]
    fn title_search_filters_within_namespace() {
        let db = db();
        let target = db.create_conversation("ai", "写一段排序", at(200)).unwrap();
        db.create_conversation("ai", "解释闭包", at(100)).unwrap();
        db.create_conversation("echo", "排序 demo", at(300))
            .unwrap();

        let hits = db.conversations("ai", Some("排序"), 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, target);
        assert!(db.conversations("ai", Some("zzz"), 10).unwrap().is_empty());
    }

    #[test]
    fn limit_caps_results() {
        let db = db();
        for i in 0..5 {
            db.create_conversation("ai", &format!("c{i}"), at(100 + i))
                .unwrap();
        }
        assert_eq!(db.conversations("ai", None, 3).unwrap().len(), 3);
    }
}
