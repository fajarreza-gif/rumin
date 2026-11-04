use ai_rumin::graph::CodeGraph;
use ai_rumin::indexer::Indexer;
use ai_rumin::parsers::*;
use ai_rumin::types::EntityType;

#[test]
fn test_json_manifest_filtering_and_safeguards() {
    // Supported manifest and schema files
    assert!(is_supported_json_file("package.json"));
    assert!(is_supported_json_file("tsconfig.json"));
    assert!(is_supported_json_file("tsconfig.app.json"));
    assert!(is_supported_json_file("tsconfig.base.json"));
    assert!(is_supported_json_file("jsconfig.json"));
    assert!(is_supported_json_file("composer.json"));
    assert!(is_supported_json_file("deno.json"));
    assert!(is_supported_json_file("deno.jsonc"));
    assert!(is_supported_json_file("openapi.json"));
    assert!(is_supported_json_file("swagger.json"));
    assert!(is_supported_json_file("api.openapi.json"));
    assert!(is_supported_json_file("service.swagger.json"));
    assert!(is_supported_json_file("customer.schema.json"));

    // Explicit exclusions for lockfiles and generic JSON noise
    assert!(!is_supported_json_file("package-lock.json"));
    assert!(!is_supported_json_file("npm-shrinkwrap.json"));
    assert!(!is_supported_json_file("bun.lock.json"));
    assert!(!is_supported_json_file("data.json"));
    assert!(!is_supported_json_file("payload.json"));
    assert!(!is_supported_json_file("users_dump.json"));
    assert!(!is_supported_json_file("test_fixture.json"));

    // Indexer::is_supported_file checks
    assert!(Indexer::is_supported_file("package.json", "json"));
    assert!(Indexer::is_supported_file("tsconfig.json", "json"));
    assert!(Indexer::is_supported_file("api.openapi.json", "json"));
    assert!(!Indexer::is_supported_file("package-lock.json", "json"));
    assert!(!Indexer::is_supported_file("payload.json", "json"));
}

#[test]
fn test_parse_package_json() {
    let mut graph = CodeGraph::new();
    let content = r#"{
        "name": "my-express-app",
        "version": "1.2.3",
        "description": "Enterprise Express microservice",
        "license": "MIT",
        "main": "./dist/index.js",
        "scripts": {
            "build": "tsc -b",
            "test": "jest --coverage",
            "start": "node dist/index.js"
        },
        "dependencies": {
            "express": "^4.19.2",
            "zod": "^3.22.4"
        },
        "devDependencies": {
            "typescript": "^5.4.5",
            "@types/express": "^4.17.21"
        }
    }"#;

    parse_json_manifest("package.json", content, &mut graph);

    // Root package entity
    let pkg = graph.get_entity("package.json::package").expect("Package entity must exist");
    assert_eq!(pkg.name, "my-express-app");
    assert_eq!(pkg.entity_type, EntityType::Module);
    assert_eq!(pkg.signature.as_deref(), Some("package my-express-app@1.2.3"));
    assert_eq!(pkg.docstring.as_deref(), Some("Enterprise Express microservice"));
    assert_eq!(pkg.metadata.get("license").map(String::as_str), Some("MIT"));

    // Scripts as Targets
    let build_script = graph.get_entity("package.json::script::build").expect("Build script entity");
    assert_eq!(build_script.entity_type, EntityType::Target);
    assert_eq!(build_script.signature.as_deref(), Some("npm run build: tsc -b"));
    assert!(graph.has_edge("package.json::package", "package.json::script::build"));

    let test_script = graph.get_entity("package.json::script::test").expect("Test script entity");
    assert_eq!(test_script.entity_type, EntityType::Target);

    // Dependencies as Modules with DependsOn edge
    let dep_express = graph.get_entity("package.json::dep::express").expect("Express dep");
    assert_eq!(dep_express.entity_type, EntityType::Module);
    assert_eq!(dep_express.signature.as_deref(), Some("dep: express (^4.19.2)"));
    assert!(graph.has_edge("package.json::package", "package.json::dep::express"));

    // Dev dependencies
    let dev_ts = graph.get_entity("package.json::dev_dep::typescript").expect("Typescript dev dep");
    assert_eq!(dev_ts.entity_type, EntityType::Module);
    assert!(graph.has_edge("package.json::package", "package.json::dev_dep::typescript"));

    // Entry point reference
    let main_entry = graph.get_entity("package.json::entry::main").expect("Main entry point");
    assert_eq!(main_entry.entity_type, EntityType::Variable);
    assert!(graph.has_edge("package.json::package", "package.json::entry::main"));
}

#[test]
fn test_parse_tsconfig_json() {
    let mut graph = CodeGraph::new();
    let content = r#"{
        "extends": "./tsconfig.base.json",
        "compilerOptions": {
            "target": "ES2022",
            "module": "NodeNext",
            "paths": {
                "@components/*": ["src/components/*"],
                "@utils/*": ["src/utils/*"]
            }
        }
    }"#;

    parse_json_manifest("tsconfig.json", content, &mut graph);

    let config = graph.get_entity("tsconfig.json::config").expect("tsconfig entity");
    assert_eq!(config.entity_type, EntityType::Target);
    assert_eq!(config.signature.as_deref(), Some("tsconfig (target: ES2022, module: NodeNext)"));

    // Extends edge
    assert!(graph.has_edge("tsconfig.json::config", "tsconfig.json::extends::./tsconfig.base.json"));

    // Path aliases
    let alias = graph.get_entity("tsconfig.json::alias::@components/*").expect("Path alias entity");
    assert_eq!(alias.entity_type, EntityType::Variable);
    assert!(alias.signature.as_ref().unwrap().contains("@components/* -> [src/components/*]"));
    assert!(graph.has_edge("tsconfig.json::config", "tsconfig.json::alias::@components/*"));
}

#[test]
fn test_parse_openapi_specification() {
    let mut graph = CodeGraph::new();
    let content = r#"{
        "openapi": "3.0.3",
        "info": {
            "title": "Payment Gateway API",
            "version": "2.4.0",
            "description": "High-throughput transaction handling API"
        },
        "paths": {
            "/v1/payments": {
                "get": {
                    "operationId": "listPayments",
                    "summary": "List all transactions",
                    "description": "Returns paginated list of settled payments"
                },
                "post": {
                    "operationId": "createPayment",
                    "summary": "Initiate a charge",
                    "description": "Creates a new card or bank charge"
                }
            },
            "/v1/payments/{paymentId}": {
                "get": {
                    "summary": "Retrieve payment by ID"
                }
            }
        },
        "components": {
            "schemas": {
                "Payment": {
                    "type": "object"
                },
                "ChargeRequest": {
                    "type": "object"
                }
            }
        }
    }"#;

    parse_json_manifest("api/openapi.json", content, &mut graph);

    // API entity
    let api = graph.get_entity("api/openapi.json::api").expect("API entity");
    assert_eq!(api.name, "Payment Gateway API");
    assert_eq!(api.signature.as_deref(), Some("OpenAPI Payment Gateway API (2.4.0)"));
    assert_eq!(api.docstring.as_deref(), Some("High-throughput transaction handling API"));

    // Endpoints
    let get_payments = graph.get_entity("api/openapi.json::endpoint::GET /v1/payments").expect("GET endpoint");
    assert_eq!(get_payments.name, "listPayments");
    assert_eq!(get_payments.entity_type, EntityType::Function);
    assert_eq!(get_payments.signature.as_deref(), Some("GET /v1/payments - List all transactions"));
    assert_eq!(get_payments.docstring.as_deref(), Some("Returns paginated list of settled payments"));
    assert!(graph.has_edge("api/openapi.json::api", "api/openapi.json::endpoint::GET /v1/payments"));

    let post_payments = graph.get_entity("api/openapi.json::endpoint::POST /v1/payments").expect("POST endpoint");
    assert_eq!(post_payments.name, "createPayment");
    assert!(graph.has_edge("api/openapi.json::api", "api/openapi.json::endpoint::POST /v1/payments"));

    let get_single = graph.get_entity("api/openapi.json::endpoint::GET /v1/payments/{paymentId}").expect("Single endpoint");
    assert_eq!(get_single.name, "GET /v1/payments/{paymentId}");
    assert_eq!(get_single.signature.as_deref(), Some("GET /v1/payments/{paymentId} - Retrieve payment by ID"));

    // Schemas
    let schema_payment = graph.get_entity("api/openapi.json::schema::Payment").expect("Payment schema");
    assert_eq!(schema_payment.entity_type, EntityType::Struct);
    assert!(graph.has_edge("api/openapi.json::api", "api/openapi.json::schema::Payment"));
}

#[test]
fn test_parse_json_schema() {
    let mut graph = CodeGraph::new();
    let content = r#"{
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "CustomerProfile",
        "description": "Customer entity definition",
        "properties": {
            "id": { "type": "string" },
            "age": { "type": "integer" },
            "isActive": { "type": "boolean" }
        }
    }"#;

    parse_json_manifest("schemas/customer.schema.json", content, &mut graph);

    let schema = graph.get_entity("schemas/customer.schema.json::schema").expect("Schema entity");
    assert_eq!(schema.name, "CustomerProfile");
    assert_eq!(schema.entity_type, EntityType::Struct);
    assert_eq!(schema.signature.as_deref(), Some("schema: CustomerProfile"));

    let id_prop = graph.get_entity("schemas/customer.schema.json::prop::id").expect("ID property");
    assert_eq!(id_prop.entity_type, EntityType::Variable);
    assert_eq!(id_prop.signature.as_deref(), Some("id: string"));
    assert!(graph.has_edge("schemas/customer.schema.json::schema", "schemas/customer.schema.json::prop::id"));
}

#[test]
fn test_parse_composer_and_deno_json() {
    let mut graph = CodeGraph::new();

    let composer_content = r#"{
        "name": "laravel/laravel",
        "description": "The Laravel Framework",
        "require": {
            "php": "^8.2",
            "guzzlehttp/guzzle": "^7.8"
        }
    }"#;

    parse_json_manifest("composer.json", composer_content, &mut graph);
    let pkg = graph.get_entity("composer.json::package").expect("composer package");
    assert_eq!(pkg.name, "laravel/laravel");
    assert!(graph.has_edge("composer.json::package", "composer.json::dep::guzzlehttp/guzzle"));

    let deno_content = r#"{
        "name": "fresh-project",
        "tasks": {
            "start": "deno run -A main.ts"
        },
        "imports": {
            "preact": "https://esm.sh/preact@10.19.6"
        }
    }"#;

    parse_json_manifest("deno.json", deno_content, &mut graph);
    let deno_mod = graph.get_entity("deno.json::config").expect("deno module");
    assert_eq!(deno_mod.name, "fresh-project");
    assert!(graph.has_edge("deno.json::config", "deno.json::task::start"));
    assert!(graph.has_edge("deno.json::config", "deno.json::import::preact"));
}

#[tokio::test]
async fn test_end_to_end_indexer_with_json_manifests() {
    let temp_dir = std::env::temp_dir().join(format!("rumin_json_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let pkg_path = temp_dir.join("package.json");
    std::fs::write(&pkg_path, r#"{"name":"test-service","version":"1.0.0","dependencies":{"tokio":"1.0"}}"#).unwrap();

    let lock_path = temp_dir.join("package-lock.json");
    std::fs::write(&lock_path, r#"{"name":"test-service","lockfileVersion":3}"#).unwrap();

    let openapi_path = temp_dir.join("openapi.json");
    std::fs::write(&openapi_path, r#"{"openapi":"3.0.0","info":{"title":"User API","version":"1.0"},"paths":{"/users":{"get":{"summary":"get users"}}}}"#).unwrap();

    let mut graph = CodeGraph::new();
    let sidecar = ai_rumin::sidecar::SidecarClient::new("dummy.py");

    let res_pkg = Indexer::index_single_file(&temp_dir, &pkg_path, &mut graph, &sidecar).await.unwrap();
    assert!(res_pkg, "package.json must be indexed");
    assert!(graph.get_entity("package.json::package").is_some());

    let res_lock = Indexer::index_single_file(&temp_dir, &lock_path, &mut graph, &sidecar).await.unwrap();
    assert!(!res_lock, "package-lock.json must be ignored");

    let res_api = Indexer::index_single_file(&temp_dir, &openapi_path, &mut graph, &sidecar).await.unwrap();
    assert!(res_api, "openapi.json must be indexed");
    assert!(graph.get_entity("openapi.json::api").is_some());

    let _ = std::fs::remove_dir_all(&temp_dir);
}

