pub const STATUS: &str = "SELECT @@SERVERNAME AS server_name, @@VERSION AS server_version, DB_NAME() AS current_database";

pub const DATABASES: &str =
    "SELECT name, database_id, state_desc, user_access_desc FROM sys.databases";

pub const TABLES: &str = r#"
SELECT TABLE_SCHEMA, TABLE_NAME, TABLE_TYPE
FROM INFORMATION_SCHEMA.TABLES
"#;

pub const DESCRIBE: &str = r#"
SELECT COLUMN_NAME, DATA_TYPE, IS_NULLABLE, COLUMN_DEFAULT
FROM INFORMATION_SCHEMA.COLUMNS
WHERE TABLE_SCHEMA = @P1 AND TABLE_NAME = @P2
ORDER BY ORDINAL_POSITION
"#;

pub const COLUMNS: &str = r#"
SELECT TABLE_SCHEMA, TABLE_NAME, COLUMN_NAME, DATA_TYPE
FROM INFORMATION_SCHEMA.COLUMNS
"#;

pub const INDEXES: &str = r#"
SELECT t.name AS table_name, i.name AS index_name, i.type_desc
FROM sys.indexes i
JOIN sys.tables t ON i.object_id = t.object_id
WHERE i.is_hypothetical = 0
"#;

pub const FOREIGN_KEYS: &str = r#"
SELECT fk.name AS fk_name, tp.name AS parent_table, tr.name AS referenced_table
FROM sys.foreign_keys fk
JOIN sys.tables tp ON fk.parent_object_id = tp.object_id
JOIN sys.tables tr ON fk.referenced_object_id = tr.object_id
"#;
