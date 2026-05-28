//! InsightDB Connector 集成测试
//!
//! 使用 testcontainers 启动 MySQL/PostgreSQL 容器，验证连接、查询、取消等核心功能。
//! 运行前确保系统已安装 Docker 并处于运行状态。
//!
//! 注意：测试为同步形式，内部通过 tokio::runtime::Runtime 执行异步代码。

use testcontainers::core::Container;
use testcontainers::Image;
use testcontainers_modules::{mysql::Mysql, postgres::Postgres};

use insightdb_connector::{ConnectorConfig, DatabaseConnection, ConnectorError};

/// 获取 MySQL 容器的连接 URL（同步方法）
fn mysql_url(container: &Container<Mysql>) -> String {
    let host = container.get_host();
    let port = container.get_host_port_ipv4(3306);
    format!("mysql://root:root@{host}:{port}/mysql")
}

/// 获取 PostgreSQL 容器的连接 URL
fn postgres_url(container: &Container<Postgres>) -> String {
    let host = container.get_host();
    let port = container.get_host_port_ipv4(5432);
    format!("postgres://test:test@{host}:{port}/test")
}

#[test]
fn test_mysql_connect_and_ping() {
    let container = Mysql::default().run();
    let url = mysql_url(&container);
    let config = ConnectorConfig::from_url(&url).unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let conn = rt.block_on(DatabaseConnection::connect(config))
        .expect("连接 MySQL 失败");

    rt.block_on(conn.ping()).expect("MySQL ping 失败");

    let result = rt.block_on(conn.query("SELECT 1 AS val", 10))
        .expect("MySQL 查询失败");

    assert!(!result.columns.is_empty(), "应返回至少一列");
    assert_eq!(result.columns[0], "val");
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0][0], Some("1".to_string()));
}

#[test]
fn test_postgres_connect_and_ping() {
    let container = Postgres::default().run();
    let url = postgres_url(&container);
    let config = ConnectorConfig::from_url(&url).unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let conn = rt.block_on(DatabaseConnection::connect(config))
        .expect("连接 PostgreSQL 失败");

    rt.block_on(conn.ping()).expect("PostgreSQL ping 失败");

    let result = rt.block_on(conn.query("SELECT 1 AS val", 10))
        .expect("PostgreSQL 查询失败");

    assert_eq!(result.columns[0], "val");
    assert_eq!(result.rows[0][0], Some("1".to_string()));
}

#[test]
fn test_cancel_with_no_active_query() {
    let container = Mysql::default().run();
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

#[test]
fn test_mysql_query_fetch_size_limits_rows() {
    let container = Mysql::default().run();
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
fn test_invalid_url_returns_invalid_config() {
    let err = ConnectorConfig::from_url("not-a-url").unwrap_err();
    match err {
        ConnectorError::InvalidConfig { .. } => { /* 正确 */ }
        other => panic!("预期 InvalidConfig，但得到 {other:?}"),
    }
}

#[test]
fn test_connection_to_nonexistent_host_fails() {
    let bad_url = "mysql://root:pass@192.0.2.1:3306/test";
    let config = ConnectorConfig::from_url(bad_url).unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let err = rt.block_on(DatabaseConnection::connect(config))
        .unwrap_err();

    match err {
        ConnectorError::ConnectionFailed { .. } => { /* 正确 */ }
        other => panic!("预期 ConnectionFailed，但得到 {other:?}"),
    }
}

#[test]
fn test_query_syntax_error_returns_query_execution_failed() {
    let container = Mysql::default().run();
    let url = mysql_url(&container);
    let config = ConnectorConfig::from_url(&url).unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let conn = rt.block_on(DatabaseConnection::connect(config))
        .expect("连接 MySQL 失败");

    let err = rt.block_on(conn.query("SELECT INVALID SQL", 10))
        .unwrap_err();

    match err {
        ConnectorError::QueryExecutionFailed { .. } => { /* 正确 */ }
        other => panic!("预期 QueryExecutionFailed，但得到 {other:?}"),
    }
}
