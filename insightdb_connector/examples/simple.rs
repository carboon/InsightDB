//! 简单的 CLI 演示：通过数据库 URL 连接，执行 SELECT 1 并打印结果。
//!
//! 运行方式：
//!   cargo run --example simple -- --db-url mysql://root:pass@localhost:3306/testdb
//!
//! 或设置环境变量 DATABASE_URL 后直接运行：
//!   cargo run --example simple

use std::env;
use insightdb_connector::{ConnectorConfig, DatabaseConnection};

#[tokio::main]
async fn main() {
    env_logger::init();

    // 从命令行参数或环境变量获取连接 URL
    let args: Vec<String> = env::args().collect();
    let url = if let Some(pos) = args.iter().position(|a| a == "--db-url") {
        args.get(pos + 1).cloned()
    } else {
        env::var("DATABASE_URL").ok()
    }.expect("请通过 --db-url 或 DATABASE_URL 提供数据库连接 URL");

    // 解析配置并连接
    let config = ConnectorConfig::from_url(&url)
        .expect("无法解析连接 URL");
    println!("解析配置: {:?}", config);

    let conn = DatabaseConnection::connect(config)
        .await
        .expect("连接数据库失败");

    // 执行SELECT 1
    match conn.ping().await {
        Ok(_) => println!("✅ 数据库连接成功！"),
        Err(e) => eprintln!("❌ 连接失败: {e}"),
    }

    // 执行一个简单查询
    let result = conn.query("SELECT 1 AS one, 2 AS two", 10).await
        .expect("查询失败");

    println!("查询结果:");
    println!("  列: {:?}", result.columns);
    for row in &result.rows {
        println!("  行: {:?}", row);
    }
}
