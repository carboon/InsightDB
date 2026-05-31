pub fn check_dangerous_sql(sql: &str) -> Result<(), DangerReason> {
    let normalized = sql.trim().to_uppercase();
    let stripped = strip_strings_and_comments(&normalized);

    if stripped.contains("DROP DATABASE") || stripped.contains("DROP SCHEMA") {
        return Err(DangerReason::DropDatabase);
    }
    if stripped.contains("DROP TABLE") {
        return Err(DangerReason::DropTable);
    }
    if stripped.contains("DROP ") {
        return Err(DangerReason::DropObject);
    }
    if stripped.contains("TRUNCATE ") {
        return Err(DangerReason::Truncate);
    }
    if stripped.starts_with("DELETE ") && !stripped.contains("WHERE ") {
        return Err(DangerReason::DeleteWithoutWhere);
    }
    if stripped.starts_with("UPDATE ") && !stripped.contains("WHERE ")
        && !stripped.contains("JOIN ")
    {
        return Err(DangerReason::UpdateWithoutWhere);
    }
    if stripped.contains("GRANT ") || stripped.contains("REVOKE ") {
        return Err(DangerReason::PrivilegeChange);
    }
    if stripped.starts_with("ALTER ") && !stripped.starts_with("ALTER SESSION") {
        return Err(DangerReason::AlterDdl);
    }
    if stripped.starts_with("CREATE ") && !stripped.starts_with("CREATE TEMP") && !stripped.starts_with("CREATE TEMPORARY")
    {
        return Err(DangerReason::CreateDdl);
    }

    Ok(())
}

fn strip_strings_and_comments(sql: &str) -> String {
    let mut result = String::with_capacity(sql.len());
    let bytes = sql.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\'' {
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\'' {
                    if i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                        i += 2;
                    } else {
                        i += 1;
                        break;
                    }
                } else {
                    i += 1;
                }
            }
            result.push_str("''");
        } else if bytes[i] == b'-' && i + 1 < bytes.len() && bytes[i + 1] == b'-' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() {
                if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                    i += 2;
                    break;
                }
                i += 1;
            }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    result
}

#[derive(Debug, Clone, PartialEq)]
pub enum DangerReason {
    DropDatabase,
    DropTable,
    DropObject,
    Truncate,
    DeleteWithoutWhere,
    UpdateWithoutWhere,
    PrivilegeChange,
    AlterDdl,
    CreateDdl,
}

impl DangerReason {
    pub fn description(&self) -> &'static str {
        match self {
            DangerReason::DropDatabase => "DROP DATABASE 操作被禁止",
            DangerReason::DropTable => "DROP TABLE 操作被禁止",
            DangerReason::DropObject => "DROP 操作被禁止",
            DangerReason::Truncate => "TRUNCATE 操作被禁止",
            DangerReason::DeleteWithoutWhere => "无 WHERE 条件的 DELETE 被禁止，可能导致全表数据被清空",
            DangerReason::UpdateWithoutWhere => "无 WHERE 条件的 UPDATE 被禁止，可能导致全表数据被修改",
            DangerReason::PrivilegeChange => "权限变更操作 (GRANT/REVOKE) 被禁止",
            DangerReason::AlterDdl => "ALTER DDL 操作被禁止",
            DangerReason::CreateDdl => "CREATE DDL 操作被禁止",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_select() {
        assert!(check_dangerous_sql("SELECT * FROM users").is_ok());
        assert!(check_dangerous_sql("SELECT 1").is_ok());
        assert!(check_dangerous_sql("EXPLAIN SELECT * FROM t").is_ok());
    }

    #[test]
    fn test_drop_blocked() {
        assert!(check_dangerous_sql("DROP TABLE users").is_err());
        assert!(check_dangerous_sql("drop database mydb").is_err());
        assert!(check_dangerous_sql("DROP INDEX idx_name").is_err());
    }

    #[test]
    fn test_truncate_blocked() {
        assert!(check_dangerous_sql("TRUNCATE TABLE users").is_err());
    }

    #[test]
    fn test_delete_without_where_blocked() {
        assert!(check_dangerous_sql("DELETE FROM users").is_err());
    }

    #[test]
    fn test_delete_with_where_allowed() {
        assert!(check_dangerous_sql("DELETE FROM users WHERE id = 1").is_ok());
    }

    #[test]
    fn test_update_without_where_blocked() {
        assert!(check_dangerous_sql("UPDATE users SET name = 'test'").is_err());
    }

    #[test]
    fn test_update_with_where_allowed() {
        assert!(check_dangerous_sql("UPDATE users SET name = 'test' WHERE id = 1").is_ok());
    }

    #[test]
    fn test_grant_revoke_blocked() {
        assert!(check_dangerous_sql("GRANT SELECT ON users TO user1").is_err());
        assert!(check_dangerous_sql("REVOKE SELECT ON users FROM user1").is_err());
    }

    #[test]
    fn test_alter_ddl_blocked() {
        assert!(check_dangerous_sql("ALTER TABLE users ADD COLUMN age INT").is_err());
    }

    #[test]
    fn test_create_ddl_blocked() {
        assert!(check_dangerous_sql("CREATE TABLE t (id INT)").is_err());
    }

    #[test]
    fn test_create_temp_allowed() {
        assert!(check_dangerous_sql("CREATE TEMP TABLE t (id INT)").is_ok());
        assert!(check_dangerous_sql("CREATE TEMPORARY TABLE t (id INT)").is_ok());
    }

    #[test]
    fn test_delete_where_in_string_does_not_count() {
        assert!(check_dangerous_sql("DELETE FROM users WHERE name = 'WHERE is this'").is_ok());
    }

    #[test]
    fn test_gt_lt_operators_not_confused() {
        assert!(check_dangerous_sql("SELECT * FROM users WHERE id > 5 AND age < 30").is_ok());
    }
}
