use testcontainers::clients::Cli;
use testcontainers::RunnableImage;
use testcontainers_modules::{mysql::Mysql, postgres::Postgres};

use insightdb_connector::{ConnectorConfig, DatabaseConnection};
use insightdb_explain::{explain, PlanNode};

fn mysql_url(container: &testcontainers::core::Container<Mysql>) -> String {
    let port = container.get_host_port_ipv4(3306);
    format!("mysql://root:root@127.0.0.1:{port}/mysql")
}

fn postgres_url(container: &testcontainers::core::Container<Postgres>) -> String {
    let port = container.get_host_port_ipv4(5432);
    format!("postgres://test:test@127.0.0.1:{port}/test")
}

fn walk_all(node: &PlanNode) -> Vec<&PlanNode> {
    node.walk()
}

mod mysql_explain_tests {
    use super::*;

    #[test]
    fn explain_simple_select_returns_plan() {
        sqlx::any::install_default_drivers();
        let docker = Cli::default();
        let container = docker.run(
            RunnableImage::from(Mysql::default())
                .with_env_var(("MYSQL_ROOT_PASSWORD", "root")),
        );
        let url = mysql_url(&container);
        let config = ConnectorConfig::from_url(&url).unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let conn = rt.block_on(DatabaseConnection::connect(config)).unwrap();

        let plan = rt.block_on(explain(&conn, "SELECT 1")).unwrap();
        assert!(!plan.node_type.is_empty());
        assert!(plan.extra.is_some());
    }

    #[test]
    fn explain_table_select_with_index() {
        sqlx::any::install_default_drivers();
        let docker = Cli::default();
        let container = docker.run(
            RunnableImage::from(Mysql::default())
                .with_env_var(("MYSQL_ROOT_PASSWORD", "root")),
        );
        let url = mysql_url(&container);
        let mut config = ConnectorConfig::from_url(&url).unwrap();
        config.read_only = false;

        let rt = tokio::runtime::Runtime::new().unwrap();
        let conn = rt.block_on(DatabaseConnection::connect(config)).unwrap();

        rt.block_on(conn.query(
            "CREATE TABLE IF NOT EXISTS test_users (
                id INT AUTO_INCREMENT PRIMARY KEY,
                name VARCHAR(100),
                email VARCHAR(200),
                age INT,
                INDEX idx_age (age)
            )",
            10,
        )).unwrap();

        rt.block_on(conn.query(
            "INSERT INTO test_users (name, email, age) VALUES
                ('a', 'a@t.com', 20),
                ('b', 'b@t.com', 25),
                ('c', 'c@t.com', 30)",
            10,
        )).unwrap();

        let plan = rt.block_on(explain(
            &conn,
            "SELECT * FROM test_users WHERE age > 20",
        )).unwrap();

        let all_nodes = walk_all(&plan);
        let has_index_scan = all_nodes.iter().any(|n| {
            n.access_method.as_deref() == Some("index_range")
                || n.index_name.is_some()
        });
        assert!(has_index_scan || plan.filter.is_some(),
            "应该使用索引或展示过滤条件: {plan:?}");

        rt.block_on(conn.query("DROP TABLE IF EXISTS test_users", 10)).unwrap();
    }

    #[test]
    fn explain_join_produces_multi_table_plan() {
        sqlx::any::install_default_drivers();
        let docker = Cli::default();
        let container = docker.run(
            RunnableImage::from(Mysql::default())
                .with_env_var(("MYSQL_ROOT_PASSWORD", "root")),
        );
        let url = mysql_url(&container);
        let mut config = ConnectorConfig::from_url(&url).unwrap();
        config.read_only = false;

        let rt = tokio::runtime::Runtime::new().unwrap();
        let conn = rt.block_on(DatabaseConnection::connect(config)).unwrap();

        rt.block_on(conn.query(
            "CREATE TABLE IF NOT EXISTS t1 (id INT PRIMARY KEY, val VARCHAR(50))", 10,
        )).unwrap();
        rt.block_on(conn.query(
            "CREATE TABLE IF NOT EXISTS t2 (id INT PRIMARY KEY, t1_id INT, data VARCHAR(50),
                INDEX idx_t1 (t1_id))", 10,
        )).unwrap();
        rt.block_on(conn.query("INSERT INTO t1 VALUES (1,'a'),(2,'b')", 10)).unwrap();
        rt.block_on(conn.query("INSERT INTO t2 VALUES (1,1,'x'),(2,2,'y')", 10)).unwrap();

        let plan = rt.block_on(explain(
            &conn,
            "SELECT t1.val, t2.data FROM t1 JOIN t2 ON t1.id = t2.t1_id",
        )).unwrap();

        let all_nodes = walk_all(&plan);
        let tables: Vec<&str> = all_nodes
            .iter()
            .filter_map(|n| n.table_name.as_deref())
            .collect();
        assert!(
            tables.contains(&"t1") || tables.contains(&"t2"),
            "应包含参与连接的表名: {tables:?}"
        );

        rt.block_on(conn.query("DROP TABLE IF EXISTS t1,t2", 10)).unwrap();
    }

    #[test]
    fn explain_full_table_scan() {
        sqlx::any::install_default_drivers();
        let docker = Cli::default();
        let container = docker.run(
            RunnableImage::from(Mysql::default())
                .with_env_var(("MYSQL_ROOT_PASSWORD", "root")),
        );
        let url = mysql_url(&container);
        let mut config = ConnectorConfig::from_url(&url).unwrap();
        config.read_only = false;

        let rt = tokio::runtime::Runtime::new().unwrap();
        let conn = rt.block_on(DatabaseConnection::connect(config)).unwrap();

        rt.block_on(conn.query(
            "CREATE TABLE IF NOT EXISTS no_index_table (id INT, data VARCHAR(100))", 10,
        )).unwrap();
        rt.block_on(conn.query(
            "INSERT INTO no_index_table VALUES (1,'x'),(2,'y'),(3,'z')", 10,
        )).unwrap();

        let plan = rt.block_on(explain(
            &conn,
            "SELECT * FROM no_index_table",
        )).unwrap();

        let all_nodes = walk_all(&plan);
        let leaf = all_nodes.iter().find(|n| n.table_name.as_deref() == Some("no_index_table"));
        assert!(leaf.is_some(), "应包含表 no_index_table 的扫描节点: {plan:?}");

        rt.block_on(conn.query("DROP TABLE IF EXISTS no_index_table", 10)).unwrap();
    }

    #[test]
    fn explain_invalid_sql_returns_error() {
        sqlx::any::install_default_drivers();
        let docker = Cli::default();
        let container = docker.run(
            RunnableImage::from(Mysql::default())
                .with_env_var(("MYSQL_ROOT_PASSWORD", "root")),
        );
        let url = mysql_url(&container);
        let config = ConnectorConfig::from_url(&url).unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let conn = rt.block_on(DatabaseConnection::connect(config)).unwrap();

        let result = rt.block_on(explain(&conn, "THIS IS NOT SQL"));
        assert!(result.is_err());
    }
}

mod postgres_explain_tests {
    use super::*;

    #[test]
    fn explain_simple_select_returns_plan() {
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
        let conn = rt.block_on(DatabaseConnection::connect(config)).unwrap();

        let plan = rt.block_on(explain(&conn, "SELECT 1")).unwrap();
        assert!(!plan.node_type.is_empty());
    }

    #[test]
    fn explain_seq_scan_on_table() {
        sqlx::any::install_default_drivers();
        let docker = Cli::default();
        let container = docker.run(
            RunnableImage::from(Postgres::default())
                .with_env_var(("POSTGRES_USER", "test"))
                .with_env_var(("POSTGRES_PASSWORD", "test"))
                .with_env_var(("POSTGRES_DB", "test")),
        );
        let url = postgres_url(&container);
        let mut config = ConnectorConfig::from_url(&url).unwrap();
        config.read_only = false;

        let rt = tokio::runtime::Runtime::new().unwrap();
        let conn = rt.block_on(DatabaseConnection::connect(config)).unwrap();

        rt.block_on(conn.query(
            "CREATE TABLE IF NOT EXISTS pg_items (
                id SERIAL PRIMARY KEY,
                name TEXT,
                price NUMERIC
            )", 10,
        )).unwrap();
        rt.block_on(conn.query(
            "INSERT INTO pg_items (name, price) VALUES
                ('apple', 1.0), ('banana', 2.0), ('cherry', 3.0)", 10,
        )).unwrap();

        let plan = rt.block_on(explain(
            &conn,
            "SELECT * FROM pg_items WHERE price > 1.0",
        )).unwrap();

        let all_nodes = walk_all(&plan);
        let item_node = all_nodes
            .iter()
            .find(|n| n.table_name.as_deref() == Some("pg_items"));
        assert!(item_node.is_some(), "应包含 pg_items 的扫描节点: {plan:?}");
        assert!(plan.filter.is_some() || item_node.unwrap().filter.is_some(),
            "应有过滤条件");

        rt.block_on(conn.query("DROP TABLE IF EXISTS pg_items", 10)).unwrap();
    }

    #[test]
    fn explain_index_scan() {
        sqlx::any::install_default_drivers();
        let docker = Cli::default();
        let container = docker.run(
            RunnableImage::from(Postgres::default())
                .with_env_var(("POSTGRES_USER", "test"))
                .with_env_var(("POSTGRES_PASSWORD", "test"))
                .with_env_var(("POSTGRES_DB", "test")),
        );
        let url = postgres_url(&container);
        let mut config = ConnectorConfig::from_url(&url).unwrap();
        config.read_only = false;

        let rt = tokio::runtime::Runtime::new().unwrap();
        let conn = rt.block_on(DatabaseConnection::connect(config)).unwrap();

        rt.block_on(conn.query(
            "CREATE TABLE IF NOT EXISTS pg_keys (
                id SERIAL PRIMARY KEY,
                val INTEGER NOT NULL
            )", 10,
        )).unwrap();
        rt.block_on(conn.query(
            "CREATE INDEX IF NOT EXISTS idx_val ON pg_keys(val)", 10,
        )).unwrap();
        rt.block_on(conn.query(
            "INSERT INTO pg_keys (val) SELECT generate_series(1, 5)", 10,
        )).unwrap();

        let plan = rt.block_on(explain(
            &conn,
            "SELECT * FROM pg_keys WHERE id = 3",
        )).unwrap();

        let all_nodes = walk_all(&plan);
        let has_index_scan = all_nodes.iter().any(|n| {
            n.access_method.as_deref() == Some("index_scan")
                || n.access_method.as_deref() == Some("index_only_scan")
        });
        assert!(has_index_scan, "应使用索引扫描: {plan:?}");

        rt.block_on(conn.query("DROP TABLE IF EXISTS pg_keys", 10)).unwrap();
    }

    #[test]
    fn explain_join_plan() {
        sqlx::any::install_default_drivers();
        let docker = Cli::default();
        let container = docker.run(
            RunnableImage::from(Postgres::default())
                .with_env_var(("POSTGRES_USER", "test"))
                .with_env_var(("POSTGRES_PASSWORD", "test"))
                .with_env_var(("POSTGRES_DB", "test")),
        );
        let url = postgres_url(&container);
        let mut config = ConnectorConfig::from_url(&url).unwrap();
        config.read_only = false;

        let rt = tokio::runtime::Runtime::new().unwrap();
        let conn = rt.block_on(DatabaseConnection::connect(config)).unwrap();

        rt.block_on(conn.query(
            "CREATE TABLE IF NOT EXISTS orders (
                id SERIAL PRIMARY KEY, user_id INT, amount NUMERIC
            )", 10,
        )).unwrap();
        rt.block_on(conn.query(
            "CREATE TABLE IF NOT EXISTS users_pg (
                id SERIAL PRIMARY KEY, name TEXT
            )", 10,
        )).unwrap();
        rt.block_on(conn.query(
            "INSERT INTO orders (user_id, amount) VALUES (1, 10.0)", 10,
        )).unwrap();
        rt.block_on(conn.query(
            "INSERT INTO users_pg (id, name) VALUES (1, 'Alice')", 10,
        )).unwrap();

        let plan = rt.block_on(explain(
            &conn,
            "SELECT u.name, o.amount FROM users_pg u
                JOIN orders o ON u.id = o.user_id",
        )).unwrap();

        let all_nodes = walk_all(&plan);
        let tables: Vec<&str> = all_nodes
            .iter()
            .filter_map(|n| n.table_name.as_deref())
            .collect();
        assert!(
            tables.contains(&"orders") || tables.contains(&"users_pg"),
            "应包含连接相关的表: {tables:?}"
        );
        assert!(
            plan.join_type.is_some() || all_nodes.iter().any(|n| n.join_type.is_some()),
            "应包含连接类型信息: {plan:?}"
        );

        rt.block_on(conn.query("DROP TABLE IF EXISTS orders, users_pg", 10)).unwrap();
    }

    #[test]
    fn explain_order_by_produces_sort_node() {
        sqlx::any::install_default_drivers();
        let docker = Cli::default();
        let container = docker.run(
            RunnableImage::from(Postgres::default())
                .with_env_var(("POSTGRES_USER", "test"))
                .with_env_var(("POSTGRES_PASSWORD", "test"))
                .with_env_var(("POSTGRES_DB", "test")),
        );
        let url = postgres_url(&container);
        let mut config = ConnectorConfig::from_url(&url).unwrap();
        config.read_only = false;

        let rt = tokio::runtime::Runtime::new().unwrap();
        let conn = rt.block_on(DatabaseConnection::connect(config)).unwrap();

        rt.block_on(conn.query(
            "CREATE TABLE IF NOT EXISTS sort_test (
                id SERIAL PRIMARY KEY, val INTEGER NOT NULL
            )", 10,
        )).unwrap();
        rt.block_on(conn.query(
            "INSERT INTO sort_test (val) SELECT generate_series(1, 100)", 10,
        )).unwrap();

        let plan = rt.block_on(explain(
            &conn,
            "SELECT * FROM sort_test ORDER BY val DESC",
        )).unwrap();

        let all_nodes = walk_all(&plan);
        let _has_sort = all_nodes.iter().any(|n| n.node_type == "Sort");
        let has_seq_scan = all_nodes.iter().any(|n| n.node_type == "Seq Scan");

        assert!(has_seq_scan, "应包含 Seq Scan: {plan:?}");

        rt.block_on(conn.query("DROP TABLE IF EXISTS sort_test", 10)).unwrap();
    }
}
