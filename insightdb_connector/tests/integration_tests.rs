//! InsightDB Connector 集成测试
//!
//! 使用 testcontainers 启动 MySQL/PostgreSQL 容器，验证连接、查询、取消等核心功能。
//! 运行前确保系统已安装 Docker 并处于运行状态。

use testcontainers::clients::Cli;
use testcontainers::core::Container;
use testcontainers::RunnableImage;
use testcontainers_modules::{mysql::Mysql, postgres::Postgres};

use insightdb_connector::{ConnectorConfig, DatabaseConnection, ConnectorError};

/// 获取 MySQL 容器的连接 URL
fn mysql_url(container: &Container<Mysql>) -> String {
    let port = container.get_host_port_ipv4(3306);
    format!("mysql://root:root@127.0.0.1:{port}/mysql")
}

/// 获取 PostgreSQL 容器的连接 URL
fn postgres_url(container: &Container<Postgres>) -> String {
    let port = container.get_host_port_ipv4(5432);
    format!("postgres://test:test@127.0.0.1:{port}/test")
}

// ── 连接与基本查询 ──

#[test]
fn test_mysql_connect_and_ping() {
    sqlx::any::install_default_drivers();
    let docker = Cli::default();
    let container = docker.run(
        RunnableImage::from(Mysql::default())
            .with_env_var(("MYSQL_ROOT_PASSWORD", "root")),
    );
    let url = mysql_url(&container);
    let config = ConnectorConfig::from_url(&url).unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let conn = rt.block_on(DatabaseConnection::connect(config))
        .expect("连接 MySQL 失败");

    rt.block_on(conn.ping()).expect("MySQL ping 失败");

    let result = rt.block_on(conn.query("SELECT '1' AS val", 10))
        .expect("MySQL 查询失败");

    assert!(!result.columns.is_empty(), "应返回至少一列");
    assert_eq!(result.columns[0], "val");
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0][0], Some("1".to_string()));
}

#[test]
fn test_postgres_connect_and_ping() {
    sqlx::any::install_default_drivers();
    let docker = Cli::default();
    let container = docker.run(
        RunnableImage::from(Postgres::default())
            .with_env_var(("POSTGRES_USER", "test"))
            .with_env_var(("POSTGRES_PASSWORD", "test"))
            .with_env_var(("POSTGRES_DB", "test")),
    );
    let url = postgres_url(&container);
    let config = ConnectorConfig::from_url(&url).unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let conn = rt.block_on(DatabaseConnection::connect(config))
        .expect("连接 PostgreSQL 失败");

    rt.block_on(conn.ping()).expect("PostgreSQL ping 失败");

    let result = rt.block_on(conn.query("SELECT '1' AS val", 10))
        .expect("PostgreSQL 查询失败");

    assert_eq!(result.columns[0], "val");
    assert_eq!(result.rows[0][0], Some("1".to_string()));
}

// ── 查询取消 ──

#[test]
fn test_cancel_with_no_active_query() {
    sqlx::any::install_default_drivers();
    let docker = Cli::default();
    let container = docker.run(
        RunnableImage::from(Mysql::default())
            .with_env_var(("MYSQL_ROOT_PASSWORD", "root")),
    );
    let url = mysql_url(&container);
    let config = ConnectorConfig::from_url(&url).unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let conn = rt.block_on(DatabaseConnection::connect(config))
        .expect("连接 MySQL 失败");

    let canceller = conn.canceller();
    let err = rt.block_on(canceller.cancel()).unwrap_err();

    match err {
        ConnectorError::CancelFailed { message, .. } => {
            assert!(message.contains("当前没有正在执行的查询"),
                "取消错误信息应正确提示无活动查询");
        }
        other => panic!("预期 CancelFailed，但得到 {other:?}"),
    }
}

// ── 查询限制与类型处理 ──

#[test]
fn test_mysql_query_fetch_size_limits_rows() {
    sqlx::any::install_default_drivers();
    let docker = Cli::default();
    let container = docker.run(
        RunnableImage::from(Mysql::default())
            .with_env_var(("MYSQL_ROOT_PASSWORD", "root")),
    );
    let url = mysql_url(&container);
    let config = ConnectorConfig::from_url(&url).unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let conn = rt.block_on(DatabaseConnection::connect(config))
        .expect("连接 MySQL 失败");

    let result = rt.block_on(conn.query(
        "SELECT * FROM (SELECT 1 AS val UNION ALL SELECT 2 UNION ALL SELECT 3) AS t",
        2,
    )).expect("查询失败");

    assert_eq!(result.rows.len(), 2, "fetch_size 应限制返回行数");
}

#[test]
fn test_numeric_columns_return_string_representation() {
    sqlx::any::install_default_drivers();
    let docker = Cli::default();
    let container = docker.run(
        RunnableImage::from(Mysql::default())
            .with_env_var(("MYSQL_ROOT_PASSWORD", "root")),
    );
    let url = mysql_url(&container);
    let config = ConnectorConfig::from_url(&url).unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let conn = rt.block_on(DatabaseConnection::connect(config))
        .expect("连接 MySQL 失败");

    let result = rt.block_on(conn.query(
        "SELECT CAST(42 AS SIGNED) AS int_val, CAST(3.14 AS DOUBLE) AS float_val",
        10,
    )).expect("查询失败");

    assert_eq!(result.rows.len(), 1);
    // 数值列应被转换为字符串，而非返回 None
    let int_val = &result.rows[0][0];
    let float_val = &result.rows[0][1];
    assert!(int_val.is_some(), "整数列不应为 None");
    assert!(float_val.is_some(), "浮点列不应为 None");
}

#[test]
fn test_postgres_numeric_columns() {
    sqlx::any::install_default_drivers();
    let docker = Cli::default();
    let container = docker.run(
        RunnableImage::from(Postgres::default())
            .with_env_var(("POSTGRES_USER", "test"))
            .with_env_var(("POSTGRES_PASSWORD", "test"))
            .with_env_var(("POSTGRES_DB", "test")),
    );
    let url = postgres_url(&container);
    let config = ConnectorConfig::from_url(&url).unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let conn = rt.block_on(DatabaseConnection::connect(config))
        .expect("连接 PostgreSQL 失败");

    let result = rt.block_on(conn.query(
        "SELECT 42 AS int_val, 3.14::double precision AS float_val",
        10,
    )).expect("查询失败");

    assert_eq!(result.rows.len(), 1);
    assert!(result.rows[0][0].is_some(), "PG 整数列不应为 None");
    assert!(result.rows[0][1].is_some(), "PG 浮点列不应为 None");
}

#[test]
fn test_empty_result_set() {
    sqlx::any::install_default_drivers();
    let docker = Cli::default();
    let container = docker.run(
        RunnableImage::from(Mysql::default())
            .with_env_var(("MYSQL_ROOT_PASSWORD", "root")),
    );
    let url = mysql_url(&container);
    let config = ConnectorConfig::from_url(&url).unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let conn = rt.block_on(DatabaseConnection::connect(config))
        .expect("连接 MySQL 失败");

    let result = rt.block_on(conn.query(
        "SELECT 1 AS val WHERE 1 = 0",
        10,
    )).expect("查询失败");

    assert!(result.columns.is_empty(), "空结果集应无列");
    assert!(result.rows.is_empty(), "空结果集应无行");
}

// ── 错误处理 ──

#[test]
fn test_invalid_url_returns_invalid_config() {
    let err = ConnectorConfig::from_url("not-a-url").unwrap_err();
    match err {
        ConnectorError::InvalidConfig { .. } => {}
        other => panic!("预期 InvalidConfig，但得到 {other:?}"),
    }
}

#[test]
fn test_connection_to_nonexistent_host_fails() {
    sqlx::any::install_default_drivers();
    let bad_url = "mysql://root:pass@192.0.2.1:3306/test";
    let config = ConnectorConfig::from_url(bad_url).unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let err = rt.block_on(DatabaseConnection::connect(config))
        .unwrap_err();

    match err {
        ConnectorError::ConnectionFailed { .. } => {}
        other => panic!("预期 ConnectionFailed，但得到 {other:?}"),
    }
}

#[test]
fn test_query_syntax_error_returns_query_execution_failed() {
    sqlx::any::install_default_drivers();
    let docker = Cli::default();
    let container = docker.run(
        RunnableImage::from(Mysql::default())
            .with_env_var(("MYSQL_ROOT_PASSWORD", "root")),
    );
    let url = mysql_url(&container);
    let config = ConnectorConfig::from_url(&url).unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let conn = rt.block_on(DatabaseConnection::connect(config))
        .expect("连接 MySQL 失败");

    let err = rt.block_on(conn.query("SELECT INVALID SQL", 10))
        .unwrap_err();

    match err {
        ConnectorError::QueryExecutionFailed { .. } => {}
        other => panic!("预期 QueryExecutionFailed，但得到 {other:?}"),
    }
}

#[test]
fn test_unsupported_database_scheme() {
    let err = ConnectorConfig::from_url("sqlite:///tmp/db").unwrap_err();
    match err {
        ConnectorError::InvalidConfig { message, .. } => {
            assert!(message.contains("不支持的数据库协议"));
        }
        other => panic!("预期 InvalidConfig，但得到 {other:?}"),
    }
}

#[test]
fn test_missing_database_name_in_url() {
    let err = ConnectorConfig::from_url("mysql://root:pass@localhost:3306/").unwrap_err();
    match err {
        ConnectorError::InvalidConfig { message, .. } => {
            assert!(message.contains("数据库名称"));
        }
        other => panic!("预期 InvalidConfig，但得到 {other:?}"),
    }
}

// ── 错误模型方法 ──

#[test]
fn test_connector_error_code_and_retryable() {
    let err = ConnectorError::Timeout {
        elapsed_secs: 30,
        suggestion: Some("重试".to_string()),
    };
    assert_eq!(err.code(), "TIMEOUT");
    assert!(err.retryable());
    assert!(err.suggestion().is_some());
}

#[test]
fn test_connector_error_cancelled_not_retryable() {
    let err = ConnectorError::Cancelled;
    assert_eq!(err.code(), "CANCELLED");
    assert!(!err.retryable());
}

// ── 配置 Debug 脱敏 ──

#[test]
fn test_config_debug_masks_password() {
    let config = ConnectorConfig::from_url("mysql://admin:s3cret@host/db").unwrap();
    let debug_output = format!("{:?}", config);
    assert!(debug_output.contains("********"), "密码应在 Debug 输出中被遮蔽");
    assert!(!debug_output.contains("s3cret"), "原始密码不应出现在 Debug 输出中");
}

// ── 连接池 close ──

#[test]
fn test_close_connection() {
    sqlx::any::install_default_drivers();
    let docker = Cli::default();
    let container = docker.run(
        RunnableImage::from(Mysql::default())
            .with_env_var(("MYSQL_ROOT_PASSWORD", "root")),
    );
    let url = mysql_url(&container);
    let config = ConnectorConfig::from_url(&url).unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let conn = rt.block_on(DatabaseConnection::connect(config))
        .expect("连接 MySQL 失败");

    // close 应不 panic
    rt.block_on(conn.close());
}
