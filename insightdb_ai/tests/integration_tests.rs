use testcontainers::clients::Cli;
use testcontainers::RunnableImage;
use testcontainers_modules::{mysql::Mysql, postgres::Postgres};

use insightdb_connector::{ConnectorConfig, DatabaseConnection};
use insightdb_catalog::collect_schema;
use insightdb_explain::explain;
use insightdb_rules::run_rules;
use insightdb_advisor::DiagnosisReport;
use insightdb_ai::{Sanitizer, PromptBuilder, MockAiClient, AiClient};

fn mysql_url(container: &testcontainers::core::Container<Mysql>) -> String {
    let port = container.get_host_port_ipv4(3306);
    format!("mysql://root:root@127.0.0.1:{port}/mysql")
}

fn postgres_url(container: &testcontainers::core::Container<Postgres>) -> String {
    let port = container.get_host_port_ipv4(5432);
    format!("postgres://test:test@127.0.0.1:{port}/test")
}

mod mysql_ai_tests {
    use super::*;

    #[test]
    fn end_to_end_mysql_sanitize_prompt_mock_ai() {
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

        rt.block_on(conn.query("CREATE DATABASE IF NOT EXISTS ai_test", 10)).unwrap();
        let mut ac = ConnectorConfig::from_url(
            &format!("mysql://root:root@127.0.0.1:{}/ai_test",
                     container.get_host_port_ipv4(3306))
        ).unwrap();
        ac.read_only = false;
        let ai_conn = rt.block_on(DatabaseConnection::connect(ac)).unwrap();

        // 创建测试表
        rt.block_on(ai_conn.query(
            "CREATE TABLE orders (
                id INT AUTO_INCREMENT PRIMARY KEY,
                customer_email VARCHAR(200),
                amount DECIMAL(10,2),
                status VARCHAR(20),
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )", 10,
        )).unwrap();

        // 插入数据
        rt.block_on(ai_conn.query(
            "INSERT INTO orders (customer_email, amount, status)
             SELECT CONCAT('user', (@r := @r + 1), '@example.com'), 99.99, 'pending'
             FROM information_schema.COLUMNS a,
                  information_schema.COLUMNS b,
                  (SELECT @r := 0) r
             LIMIT 1500", 10,
        )).unwrap();
        rt.block_on(ai_conn.query("ANALYZE TABLE orders", 10)).unwrap();

        let sql = "SELECT * FROM orders WHERE status = 'pending'";

        // 采集 → 诊断
        let schema = rt.block_on(collect_schema(&ai_conn)).unwrap();
        let plan = rt.block_on(explain(&ai_conn, sql)).unwrap();
        let findings = run_rules(&plan, &schema);

        assert!(!findings.is_empty(), "应触发规则");

        let report = DiagnosisReport::new(
            sql, "mysql", "8.0", "ai_test",
            findings, plan, schema,
        );

        // 脱敏
        let mut sanitizer = Sanitizer::new();
        let ctx = sanitizer.sanitize(&report);

        assert!(!ctx.sanitized_sql.contains("orders"));
        assert!(!ctx.sanitized_sql.contains("customer_email"));
        assert!(!ctx.sanitized_sql.contains("pending")); // 字面量被替换
        assert!(!ctx.catalog_summary.contains("orders"));

        // 构建 Prompt
        let builder = PromptBuilder::default();
        let prompt = builder.build(&ctx);

        assert!(prompt.contains("InsightDB"));
        assert!(prompt.contains("# Role"));
        assert!(prompt.contains("FULL_TABLE_SCAN") || prompt.contains("MISSING_INDEX"));
        assert!(!prompt.contains("orders")); // 已脱敏

        // Mock AI
        let client = MockAiClient::new("mock-v1");
        let explanation = rt.block_on(client.explain(&prompt)).unwrap();

        assert!(!explanation.problem_summary.is_empty());
        assert!(!explanation.evidence.is_empty());
        assert!(!explanation.recommendations.is_empty());

        // 验证证据标注
        for e in &explanation.evidence {
            assert!(e.source.starts_with("rule_") || e.source == "explain_plan" || e.source == "schema_metadata");
            assert!(e.is_fact);
        }

        // 验证建议标注
        for r in &explanation.recommendations {
            assert!(r.is_inference);
            assert!(!r.action.is_empty());
        }

        rt.block_on(ai_conn.query("DROP TABLE IF EXISTS orders", 10)).unwrap();
        rt.block_on(conn.query("DROP DATABASE IF EXISTS ai_test", 10)).unwrap();
    }

    #[test]
    fn sanitizer_maps_all_table_names() {
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

        rt.block_on(conn.query("CREATE DATABASE IF NOT EXISTS ai_map", 10)).unwrap();
        let mut mc = ConnectorConfig::from_url(
            &format!("mysql://root:root@127.0.0.1:{}/ai_map",
                     container.get_host_port_ipv4(3306))
        ).unwrap();
        mc.read_only = false;
        let m_conn = rt.block_on(DatabaseConnection::connect(mc)).unwrap();

        rt.block_on(m_conn.query(
            "CREATE TABLE users (id INT PRIMARY KEY, email VARCHAR(200))", 10,
        )).unwrap();
        rt.block_on(m_conn.query(
            "CREATE TABLE products (id INT PRIMARY KEY, name VARCHAR(100))", 10,
        )).unwrap();
        rt.block_on(m_conn.query("INSERT INTO users VALUES (1, 'a@b.com')", 10)).unwrap();
        rt.block_on(m_conn.query("INSERT INTO products VALUES (1, 'test')", 10)).unwrap();

        let schema = rt.block_on(collect_schema(&m_conn)).unwrap();
        let plan = rt.block_on(explain(&m_conn, "SELECT * FROM users")).unwrap();
        let findings = run_rules(&plan, &schema);

        let report = DiagnosisReport::new(
            "SELECT * FROM users", "mysql", "8.0", "ai_map",
            findings, plan, schema,
        );

        let mut sanitizer = Sanitizer::new();
        let ctx = sanitizer.sanitize(&report);

        // 每个表名都应被映射
        // 每个表名都应被映射（映射顺序不固定）
        let table_keys: Vec<&str> = ctx.identifier_mapping.iter()
            .filter(|(k, _)| k.starts_with("table:"))
            .map(|(k, _)| k.as_str())
            .collect();
        assert!(table_keys.contains(&"table:users"), "应包含 users 映射: {table_keys:?}");
        assert!(table_keys.contains(&"table:products"), "应包含 products 映射: {table_keys:?}");

        // SQL 中不应出现原始表名
        assert!(!ctx.sanitized_sql.contains("users"));
        assert!(!ctx.sanitized_sql.contains("products"));

        rt.block_on(m_conn.query("DROP TABLE IF EXISTS users, products", 10)).unwrap();
        rt.block_on(conn.query("DROP DATABASE IF EXISTS ai_map", 10)).unwrap();
    }

    #[test]
    fn prompt_never_contains_credentials_or_raw_data() {
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

        rt.block_on(conn.query("CREATE DATABASE IF NOT EXISTS ai_secure", 10)).unwrap();
        let mut sc = ConnectorConfig::from_url(
            &format!("mysql://root:root@127.0.0.1:{}/ai_secure",
                     container.get_host_port_ipv4(3306))
        ).unwrap();
        sc.read_only = false;
        let s_conn = rt.block_on(DatabaseConnection::connect(sc)).unwrap();

        rt.block_on(s_conn.query(
            "CREATE TABLE secret_data (id INT PRIMARY KEY, ssn VARCHAR(20), phone VARCHAR(20))", 10,
        )).unwrap();
        rt.block_on(s_conn.query(
            "INSERT INTO secret_data VALUES (1, '123-45-6789', '555-0100')", 10,
        )).unwrap();

        let sql = "SELECT ssn, phone FROM secret_data WHERE ssn = '123-45-6789'";
        let schema = rt.block_on(collect_schema(&s_conn)).unwrap();
        let plan = rt.block_on(explain(&s_conn, sql)).unwrap();
        let findings = run_rules(&plan, &schema);

        let report = DiagnosisReport::new(
            sql, "mysql", "8.0", "ai_secure",
            findings, plan, schema,
        );

        let mut sanitizer = Sanitizer::new();
        let ctx = sanitizer.sanitize(&report);
        let builder = PromptBuilder::default();
        let prompt = builder.build(&ctx);

        // 不包含原始敏感数据
        assert!(!prompt.contains("123-45-6789"));
        assert!(!prompt.contains("555-0100"));
        assert!(!prompt.contains("ssn"));
        assert!(!prompt.contains("phone"));
        assert!(!prompt.contains("secret_data"));

        // 不包含凭据
        assert!(!prompt.contains("root"));
        assert!(!prompt.contains("password"));

        rt.block_on(s_conn.query("DROP TABLE IF EXISTS secret_data", 10)).unwrap();
        rt.block_on(conn.query("DROP DATABASE IF EXISTS ai_secure", 10)).unwrap();
    }
}

mod postgres_ai_tests {
    use super::*;

    #[test]
    fn end_to_end_pg_sanitize_prompt_mock_ai() {
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
            "CREATE TABLE pg_events (
                id SERIAL PRIMARY KEY,
                user_id INTEGER NOT NULL,
                event_type TEXT,
                event_data JSONB,
                logged_at TIMESTAMP DEFAULT NOW()
            )", 10,
        )).unwrap();

        rt.block_on(conn.query(
            "INSERT INTO pg_events (user_id, event_type)
             SELECT n, 'click' FROM generate_series(1, 1500) AS n", 10,
        )).unwrap();
        rt.block_on(conn.query("ANALYZE pg_events", 10)).unwrap();

        let sql = "SELECT * FROM pg_events WHERE event_type = 'click'";
        let schema = rt.block_on(collect_schema(&conn)).unwrap();
        let plan = rt.block_on(explain(&conn, sql)).unwrap();
        let findings = run_rules(&plan, &schema);

        assert!(!findings.is_empty(), "应触发规则");

        let report = DiagnosisReport::new(
            sql, "postgresql", "15.0", "test",
            findings, plan, schema,
        );

        let mut sanitizer = Sanitizer::new();
        let ctx = sanitizer.sanitize(&report);

        assert!(!ctx.sanitized_sql.contains("pg_events"));
        assert!(!ctx.sanitized_sql.contains("event_type"));
        assert!(!ctx.sanitized_sql.contains("click"));

        let builder = PromptBuilder::default();
        let prompt = builder.build(&ctx);

        assert!(prompt.contains("InsightDB"));
        assert!(!prompt.contains("pg_events"));

        let client = MockAiClient::new("mock-v2");
        let explanation = rt.block_on(client.explain(&prompt)).unwrap();

        assert!(!explanation.problem_summary.is_empty());
        assert!(explanation.confidence > 0.0);
        assert!(!explanation.evidence.iter().any(|e| !e.is_fact));
        assert!(!explanation.recommendations.iter().any(|r| !r.is_inference));

        rt.block_on(conn.query("DROP TABLE IF EXISTS pg_events", 10)).unwrap();
    }

    #[test]
    fn noop_client_works_when_ai_unavailable() {
        let client = insightdb_ai::NoopAiClient;
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(client.explain("test")).unwrap();

        assert_eq!(result.model, "noop");
        assert!(result.evidence.is_empty());
        assert_eq!(result.confidence, 0.0);
    }
}
