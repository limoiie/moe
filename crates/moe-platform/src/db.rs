//! Platform-level SQLite: Conversation storage (ADR-0005; isolated by Namespace).

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use moe_core::conversation::{AttachmentRef, Conversation, Message, Role};
use rusqlite::Connection;

pub struct Db {
    conn: Connection,
}

/// `data_dir/moe/moe.db`.
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
                 attachments TEXT NOT NULL DEFAULT '[]',
                 created_unix INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_messages_conversation
                 ON messages(conversation_id);",
        )
        .map_err(|err| err.to_string())?;
        // Legacy DB upgrade: messages.attachments is a column added in IIE4AD-358.
        let has_attachments = conn
            .prepare("PRAGMA table_info(messages)")
            .and_then(|mut stmt| {
                let names = stmt
                    .query_map([], |row| row.get::<_, String>(1))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(names.iter().any(|name| name == "attachments"))
            })
            .map_err(|err| err.to_string())?;
        if !has_attachments {
            conn.execute_batch(
                "ALTER TABLE messages ADD COLUMN attachments TEXT NOT NULL DEFAULT '[]'",
            )
            .map_err(|err| err.to_string())?;
        }
        Ok(Self { conn })
    }

    /// Create a conversation, returning its id; `updated_unix` starts at `now`.
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

    /// Append a message (append-only) and touch the conversation's `updated_unix`; attachments store only references (ADR-0010).
    pub fn append_message(
        &self,
        conversation_id: &str,
        role: Role,
        content: &str,
        attachments: &[AttachmentRef],
        now: SystemTime,
    ) -> Result<(), String> {
        let ts = unix_secs(now) as i64;
        let attachments = serde_json::to_string(attachments).map_err(|err| err.to_string())?;
        self.conn
            .execute(
                "INSERT INTO messages (conversation_id, role, content, attachments, created_unix)
                 SELECT ?1, ?2, ?3, ?4, ?5
                 WHERE EXISTS (SELECT 1 FROM conversations WHERE id = ?1)",
                rusqlite::params![conversation_id, role.as_str(), content, attachments, ts],
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

    /// Conversation list: `query` fuzzy-matches titles; ordered by most recent update; Namespace-isolated.
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

    /// Delete one conversation and all of its messages, returning the number of conversations deleted (0 if missing).
    pub fn delete_conversation(&self, conversation_id: &str) -> Result<usize, String> {
        self.conn
            .execute(
                "DELETE FROM messages WHERE conversation_id = ?1",
                rusqlite::params![conversation_id],
            )
            .map_err(|err| err.to_string())?;
        self.conn
            .execute(
                "DELETE FROM conversations WHERE id = ?1",
                rusqlite::params![conversation_id],
            )
            .map_err(|err| err.to_string())
    }

    /// Delete all conversations and their messages under a Namespace (ADR-0022's DeleteAll), returning the number deleted.
    pub fn delete_all_conversations(&self, namespace: &str) -> Result<usize, String> {
        self.conn
            .execute(
                "DELETE FROM messages WHERE conversation_id IN
                 (SELECT id FROM conversations WHERE namespace = ?1)",
                rusqlite::params![namespace],
            )
            .map_err(|err| err.to_string())?;
        self.conn
            .execute(
                "DELETE FROM conversations WHERE namespace = ?1",
                rusqlite::params![namespace],
            )
            .map_err(|err| err.to_string())
    }

    /// The last assistant message in a conversation (used for history preview, IIE4AD-370).
    pub fn last_assistant_message(&self, conversation_id: &str) -> Result<Option<String>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT content FROM messages
                 WHERE conversation_id = ?1 AND role = 'assistant'
                 ORDER BY id DESC LIMIT 1",
            )
            .map_err(|err| err.to_string())?;
        let mut rows = stmt
            .query(rusqlite::params![conversation_id])
            .map_err(|err| err.to_string())?;
        match rows.next().map_err(|err| err.to_string())? {
            Some(row) => Ok(Some(row.get(0).map_err(|err| err.to_string())?)),
            None => Ok(None),
        }
    }

    /// Messages within a conversation (in chronological order).
    pub fn messages(&self, conversation_id: &str) -> Result<Vec<Message>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT role, content, attachments FROM messages
                 WHERE conversation_id = ?1 ORDER BY id ASC",
            )
            .map_err(|err| err.to_string())?;
        let iter = stmt
            .query_map(rusqlite::params![conversation_id], |row| {
                let role: String = row.get(0)?;
                let raw: String = row.get(2)?;
                Ok(Message {
                    role: Role::parse(&role),
                    content: row.get(1)?,
                    // Parse failures count as no attachments (one dirty row must not drop the whole history)
                    attachments: serde_json::from_str(&raw).unwrap_or_default(),
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
        let a = db
            .create_conversation("ai", "explain closures", at(100))
            .unwrap();
        let b = db
            .create_conversation("ai", "write a sort", at(200))
            .unwrap();
        // Appending a message touches a → a is now most recently updated
        db.append_message(&a, Role::User, "explain closures", &[], at(300))
            .unwrap();

        let list = db.conversations("ai", None, 10).unwrap();
        assert_eq!(
            list.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            [a.as_str(), b.as_str()]
        );
        assert_eq!(list[0].title, "explain closures");
        assert_eq!(list[0].updated_unix, 300);
    }

    #[test]
    fn messages_are_ordered_and_namespace_isolated() {
        let db = db();
        let ai = db.create_conversation("ai", "hi", at(0)).unwrap();
        let other = db.create_conversation("echo", "hi", at(0)).unwrap();
        db.append_message(&ai, Role::User, "question", &[], at(1))
            .unwrap();
        db.append_message(&ai, Role::Assistant, "answer", &[], at(2))
            .unwrap();

        let msgs = db.messages(&ai).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, Role::User);
        assert_eq!(msgs[1].role, Role::Assistant);
        assert_eq!(msgs[1].content, "answer");
        assert!(msgs[0].attachments.is_empty());

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
        let target = db
            .create_conversation("ai", "write a sort", at(200))
            .unwrap();
        db.create_conversation("ai", "explain closures", at(100))
            .unwrap();
        db.create_conversation("echo", "sort demo", at(300))
            .unwrap();

        let hits = db.conversations("ai", Some("sort"), 10).unwrap();
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

    /// Attachment references persist with the message (IIE4AD-358).
    #[test]
    fn messages_round_trip_attachment_refs() {
        let db = db();
        let conv = db
            .create_conversation("ai", "look at image", at(0))
            .unwrap();
        let refs = vec![AttachmentRef {
            name: "shot.png".into(),
            path: "/tmp/shot.png".into(),
        }];
        db.append_message(&conv, Role::User, "what is in this image", &refs, at(1))
            .unwrap();
        db.append_message(&conv, Role::Assistant, "it is a cat", &[], at(2))
            .unwrap();

        let messages = db.messages(&conv).unwrap();
        assert_eq!(messages[0].attachments, refs);
        assert!(messages[1].attachments.is_empty());
    }

    /// Missing conversations accept no messages (dangling-row guard; neither tests nor async writes pollute the DB).
    #[test]
    fn append_to_missing_conversation_is_noop() {
        let db = db();
        db.append_message("424242", Role::User, "dangling", &[], at(1))
            .unwrap();
        assert!(db.messages("424242").unwrap().is_empty());
    }

    /// History preview takes the last assistant message (IIE4AD-370).
    #[test]
    fn last_assistant_message_prefers_latest() {
        let db = db();
        let conv = db.create_conversation("ai", "preview", at(0)).unwrap();
        assert_eq!(db.last_assistant_message(&conv).unwrap(), None);
        db.append_message(&conv, Role::User, "question", &[], at(1))
            .unwrap();
        db.append_message(&conv, Role::Assistant, "answer one", &[], at(2))
            .unwrap();
        db.append_message(&conv, Role::User, "ask again", &[], at(3))
            .unwrap();
        db.append_message(&conv, Role::Assistant, "answer two", &[], at(4))
            .unwrap();
        assert_eq!(
            db.last_assistant_message(&conv).unwrap().as_deref(),
            Some("answer two")
        );
    }

    /// Legacy DBs (no attachments column) are upgraded on open, keeping history.
    #[test]
    fn opens_legacy_db_and_migrates_attachments_column() {
        let path = std::env::temp_dir().join(format!(
            "moe-legacy-{}-{}.db",
            std::process::id(),
            "migrate"
        ));
        let _ = std::fs::remove_file(&path);
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE conversations (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     namespace TEXT NOT NULL,
                     title TEXT NOT NULL,
                     created_unix INTEGER NOT NULL,
                     updated_unix INTEGER NOT NULL
                 );
                 CREATE TABLE messages (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     conversation_id INTEGER NOT NULL,
                     role TEXT NOT NULL,
                     content TEXT NOT NULL,
                     created_unix INTEGER NOT NULL
                 );
                 INSERT INTO conversations (namespace, title, created_unix, updated_unix)
                     VALUES ('ai', 'old conversation', 1, 1);
                 INSERT INTO messages (conversation_id, role, content, created_unix)
                     VALUES (1, 'user', 'old message', 1);",
            )
            .unwrap();
        }

        let db = Db::open(&path).unwrap();
        let messages = db.messages("1").unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "old message");
        assert!(messages[0].attachments.is_empty());
        // The upgraded DB accepts attachments
        db.append_message(
            "1",
            Role::User,
            "new message",
            &[AttachmentRef {
                name: "a.md".into(),
                path: "/tmp/a.md".into(),
            }],
            at(2),
        )
        .unwrap();
        assert_eq!(db.messages("1").unwrap()[1].attachments.len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    /// Deleting one conversation: messages cascade-delete, missing returns 0, other Namespaces are unaffected (ADR-0022).
    #[test]
    fn delete_conversation_cascades_and_isolates() {
        let db = db();
        let a = db
            .create_conversation("ai", "conversation A", at(1))
            .unwrap();
        let b = db
            .create_conversation("other", "another extension", at(1))
            .unwrap();
        db.append_message(&a, Role::User, "question", &[], at(2))
            .unwrap();
        db.append_message(&a, Role::Assistant, "answer", &[], at(3))
            .unwrap();

        assert_eq!(db.delete_conversation(&a).unwrap(), 1);
        assert_eq!(db.conversations("ai", None, 10).unwrap().len(), 0);
        assert!(
            db.messages(&a).unwrap().is_empty(),
            "messages cascade-delete"
        );
        assert_eq!(
            db.conversations("other", None, 10).unwrap().len(),
            1,
            "Namespace isolation"
        );
        // Delete again: 0 (does not exist)
        assert_eq!(db.delete_conversation(&a).unwrap(), 0);
        assert_eq!(db.delete_conversation(&b).unwrap(), 1);
    }

    /// Delete all: clears only this Namespace, returns the count deleted (ADR-0022).
    #[test]
    fn delete_all_conversations_clears_only_own_namespace() {
        let db = db();
        db.create_conversation("ai", "one", at(1)).unwrap();
        db.create_conversation("ai", "two", at(2)).unwrap();
        db.create_conversation("other", "another extension", at(1))
            .unwrap();

        assert_eq!(db.delete_all_conversations("ai").unwrap(), 2);
        assert_eq!(db.conversations("ai", None, 10).unwrap().len(), 0);
        assert_eq!(db.conversations("other", None, 10).unwrap().len(), 1);
        // Empty namespace: 0
        assert_eq!(db.delete_all_conversations("ai").unwrap(), 0);
    }
}
