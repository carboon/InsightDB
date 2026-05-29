use testcontainers::clients::Cli;
use testcontainers::RunnableImage;
use testcontainers_modules::{mysql::Mysql, postgres::Postgres};

use insightdb_connector::{ConnectorConfig, DatabaseConnection};
use insightdb_catalog::{collect_schema, collect_version};
use insightdb_explain::explain;
use insightdb_rules::{run_rules, Severity};
use insightdb_advisor::DiagnosisReport;

fn mysql_url(container: &testcontainers::core::Container<Mysql>) -> String {
    let port = container.get_host_port_ipv4(3306);
    format!("mysql://root:root@127.0.0.1:{port}/mysql")
}

fn postgres_url(container: &testcontainers::core::Container<Postgres>) -> String {
    let port = container.get_host_port_ipv4(5432);
    format!("postgres://test:test@127.0.0.1:{port}/test")
}

mod mysql_phase2_tests {
    use super::*;

    #[test]
    fn full_table_scan_triggers_rules_and_report() {
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

        // 创建大表（无索引）
        rt.block_on(conn.query("CREATE DATABASE IF NOT EXISTS phase2_test", 10)).unwrap();
        let mut p2_config = ConnectorConfig::from_url(
            &format!("mysql://root:root@127.0.0.1:{}/phase2_test",
                     container.get_host_port_ipv4(3306))
        ).unwrap();
        p2_config.read_only = false;
        let p2 = rt.block_on(DatabaseConnection::connect(p2_config)).unwrap();

        rt.block_on(p2.query(
            "CREATE TABLE big_logs (
                id INT AUTO_INCREMENT PRIMARY KEY,
                user_id INT,
                action VARCHAR(50),
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )", 10,
        )).unwrap();

        // 插入 1500 行触发全表扫描告警（阈值 1000）
        rt.block_on(p2.query(
            "INSERT INTO big_logs (user_id, action)
             SELECT (@r := @r + 1), 'auto'
             FROM information_schema.COLUMNS a,
                  information_schema.COLUMNS b,
                  (SELECT @r := 0) r
             LIMIT 1500", 10,
        )).unwrap();
        rt.block_on(p2.query("ANALYZE TABLE big_logs", 10)).unwrap();

        // 全表扫描查询（无 WHERE 条件，大表）
        let sql = "SELECT * FROM big_logs";

        let schema = rt.block_on(collect_schema(&p2)).unwrap();
        let plan = rt.block_on(explain(&p2, sql)).unwrap();
        let findings = run_rules(&plan, &schema);

        assert!(!findings.is_empty(), "应对全表扫描产生告警: {:?}", findings);
        assert!(findings.iter().any(|f| f.id == "FULL_TABLE_SCAN"),
            "应包含 FULL_TABLE_SCAN: {:?}", findings);

        // 构建报告
        let version = rt.block_on(collect_version(&p2)).unwrap();
        let report = DiagnosisReport::new(
            sql, "mysql", &version, "phase2_test",
            findings, plan, schema,
        );
        assert!(report.total_findings >= 1);
        assert!(!report.summary.is_empty());

        // 序列化验证
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("FULL_TABLE_SCAN"));

        rt.block_on(p2.query("DROP TABLE IF EXISTS big_logs", 10)).unwrap();
        rt.block_on(conn.query("DROP DATABASE IF EXISTS phase2_test", 10)).unwrap();
    }

    #[test]
    fn multiple_rules_can_trigger_simultaneously() {
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

        rt.block_on(conn.query("CREATE DATABASE IF NOT EXISTS phase2_multi", 10)).unwrap();
        let mut mc = ConnectorConfig::from_url(
            &format!("mysql://root:root@127.0.0.1:{}/phase2_multi",
                     container.get_host_port_ipv4(3306))
        ).unwrap();
        mc.read_only = false;
        let mc_conn = rt.block_on(DatabaseConnection::connect(mc)).unwrap();

        rt.block_on(mc_conn.query(
            "CREATE TABLE no_index_table (id INT, val VARCHAR(200), cat VARCHAR(50))", 10,
        )).unwrap();
        rt.block_on(mc_conn.query(
            "INSERT INTO no_index_table (id, val, cat)
             SELECT n, CONCAT('val_', n), IF(n % 2 = 0, 'A', 'B')
             FROM (SELECT 1 AS n UNION ALL SELECT 2 UNION ALL SELECT 3 UNION ALL
                   SELECT 4 UNION ALL SELECT 5) t", 10,
        )).unwrap();
        rt.block_on(mc_conn.query("ANALYZE TABLE no_index_table", 10)).unwrap();

        // 全表扫描 + filesort + 无索引
        let sql = "SELECT * FROM no_index_table WHERE cat = 'A' ORDER BY val";
        let schema = rt.block_on(collect_schema(&mc_conn)).unwrap();
        let plan = rt.block_on(explain(&mc_conn, sql)).unwrap();
        let findings = run_rules(&plan, &schema);

        let ids: Vec<&str> = findings.iter().map(|f| f.id.as_str()).collect();
        // 可能触发：FULL_TABLE_SCAN, MISSING_INDEX, FILESORT
        assert!(!findings.is_empty(), "应至少触发一条规则: {:?}", ids);

        let report = DiagnosisReport::new(
            sql, "mysql", "8.0", "phase2_multi",
            findings.clone(), plan, schema,
        );
        assert!(report.total_findings >= 1);
        assert!(!report.summary.is_empty());

        rt.block_on(mc_conn.query("DROP TABLE IF EXISTS no_index_table", 10)).unwrap();
        rt.block_on(conn.query("DROP DATABASE IF EXISTS phase2_multi", 10)).unwrap();
    }

    #[test]
    fn well_indexed_query_no_findings() {
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

        rt.block_on(conn.query("CREATE DATABASE IF NOT EXISTS phase2_clean", 10)).unwrap();
        let mut cc = ConnectorConfig::from_url(
            &format!("mysql://root:root@127.0.0.1:{}/phase2_clean",
                     container.get_host_port_ipv4(3306))
        ).unwrap();
        cc.read_only = false;
        let cc_conn = rt.block_on(DatabaseConnection::connect(cc)).unwrap();

        rt.block_on(cc_conn.query(
            "CREATE TABLE indexed_table (
                id INT PRIMARY KEY AUTO_INCREMENT,
                status VARCHAR(20),
                INDEX idx_status (status)
            )", 10,
        )).unwrap();
        rt.block_on(cc_conn.query(
            "INSERT INTO indexed_table (status) VALUES ('active'), ('active'), ('inactive')", 10,
        )).unwrap();

        let sql = "SELECT * FROM indexed_table WHERE status = 'active'";
        let schema = rt.block_on(collect_schema(&cc_conn)).unwrap();
        let plan = rt.block_on(explain(&cc_conn, sql)).unwrap();
        let findings = run_rules(&plan, &schema);

        // 小表 + 有索引 → 预计不触发高危规则
        assert!(!findings.iter().any(|f| f.severity == Severity::Critical),
            "不应有 Critical 级别告警: {:?}", findings);

        let report = DiagnosisReport::new(
            sql, "mysql", "8.0", "phase2_clean",
            findings, plan, schema,
        );
        assert!(!report.summary.is_empty());

        rt.block_on(cc_conn.query("DROP TABLE IF EXISTS indexed_table", 10)).unwrap();
        rt.block_on(conn.query("DROP DATABASE IF EXISTS phase2_clean", 10)).unwrap();
    }

    #[test]
    fn report_contains_all_required_sections() {
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

        rt.block_on(conn.query("CREATE DATABASE IF NOT EXISTS phase2_rpt", 10)).unwrap();
        let mut rc = ConnectorConfig::from_url(
            &format!("mysql://root:root@127.0.0.1:{}/phase2_rpt",
                     container.get_host_port_ipv4(3306))
        ).unwrap();
        rc.read_only = false;
        let rc_conn = rt.block_on(DatabaseConnection::connect(rc)).unwrap();

        rt.block_on(rc_conn.query(
            "CREATE TABLE report_test (id INT PRIMARY KEY, data VARCHAR(100))", 10,
        )).unwrap();
        rt.block_on(rc_conn.query(
            "INSERT INTO report_test VALUES (1,'a'),(2,'b'),(3,'c')", 10,
        )).unwrap();

        let sql = "SELECT * FROM report_test";
        let version = rt.block_on(collect_version(&rc_conn)).unwrap();
        let schema = rt.block_on(collect_schema(&rc_conn)).unwrap();
        let plan = rt.block_on(explain(&rc_conn, sql)).unwrap();
        let findings = run_rules(&plan, &schema);

        let report = DiagnosisReport::new(
            sql, "mysql", &version, "phase2_rpt",
            findings, plan, schema,
        );

        // 验证报告完整性
        assert!(!report.sql.is_empty());
        assert!(!report.db_type.is_empty());
        assert!(!report.db_version.is_empty());
        assert!(!report.database_name.is_empty());
        assert!(!report.generated_at.is_empty());
        assert!(!report.summary.is_empty());
        assert!(!report.plan.node_type.is_empty());
        assert!(!report.schema.tables.is_empty());

        let json = serde_json::to_string_pretty(&report).unwrap();
        assert!(json.contains("\"sql\""));
        assert!(json.contains("\"findings\""));
        assert!(json.contains("\"plan\""));
        assert!(json.contains("\"schema\""));
        assert!(json.contains("\"summary\""));
        assert!(json.contains("\"overall_severity\""));

        rt.block_on(rc_conn.query("DROP TABLE IF EXISTS report_test", 10)).unwrap();
        rt.block_on(conn.query("DROP DATABASE IF EXISTS phase2_rpt", 10)).unwrap();
    }
}

mod postgres_phase2_tests {
    use super::*;

    #[test]
    fn pg_seq_scan_triggers_rules() {
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
            "CREATE TABLE pg_large (
                id SERIAL PRIMARY KEY,
                val INTEGER NOT NULL,
                data TEXT
            )", 10,
        )).unwrap();
        rt.block_on(conn.query(
            "INSERT INTO pg_large (val, data)
             SELECT n, 'row_' || n FROM generate_series(1, 1500) AS n", 10,
        )).unwrap();
        rt.block_on(conn.query("ANALYZE pg_large", 10)).unwrap();

        let sql = "SELECT * FROM pg_large";
        let version = rt.block_on(collect_version(&conn)).unwrap();
        let schema = rt.block_on(collect_schema(&conn)).unwrap();
        let plan = rt.block_on(explain(&conn, sql)).unwrap();
        let findings = run_rules(&plan, &schema);

        assert!(!findings.is_empty(), "应对全表扫描产生告警: {:?}", findings);

        let report = DiagnosisReport::new(
            sql, "postgresql", &version, "test",
            findings, plan, schema,
        );
        assert!(report.total_findings >= 1);
        assert!(!report.summary.is_empty());

        rt.block_on(conn.query("DROP TABLE IF EXISTS pg_large", 10)).unwrap();
    }

    #[test]
    fn pg_index_scan_produces_clean_report() {
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
            "CREATE TABLE pg_indexed (
                id SERIAL PRIMARY KEY,
                name TEXT NOT NULL,
                status TEXT DEFAULT 'active'
            )", 10,
        )).unwrap();
        rt.block_on(conn.query(
            "CREATE INDEX idx_status ON pg_indexed(status)", 10,
        )).unwrap();
        rt.block_on(conn.query(
            "INSERT INTO pg_indexed (name, status)
             SELECT 'user_' || n, CASE WHEN n % 2 = 0 THEN 'active' ELSE 'inactive' END
             FROM generate_series(1, 200) AS n", 10,
        )).unwrap();
        rt.block_on(conn.query("ANALYZE pg_indexed", 10)).unwrap();

        let sql = "SELECT * FROM pg_indexed WHERE status = 'active'";
        let version = rt.block_on(collect_version(&conn)).unwrap();
        let schema = rt.block_on(collect_schema(&conn)).unwrap();
        let plan = rt.block_on(explain(&conn, sql)).unwrap();
        let findings = run_rules(&plan, &schema);

        // 有索引 → 不应有问题
        let critical_high: Vec<_> = findings.iter()
            .filter(|f| f.severity <= Severity::High)
            .collect();
        assert!(critical_high.is_empty(),
            "有索引的表不应产生高危告警: {:?}", critical_high);

        let report = DiagnosisReport::new(
            sql, "postgresql", &version, "test",
            findings, plan, schema,
        );
        assert!(!report.summary.is_empty());

        rt.block_on(conn.query("DROP TABLE IF EXISTS pg_indexed", 10)).unwrap();
    }

    #[test]
    fn pg_nested_loop_risk() {
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
            "CREATE TABLE nl1 (id SERIAL PRIMARY KEY, val INT)", 10,
        )).unwrap();
        rt.block_on(conn.query(
            "CREATE TABLE nl2 (id SERIAL PRIMARY KEY, ref_id INT)", 10,
        )).unwrap();
        rt.block_on(conn.query(
            "INSERT INTO nl1 (val) SELECT n FROM generate_series(1, 1000) AS n", 10,
        )).unwrap();
        rt.block_on(conn.query(
            "INSERT INTO nl2 (ref_id) SELECT (n % 100) + 1 FROM generate_series(1, 500) AS n", 10,
        )).unwrap();
        rt.block_on(conn.query("ANALYZE nl1", 10)).unwrap();
        rt.block_on(conn.query("ANALYZE nl2", 10)).unwrap();

        let sql = "SELECT * FROM nl1 JOIN nl2 ON nl1.id = nl2.ref_id";
        let schema = rt.block_on(collect_schema(&conn)).unwrap();
        let plan = rt.block_on(explain(&conn, sql)).unwrap();

        // PG 可能选择 Hash Join 或 Nested Loop
        let findings = run_rules(&plan, &schema);

        let version = rt.block_on(collect_version(&conn)).unwrap();
        let report = DiagnosisReport::new(
            sql, "postgresql", &version, "test",
            findings, plan, schema,
        );
        // PG 可能选择 Hash Join 而非 Nested Loop，这里仅验证报告生成不崩溃
        assert!(!report.summary.is_empty());

        rt.block_on(conn.query("DROP TABLE IF EXISTS nl1, nl2", 10)).unwrap();
    }
}
