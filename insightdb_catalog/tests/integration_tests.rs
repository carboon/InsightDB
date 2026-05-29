use testcontainers::clients::Cli;
use testcontainers::RunnableImage;
use testcontainers_modules::{mysql::Mysql, postgres::Postgres};

use insightdb_connector::{ConnectorConfig, DatabaseConnection};
use insightdb_catalog::{collect_schema, collect_version, collect_tables, SchemaInfo, TableInfo};

fn mysql_url(container: &testcontainers::core::Container<Mysql>) -> String {
    let port = container.get_host_port_ipv4(3306);
    format!("mysql://root:root@127.0.0.1:{port}/mysql")
}

fn postgres_url(container: &testcontainers::core::Container<Postgres>) -> String {
    let port = container.get_host_port_ipv4(5432);
    format!("postgres://test:test@127.0.0.1:{port}/test")
}

fn find_table<'a>(schema: &'a SchemaInfo, name: &str) -> Option<&'a TableInfo> {
    schema.tables.iter().find(|t| t.name == name)
}

fn has_column(table: &TableInfo, name: &str) -> bool {
    table.columns.iter().any(|c| c.name == name)
}

fn has_index(table: &TableInfo, name: &str) -> bool {
    table.indexes.iter().any(|i| i.name == name)
}

mod mysql_catalog_tests {
    use super::*;

    #[test]
    fn collect_version_is_non_empty() {
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

        let version = rt.block_on(collect_version(&conn)).unwrap();
        assert!(!version.is_empty(), "版本号不应为空");
        assert!(version.contains("8.") || version.contains("MySQL"), "应为 MySQL 版本号: {version}");
    }

    #[test]
    fn collect_empty_schema_has_no_tables() {
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

        rt.block_on(conn.query("CREATE DATABASE IF NOT EXISTS test_empty_catalog", 10)).unwrap();

        let mut empty_config = ConnectorConfig::from_url(
            &format!("mysql://root:root@127.0.0.1:{}/test_empty_catalog",
                     container.get_host_port_ipv4(3306))
        ).unwrap();
        empty_config.read_only = false;
        let empty_conn = rt.block_on(DatabaseConnection::connect(empty_config)).unwrap();

        let schema = rt.block_on(collect_schema(&empty_conn)).unwrap();
        assert_eq!(schema.db_type, "mysql");
        assert!(schema.tables.is_empty(), "空数据库应无表");

        rt.block_on(conn.query("DROP DATABASE IF EXISTS test_empty_catalog", 10)).unwrap();
    }

    #[test]
    fn collect_table_with_columns_and_indexes() {
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
            "CREATE DATABASE IF NOT EXISTS test_catalog", 10,
        )).unwrap();

        let mut cat_config = ConnectorConfig::from_url(
            &format!("mysql://root:root@127.0.0.1:{}/test_catalog",
                     container.get_host_port_ipv4(3306))
        ).unwrap();
        cat_config.read_only = false;
        let cat_conn = rt.block_on(DatabaseConnection::connect(cat_config)).unwrap();

        rt.block_on(cat_conn.query(
            "CREATE TABLE IF NOT EXISTS products (
                id INT AUTO_INCREMENT PRIMARY KEY,
                name VARCHAR(200) NOT NULL,
                price DECIMAL(10,2),
                category VARCHAR(50),
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                INDEX idx_category (category),
                INDEX idx_price_category (price, category)
            )", 10,
        )).unwrap();

        rt.block_on(cat_conn.query(
            "INSERT INTO products (name, price, category) VALUES
                ('Widget', 9.99, 'A'),
                ('Gadget', 19.99, 'B'),
                ('Doohickey', 29.99, 'A')", 10,
        )).unwrap();

        let schema = rt.block_on(collect_schema(&cat_conn)).unwrap();

        let products = find_table(&schema, "products").expect("应找到 products 表");
        assert_eq!(products.engine.as_deref(), Some("InnoDB"));

        assert!(has_column(products, "id"));
        assert!(has_column(products, "name"));
        assert!(has_column(products, "price"));
        assert!(has_column(products, "category"));
        assert!(has_column(products, "created_at"));

        let id_col = products.columns.iter().find(|c| c.name == "id").unwrap();
        assert!(id_col.is_primary_key);

        let name_col = products.columns.iter().find(|c| c.name == "name").unwrap();
        assert!(!name_col.nullable);
        assert!(name_col.data_type.starts_with("varchar"), "应为 varchar 类型: {}", name_col.data_type);

        let created_col = products.columns.iter().find(|c| c.name == "created_at").unwrap();
        assert!(created_col.default_value.is_some());

        assert!(has_index(products, "PRIMARY"));
        assert!(has_index(products, "idx_category"));
        assert!(has_index(products, "idx_price_category"));

        let primary_idx = products.indexes.iter().find(|i| i.name == "PRIMARY").unwrap();
        assert!(primary_idx.is_primary);
        assert!(primary_idx.unique);

        let multi_idx = products.indexes.iter().find(|i| i.name == "idx_price_category").unwrap();
        assert!(multi_idx.columns.contains(&"price".to_string()));
        assert!(multi_idx.columns.contains(&"category".to_string()));

        assert!(!schema.collected_at.is_empty());

        rt.block_on(cat_conn.query("DROP TABLE IF EXISTS products", 10)).unwrap();
        rt.block_on(conn.query("DROP DATABASE IF EXISTS test_catalog", 10)).unwrap();
    }

    #[test]
    fn collect_multiple_tables() {
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

        rt.block_on(conn.query("CREATE DATABASE IF NOT EXISTS test_multi", 10)).unwrap();
        let mut multi_config = ConnectorConfig::from_url(
            &format!("mysql://root:root@127.0.0.1:{}/test_multi",
                     container.get_host_port_ipv4(3306))
        ).unwrap();
        multi_config.read_only = false;
        let multi = rt.block_on(DatabaseConnection::connect(multi_config)).unwrap();

        rt.block_on(multi.query("CREATE TABLE a (id INT PRIMARY KEY)", 10)).unwrap();
        rt.block_on(multi.query("CREATE TABLE b (id INT PRIMARY KEY, ref_id INT)", 10)).unwrap();
        rt.block_on(multi.query("CREATE TABLE c (id INT PRIMARY KEY, data TEXT)", 10)).unwrap();

        let tables = rt.block_on(collect_tables(&multi)).unwrap();
        assert_eq!(tables.len(), 3);
        let names: Vec<&str> = tables.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"a"));
        assert!(names.contains(&"b"));
        assert!(names.contains(&"c"));

        rt.block_on(multi.query("DROP TABLE IF EXISTS a, b, c", 10)).unwrap();
        rt.block_on(conn.query("DROP DATABASE IF EXISTS test_multi", 10)).unwrap();
    }

    #[test]
    fn collect_row_count_estimate() {
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

        rt.block_on(conn.query("CREATE DATABASE IF NOT EXISTS test_rows", 10)).unwrap();
        let mut row_cfg = ConnectorConfig::from_url(
            &format!("mysql://root:root@127.0.0.1:{}/test_rows",
                     container.get_host_port_ipv4(3306))
        ).unwrap();
        row_cfg.read_only = false;
        let row_conn = rt.block_on(DatabaseConnection::connect(row_cfg)).unwrap();

        rt.block_on(row_conn.query(
            "CREATE TABLE counted (id INT PRIMARY KEY AUTO_INCREMENT, val INT)", 10,
        )).unwrap();
        rt.block_on(row_conn.query(
            "INSERT INTO counted (val) VALUES (1),(2),(3),(4),(5)", 10,
        )).unwrap();

        // ANALYZE 更新统计信息
        rt.block_on(row_conn.query("ANALYZE TABLE counted", 10)).unwrap();

        let tables = rt.block_on(collect_tables(&row_conn)).unwrap();
        let counted = tables.iter().find(|t| t.name == "counted").unwrap();
        assert!(counted.row_count_estimate.is_some(), "应有行数估算");
        let rows = counted.row_count_estimate.unwrap();
        assert!(rows >= 3, "估算行数应 >= 3, 实际: {rows}");

        rt.block_on(row_conn.query("DROP TABLE IF EXISTS counted", 10)).unwrap();
        rt.block_on(conn.query("DROP DATABASE IF EXISTS test_rows", 10)).unwrap();
    }
}

mod postgres_catalog_tests {
    use super::*;

    #[test]
    fn collect_pg_version_is_non_empty() {
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

        let version = rt.block_on(collect_version(&conn)).unwrap();
        assert!(!version.is_empty(), "PG 版本号不应为空");
        assert!(version.contains("PostgreSQL"), "应为 PG 版本信息: {version}");
    }

    #[test]
    fn collect_schema_with_table_and_index() {
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
            "CREATE TABLE IF NOT EXISTS employees (
                id SERIAL PRIMARY KEY,
                name VARCHAR(200) NOT NULL,
                salary NUMERIC(10,2),
                department VARCHAR(50),
                hire_date DATE DEFAULT CURRENT_DATE
            )", 10,
        )).unwrap();

        rt.block_on(conn.query(
            "CREATE INDEX IF NOT EXISTS idx_dept ON employees(department)", 10,
        )).unwrap();

        rt.block_on(conn.query(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_name_dept ON employees(name, department)", 10,
        )).unwrap();

        rt.block_on(conn.query(
            "INSERT INTO employees (name, salary, department) VALUES
                ('Alice', 50000, 'Eng'),
                ('Bob', 60000, 'Sales'),
                ('Carol', 70000, 'Eng')", 10,
        )).unwrap();

        rt.block_on(conn.query("ANALYZE employees", 10)).unwrap();

        let schema = rt.block_on(collect_schema(&conn)).unwrap();
        assert_eq!(schema.db_type, "postgresql");

        let emp = find_table(&schema, "employees").expect("应找到 employees 表");
        assert_eq!(emp.table_type, "BASE TABLE");

        assert!(has_column(emp, "id"));
        assert!(has_column(emp, "name"));
        assert!(has_column(emp, "salary"));
        assert!(has_column(emp, "department"));
        assert!(has_column(emp, "hire_date"));

        let id_col = emp.columns.iter().find(|c| c.name == "id").unwrap();
        assert!(id_col.is_primary_key);

        let name_col = emp.columns.iter().find(|c| c.name == "name").unwrap();
        assert!(!name_col.nullable);

        let hire_col = emp.columns.iter().find(|c| c.name == "hire_date").unwrap();
        assert!(hire_col.default_value.is_some());

        assert!(has_index(emp, "idx_dept"));
        assert!(has_index(emp, "idx_name_dept"));

        let unique_idx = emp.indexes.iter().find(|i| i.name == "idx_name_dept").unwrap();
        assert!(unique_idx.unique);
        assert_eq!(unique_idx.columns.len(), 2);

        rt.block_on(conn.query("DROP TABLE IF EXISTS employees", 10)).unwrap();
    }

    #[test]
    fn collect_pg_tables_list() {
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

        rt.block_on(conn.query("CREATE TABLE t1 (id INT)", 10)).unwrap();
        rt.block_on(conn.query("CREATE TABLE t2 (id INT, val TEXT)", 10)).unwrap();

        let tables = rt.block_on(collect_tables(&conn)).unwrap();
        let names: Vec<&str> = tables.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"t1"), "应包含 t1: {names:?}");
        assert!(names.contains(&"t2"), "应包含 t2: {names:?}");

        rt.block_on(conn.query("DROP TABLE IF EXISTS t1, t2", 10)).unwrap();
    }

    #[test]
    fn collect_pg_row_estimate() {
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
            "CREATE TABLE pg_counted (id SERIAL PRIMARY KEY, val INT)", 10,
        )).unwrap();
        rt.block_on(conn.query(
            "INSERT INTO pg_counted (val) SELECT generate_series(1, 50)", 10,
        )).unwrap();
        rt.block_on(conn.query("ANALYZE pg_counted", 10)).unwrap();

        let tables = rt.block_on(collect_tables(&conn)).unwrap();
        let counted = tables.iter().find(|t| t.name == "pg_counted").unwrap();
        assert!(counted.row_count_estimate.is_some(), "应有行数估算");
        let rows = counted.row_count_estimate.unwrap();
        assert!(rows >= 30, "估算行数应 >= 30, 实际: {rows}");

        rt.block_on(conn.query("DROP TABLE IF EXISTS pg_counted", 10)).unwrap();
    }

    #[test]
    fn collect_pg_column_types() {
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
            "CREATE TABLE type_test (
                id SERIAL PRIMARY KEY,
                text_col TEXT,
                int_col INTEGER,
                big_col BIGINT,
                bool_col BOOLEAN,
                ts_col TIMESTAMP DEFAULT NOW()
            )", 10,
        )).unwrap();

        let schema = rt.block_on(collect_schema(&conn)).unwrap();
        let tbl = find_table(&schema, "type_test").unwrap();

        let int_col = tbl.columns.iter().find(|c| c.name == "int_col").unwrap();
        assert_eq!(int_col.data_type, "integer");

        let text_col = tbl.columns.iter().find(|c| c.name == "text_col").unwrap();
        assert_eq!(text_col.data_type, "text");

        let bool_col = tbl.columns.iter().find(|c| c.name == "bool_col").unwrap();
        assert_eq!(bool_col.data_type, "boolean");

        rt.block_on(conn.query("DROP TABLE IF EXISTS type_test", 10)).unwrap();
    }
}
