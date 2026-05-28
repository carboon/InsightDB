//! InsightDB Connector 集成测试
//!
//! 使用 testcontainers 启动 MySQL/PostgreSQL 容器，验证连接、查询、取消等核心功能。
//! 运行前确保系统已安装 Docker 并处于运行状态。

use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, Image};
use testcontainers_modules::{mysql::Mysql, postgres::Postgres};

use insightdb_connector::{ConnectorConfig, DatabaseConnection, ConnectorError};

/// 获取 MySQL 容器的连接 URL（异步，因为 ContainerAsync 方法需要 await）
async fn mysql_url(container: &ContainerAsync<Mysql>) -> String {
    let host = container.get_host().await.unwrap();
    let port = container.get_host_port_ipv4(3306).await.unwrap();
    format!("mysql://root:root@{host}:{port}/mysql")
}

/// 获取 PostgreSQL 容器的连接 URL
async fn postgres_url(container: &ContainerAsync<Postgres>) -> String {
    let host = container.get_host().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    format!("postgres://test:test@{host}:{port}/test")
}

#[tokio::test]
async fn test_mysql_connect_and_ping() {
    let container = Mysql::default()
        .start()
        .await
        .expect("无法启动 MySQL 容器，请确认 Docker 运行状态");

    let url = mysql_url(&container).await;
    let config = ConnectorConfig::from_url(&url).unwrap();
    let conn = DatabaseConnection::connect(config)
        .await
        .expect("连接 MySQL 失败");

    conn.ping()
        .await
        .expect("MySQL ping 失败");

    let result = conn.query("SELECT 1 AS val", 10)
        .await
        .expect("MySQL 查询失败");

    assert!(!result.columns.is_empty(), "应返回至少一列");
    assert_eq!(result.columns[0], "val");
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0][0], Some("1".to_string()));
}

#[tokio::test]
async fn test_postgres_connect_and_ping() {
    let container = Postgres::default()
        .start()
        .await
        .expect("无法启动 PostgreSQL 容器，请确认 Docker 运行状态");

    let url = postgres_url(&container).await;
    let config = ConnectorConfig::from_url(&url).unwrap();
    let conn = DatabaseConnection::connect(config)
        .await
        .expect("连接 PostgreSQL 失败");

    conn.ping()
        .await
        .expect("PostgreSQL ping 失败");

    let result = conn.query("SELECT 1 AS val", 10)
        .await
        .expect("PostgreSQL 查询失败");

    assert_eq!(result.columns[0], "val");
    assert_eq!(result.rows[0][0], Some("1".to_string()));
}

#[tokio::test]
async fn test_cancel_with_no_active_query() {
    let container = Mysql::default()
        .start()
        .await
        .expect("启动 MySQL 容器失败");

    let url = mysql_url(&container).await;
    let config = ConnectorConfig::from_url(&url).unwrap();
    let conn = DatabaseConnection::connect(config)
        .await
        .expect("连接 MySQL 失败");

    let canceller = conn.canceller();
    let err = canceller.cancel().await.unwrap_err();

    match err {
        ConnectorError::CancelFailed { message, .. } => {
            assert!(message.contains("当前没有正在执行的查询"),
                "取消错误信息应正确提示无活动查询");
        }
        other => panic!("预期 CancelFailed，但得到 {other:?}"),
    }
}

#[tokio::test]
async fn test_mysql_query_fetch_size_limits_rows() {
    let container = Mysql::default()
        .start()
        .await
        .expect("启动 MySQL 容器失败");

    let url = mysql_url(&container).await;
    let config = ConnectorConfig::from_url(&url).unwrap();
    let conn = DatabaseConnection::connect(config)
        .await
        .expect("连接 MySQL 失败");

    let result = conn.query("SELECT * FROM (SELECT 1 AS val UNION ALL SELECT 2 UNION ALL SELECT 3) AS t", 2)
        .await
        .expect("查询失败");

    assert_eq!(result.rows.len(), 2, "fetch_size 应限制返回行数");
}

#[tokio::test]
async fn test_invalid_url_returns_invalid_config() {
    let err = ConnectorConfig::from_url("not-a-url").unwrap_err();
    match err {
        ConnectorError::InvalidConfig { .. } => { /* 正确 */ }
        other => panic!("预期 InvalidConfig，但得到 {other:?}"),
    }
}

#[tokio::test]
async fn test_connection_to_nonexistent_host_fails() {
    let bad_url = "mysql://root:pass@192.0.2.1:3306/test";
    let config = ConnectorConfig::from_url(bad_url).unwrap();
    let err = DatabaseConnection::connect(config)
        .await
        .unwrap_err();

    match err {
        ConnectorError::ConnectionFailed { .. } => { /* 正确 */ }
        other => panic!("预期 ConnectionFailed，但得到 {other:?}"),
    }
}

#[tokio::test]
async fn test_query_syntax_error_returns_query_execution_failed() {
    let container = Mysql::default()
        .start()
        .await
        .expect("启动 MySQL 容器失败");

    let url = mysql_url(&container).await;
    let config = ConnectorConfig::from_url(&url).unwrap();
    let conn = DatabaseConnection::connect(config)
        .await
        .expect("连接 MySQL 失败");

    let err = conn.query("SELECT INVALID SQL", 10)
        .await
        .unwrap_err();

    match err {
        ConnectorError::QueryExecutionFailed { .. } => { /* 正确 */ }
        other => panic!("预期 QueryExecutionFailed，但得到 {other:?}"),
    }
}
