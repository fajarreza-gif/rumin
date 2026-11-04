use ai_rumin::graph::CodeGraph;
use ai_rumin::indexer::Indexer;
use ai_rumin::parsers::*;
use ai_rumin::types::EntityType;
use std::path::Path;

#[test]
fn test_yaml_manifest_filtering_and_safeguards() {
    // 1. CI/CD pipelines
    assert!(is_supported_yaml_file(".github/workflows/ci.yml", "ci.yml"));
    assert!(is_supported_yaml_file(".github/workflows/release.yaml", "release.yaml"));
    assert!(is_supported_yaml_file(".gitlab-ci.yml", ".gitlab-ci.yml"));
    assert!(is_supported_yaml_file(".gitlab-ci.yaml", ".gitlab-ci.yaml"));
    assert!(is_supported_yaml_file("azure-pipelines.yml", "azure-pipelines.yml"));
    assert!(is_supported_yaml_file("bitbucket-pipelines.yml", "bitbucket-pipelines.yml"));

    // 2. Docker Compose
    assert!(is_supported_yaml_file("docker-compose.yml", "docker-compose.yml"));
    assert!(is_supported_yaml_file("docker-compose.prod.yaml", "docker-compose.prod.yaml"));
    assert!(is_supported_yaml_file("compose.yml", "compose.yml"));
    assert!(is_supported_yaml_file("compose.yaml", "compose.yaml"));

    // 3. OpenAPI & Swagger
    assert!(is_supported_yaml_file("openapi.yaml", "openapi.yaml"));
    assert!(is_supported_yaml_file("openapi.yml", "openapi.yml"));
    assert!(is_supported_yaml_file("swagger.yaml", "swagger.yaml"));
    assert!(is_supported_yaml_file("api.openapi.yaml", "api.openapi.yaml"));
    assert!(is_supported_yaml_file("petstore.swagger.yml", "petstore.swagger.yml"));

    // 4. Cloud & Helm
    assert!(is_supported_yaml_file("serverless.yml", "serverless.yml"));
    assert!(is_supported_yaml_file("serverless.yaml", "serverless.yaml"));
    assert!(is_supported_yaml_file("Chart.yaml", "Chart.yaml"));
    assert!(is_supported_yaml_file("kustomization.yaml", "kustomization.yaml"));

    // 5. Exclusions: lockfiles and localization noise
    assert!(!is_supported_yaml_file("pnpm-lock.yaml", "pnpm-lock.yaml"));
    assert!(!is_supported_yaml_file("yarn.lock", "yarn.lock"));
    assert!(!is_supported_yaml_file("package-lock.yaml", "package-lock.yaml"));
    assert!(!is_supported_yaml_file("conda-lock.yml", "conda-lock.yml"));
    assert!(!is_supported_yaml_file("locales/en.yml", "en.yml"));
    assert!(!is_supported_yaml_file("translations/fr.yaml", "fr.yaml"));
    assert!(!is_supported_yaml_file("fixtures/mock_data.yaml", "mock_data.yaml"));

    // Indexer::is_supported_file_path integration checks
    assert!(Indexer::is_supported_file_path(Path::new(".github/workflows/test.yml"), "yml"));
    assert!(Indexer::is_supported_file_path(Path::new("docker-compose.yml"), "yml"));
    assert!(Indexer::is_supported_file_path(Path::new("openapi.yaml"), "yaml"));
    assert!(!Indexer::is_supported_file_path(Path::new("pnpm-lock.yaml"), "yaml"));
    assert!(!Indexer::is_supported_file_path(Path::new("locales/en.yml"), "yml"));
}

#[test]
fn test_parse_github_actions_workflow() {
    let mut graph = CodeGraph::new();
    let content = r#"
name: CI & Build Pipeline
on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  lint:
    name: Code Linting
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo clippy

  test:
    name: Test Suite
    runs-on: ubuntu-22.04
    needs: [lint]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test

  deploy:
    runs-on: ubuntu-latest
    needs: [test]
    steps:
      - uses: aws-actions/configure-aws-credentials@v3
"#;

    parse_yaml_manifest(".github/workflows/ci.yml", content, &mut graph);

    // Workflow module
    let wf = graph.get_entity(".github/workflows/ci.yml::workflow").expect("Workflow entity");
    assert_eq!(wf.name, "CI & Build Pipeline");
    assert_eq!(wf.entity_type, EntityType::Module);
    assert_eq!(wf.signature.as_deref(), Some("workflow: CI & Build Pipeline"));
    assert!(wf.metadata.get("triggers").is_some());

    // Jobs as Targets
    let lint_job = graph.get_entity(".github/workflows/ci.yml::job::lint").expect("Lint job");
    assert_eq!(lint_job.name, "Code Linting");
    assert_eq!(lint_job.entity_type, EntityType::Target);
    assert_eq!(lint_job.signature.as_deref(), Some("job lint: (runs-on: ubuntu-latest)"));
    assert!(graph.has_edge(".github/workflows/ci.yml::workflow", ".github/workflows/ci.yml::job::lint"));

    let test_job = graph.get_entity(".github/workflows/ci.yml::job::test").expect("Test job");
    assert_eq!(test_job.signature.as_deref(), Some("job test: (runs-on: ubuntu-22.04)"));

    // Job needs dependency (test -> lint)
    assert!(graph.has_edge(".github/workflows/ci.yml::job::test", ".github/workflows/ci.yml::job::lint"));

    // Action dependencies
    let action_checkout = graph.get_entity("action::actions/checkout@v4").expect("Checkout action");
    assert_eq!(action_checkout.entity_type, EntityType::Module);
    assert!(graph.has_edge(".github/workflows/ci.yml::job::lint", "action::actions/checkout@v4"));
    assert!(graph.has_edge(".github/workflows/ci.yml::job::test", "action::actions/checkout@v4"));

    let _action_rust = graph.get_entity("action::dtolnay/rust-toolchain@stable").expect("Rust toolchain action");
    assert!(graph.has_edge(".github/workflows/ci.yml::job::test", "action::dtolnay/rust-toolchain@stable"));
}

#[test]
fn test_parse_docker_compose() {
    let mut graph = CodeGraph::new();
    let content = r#"
version: '3.8'
services:
  web:
    image: node:20-alpine
    ports:
      - "3000:3000"
    depends_on:
      - api
      - redis

  api:
    image: my-company/backend:latest
    ports:
      - "8080:8080"
    depends_on:
      - db

  redis:
    image: redis:7-alpine

  db:
    image: postgres:16
    ports:
      - "5432:5432"
"#;

    parse_yaml_manifest("docker-compose.yml", content, &mut graph);

    let compose = graph.get_entity("docker-compose.yml::compose").expect("Compose root");
    assert_eq!(compose.entity_type, EntityType::Module);

    // Services
    let web_svc = graph.get_entity("docker-compose.yml::service::web").expect("Web service");
    assert_eq!(web_svc.entity_type, EntityType::Module);
    assert!(web_svc.signature.as_ref().unwrap().contains("image: node:20-alpine"));
    assert!(web_svc.signature.as_ref().unwrap().contains("ports: [3000:3000]"));
    assert!(graph.has_edge("docker-compose.yml::compose", "docker-compose.yml::service::web"));

    assert!(graph.get_entity("docker-compose.yml::service::api").is_some());
    assert!(graph.get_entity("docker-compose.yml::service::redis").is_some());
    assert!(graph.get_entity("docker-compose.yml::service::db").is_some());

    // Inter-service DependsOn edges
    assert!(graph.has_edge("docker-compose.yml::service::web", "docker-compose.yml::service::api"));
    assert!(graph.has_edge("docker-compose.yml::service::web", "docker-compose.yml::service::redis"));
    assert!(graph.has_edge("docker-compose.yml::service::api", "docker-compose.yml::service::db"));
}

#[test]
fn test_parse_openapi_yaml_specification() {
    let mut graph = CodeGraph::new();
    let content = r#"
openapi: 3.1.0
info:
  title: Order Processing Service
  version: 3.0.0
  description: Distributed order dispatch engine

paths:
  /v1/orders:
    get:
      operationId: getOrders
      summary: Retrieve pending orders
      description: Returns list of active orders
    post:
      operationId: placeOrder
      summary: Submit a new order

  /v1/orders/{orderId}:
    delete:
      summary: Cancel existing order

components:
  schemas:
    OrderPayload:
      type: object
    OrderStatus:
      type: string
"#;

    parse_yaml_manifest("specs/orders.openapi.yaml", content, &mut graph);

    // API entity
    let api = graph.get_entity("specs/orders.openapi.yaml::api").expect("API root");
    assert_eq!(api.name, "Order Processing Service");
    assert_eq!(api.signature.as_deref(), Some("OpenAPI Order Processing Service (3.0.0)"));
    assert_eq!(api.docstring.as_deref(), Some("Distributed order dispatch engine"));

    // Route Endpoints
    let get_orders = graph.get_entity("specs/orders.openapi.yaml::endpoint::GET /v1/orders").expect("GET orders");
    assert_eq!(get_orders.name, "getOrders");
    assert_eq!(get_orders.entity_type, EntityType::Function);
    assert_eq!(get_orders.signature.as_deref(), Some("GET /v1/orders - Retrieve pending orders"));
    assert!(graph.has_edge("specs/orders.openapi.yaml::api", "specs/orders.openapi.yaml::endpoint::GET /v1/orders"));

    let post_orders = graph.get_entity("specs/orders.openapi.yaml::endpoint::POST /v1/orders").expect("POST orders");
    assert_eq!(post_orders.name, "placeOrder");
    assert!(graph.has_edge("specs/orders.openapi.yaml::api", "specs/orders.openapi.yaml::endpoint::POST /v1/orders"));

    let del_order = graph.get_entity("specs/orders.openapi.yaml::endpoint::DELETE /v1/orders/{orderId}").expect("DELETE order");
    assert_eq!(del_order.signature.as_deref(), Some("DELETE /v1/orders/{orderId} - Cancel existing order"));

    // Schemas
    let schema = graph.get_entity("specs/orders.openapi.yaml::schema::OrderPayload").expect("Order schema");
    assert_eq!(schema.entity_type, EntityType::Struct);
    assert!(graph.has_edge("specs/orders.openapi.yaml::api", "specs/orders.openapi.yaml::schema::OrderPayload"));
}

#[test]
fn test_parse_serverless_and_helm() {
    let mut graph = CodeGraph::new();

    let serverless_content = r#"
service: billing-service
provider:
  name: aws
  runtime: nodejs20.x

functions:
  chargeCustomer:
    handler: src/handlers/billing.charge
  refundCustomer:
    handler: src/handlers/billing.refund
"#;

    parse_yaml_manifest("serverless.yml", serverless_content, &mut graph);
    let sls = graph.get_entity("serverless.yml::service").expect("Serverless service");
    assert_eq!(sls.name, "billing-service");
    assert_eq!(sls.entity_type, EntityType::Module);

    let fn_charge = graph.get_entity("serverless.yml::function::chargeCustomer").expect("Lambda charge");
    assert_eq!(fn_charge.entity_type, EntityType::Function);
    assert_eq!(fn_charge.signature.as_deref(), Some("lambda chargeCustomer (handler: src/handlers/billing.charge)"));
    assert!(graph.has_edge("serverless.yml::service", "serverless.yml::function::chargeCustomer"));

    let helm_content = r#"
apiVersion: v2
name: my-app-chart
version: 1.5.0
dependencies:
  - name: postgresql
    version: 12.1.0
  - name: redis
    version: 17.0.0
"#;

    parse_yaml_manifest("Chart.yaml", helm_content, &mut graph);
    let chart = graph.get_entity("Chart.yaml::chart").expect("Chart entity");
    assert_eq!(chart.name, "my-app-chart");
    assert_eq!(chart.signature.as_deref(), Some("helm chart my-app-chart (1.5.0)"));

    assert!(graph.has_edge("Chart.yaml::chart", "Chart.yaml::dep::postgresql"));
    assert!(graph.has_edge("Chart.yaml::chart", "Chart.yaml::dep::redis"));
}

#[tokio::test]
async fn test_end_to_end_indexer_with_yaml_manifests() {
    let temp_dir = std::env::temp_dir().join(format!("rumin_yaml_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(temp_dir.join(".github/workflows")).unwrap();

    let wf_path = temp_dir.join(".github/workflows/ci.yml");
    std::fs::write(&wf_path, "name: Test Workflow\njobs:\n  build:\n    runs-on: ubuntu-latest\n").unwrap();

    let compose_path = temp_dir.join("docker-compose.yml");
    std::fs::write(&compose_path, "version: '3'\nservices:\n  redis:\n    image: redis:alpine\n").unwrap();

    let lock_path = temp_dir.join("pnpm-lock.yaml");
    std::fs::write(&lock_path, "lockfileVersion: '6.0'\ndependencies:\n  foo: 1.0.0\n").unwrap();

    let mut graph = CodeGraph::new();
    let sidecar = ai_rumin::sidecar::SidecarClient::new("dummy.py");

    let res_wf = Indexer::index_single_file(&temp_dir, &wf_path, &mut graph, &sidecar).await.unwrap();
    assert!(res_wf, "GitHub actions workflow must be indexed");
    assert!(graph.get_entity(".github/workflows/ci.yml::workflow").is_some());
    assert!(graph.get_entity(".github/workflows/ci.yml::job::build").is_some());

    let res_compose = Indexer::index_single_file(&temp_dir, &compose_path, &mut graph, &sidecar).await.unwrap();
    assert!(res_compose, "docker-compose.yml must be indexed");
    assert!(graph.get_entity("docker-compose.yml::service::redis").is_some());

    let res_lock = Indexer::index_single_file(&temp_dir, &lock_path, &mut graph, &sidecar).await.unwrap();
    assert!(!res_lock, "pnpm-lock.yaml must be rejected");

    let _ = std::fs::remove_dir_all(&temp_dir);
}
