use std::path::Path;
use serde_json::Value;
use crate::graph::CodeGraph;
use crate::types::{CodeEntity, EdgeKind, EntityType};

/// Maximum file size for JSON manifest parsing (2MB) to prevent memory exhaustion
const MAX_JSON_MANIFEST_SIZE: usize = 2 * 1024 * 1024;

/// Check if a given filename corresponds to a supported architectural JSON manifest or schema.
/// This intentionally avoids generic JSON files (lockfiles, test payloads, caches, logs).
pub fn is_supported_json_file(file_name: &str) -> bool {
    // Explicit exclusions for lockfiles and noise
    if file_name == "package-lock.json"
        || file_name == "pnpm-lock.yaml"
        || file_name.ends_with("-lock.json")
        || file_name.ends_with(".lock.json")
    {
        return false;
    }

    // 1. Node.js & JavaScript ecosystem manifests
    if file_name == "package.json" {
        return true;
    }

    // 2. TypeScript / JavaScript compiler configurations
    if file_name == "tsconfig.json"
        || file_name == "jsconfig.json"
        || (file_name.starts_with("tsconfig.") && file_name.ends_with(".json"))
        || (file_name.starts_with("jsconfig.") && file_name.ends_with(".json"))
    {
        return true;
    }

    // 3. PHP Composer manifests
    if file_name == "composer.json" {
        return true;
    }

    // 4. Deno manifests
    if file_name == "deno.json" || file_name == "deno.jsonc" {
        return true;
    }

    // 5. OpenAPI & Swagger API specifications
    if file_name == "openapi.json"
        || file_name == "swagger.json"
        || file_name.ends_with(".openapi.json")
        || file_name.ends_with(".swagger.json")
        || (file_name.contains("openapi") && file_name.ends_with(".json"))
        || (file_name.contains("swagger") && file_name.ends_with(".json"))
    {
        return true;
    }

    // 6. JSON Schemas
    if file_name.ends_with(".schema.json") {
        return true;
    }

    false
}

/// Parse supported JSON manifests, specifications, and configurations into the code graph.
pub fn parse_json_manifest(rel_path: &str, content: &str, graph: &mut CodeGraph) {
    if content.len() > MAX_JSON_MANIFEST_SIZE {
        return;
    }

    let parsed: Value = match serde_json::from_str(content) {
        Ok(v) => v,
        Err(_) => return,
    };

    let obj = match parsed.as_object() {
        Some(o) => o,
        None => return,
    };

    let path = Path::new(rel_path);
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    // OpenAPI / Swagger takes precedence if detection matches structure or filename
    let is_openapi = obj.contains_key("openapi")
        || obj.contains_key("swagger")
        || file_name.contains("openapi")
        || file_name.contains("swagger");

    if is_openapi {
        parse_openapi_json(rel_path, obj, graph);
        return;
    }

    if file_name == "package.json" {
        parse_package_json(rel_path, obj, graph);
        return;
    }

    if file_name == "tsconfig.json"
        || file_name == "jsconfig.json"
        || file_name.starts_with("tsconfig.")
        || file_name.starts_with("jsconfig.")
    {
        parse_tsconfig_json(rel_path, file_name, obj, graph);
        return;
    }

    if file_name == "composer.json" {
        parse_composer_json(rel_path, obj, graph);
        return;
    }

    if file_name.ends_with(".schema.json") || obj.contains_key("$schema") {
        parse_json_schema(rel_path, file_name, obj, graph);
        return;
    }

    if file_name == "deno.json" || file_name == "deno.jsonc" {
        parse_deno_json(rel_path, obj, graph);
    }
}

/// Parse `package.json` into Module, scripts (Targets), and dependencies (Modules with DependsOn).
fn parse_package_json(rel_path: &str, obj: &serde_json::Map<String, Value>, graph: &mut CodeGraph) {
    let pkg_name = obj
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("unnamed-package");
    let pkg_ver = obj
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("0.0.0");
    let desc = obj.get("description").and_then(|v| v.as_str());

    let pkg_id = format!("{}::package", rel_path);
    let mut pkg_entity = CodeEntity::new(&pkg_id, pkg_name, EntityType::Module, rel_path, 1, 1);
    pkg_entity.signature = Some(format!("package {}@{}", pkg_name, pkg_ver));
    if let Some(d) = desc {
        pkg_entity.docstring = Some(d.to_string());
    }
    if let Some(lic) = obj.get("license").and_then(|v| v.as_str()) {
        pkg_entity.metadata.insert("license".to_string(), lic.to_string());
    }
    graph.add_entity(pkg_entity);

    // 1. Scripts -> Targets
    if let Some(scripts) = obj.get("scripts").and_then(|v| v.as_object()) {
        for (script_name, script_val) in scripts {
            let cmd = script_val.as_str().unwrap_or("");
            let script_id = format!("{}::script::{}", rel_path, script_name);
            let mut script_entity =
                CodeEntity::new(&script_id, script_name, EntityType::Target, rel_path, 1, 1);
            script_entity.signature = Some(format!("npm run {}: {}", script_name, cmd));
            graph.add_entity(script_entity);
            graph.add_edge(&pkg_id, &script_id, EdgeKind::Contains);
        }
    }

    // 2. Production dependencies -> Modules with DependsOn
    if let Some(deps) = obj.get("dependencies").and_then(|v| v.as_object()) {
        for (dep_name, dep_ver_val) in deps {
            let dep_ver = dep_ver_val.as_str().unwrap_or("*");
            let dep_id = format!("{}::dep::{}", rel_path, dep_name);
            let mut dep_entity =
                CodeEntity::new(&dep_id, dep_name, EntityType::Module, rel_path, 1, 1);
            dep_entity.signature = Some(format!("dep: {} ({})", dep_name, dep_ver));
            graph.add_entity(dep_entity);
            graph.add_edge(&pkg_id, &dep_id, EdgeKind::DependsOn);
        }
    }

    // 3. Dev dependencies -> Modules with DependsOn
    if let Some(dev_deps) = obj.get("devDependencies").and_then(|v| v.as_object()) {
        for (dep_name, dep_ver_val) in dev_deps {
            let dep_ver = dep_ver_val.as_str().unwrap_or("*");
            let dep_id = format!("{}::dev_dep::{}", rel_path, dep_name);
            let mut dep_entity =
                CodeEntity::new(&dep_id, dep_name, EntityType::Module, rel_path, 1, 1);
            dep_entity.signature = Some(format!("dev-dep: {} ({})", dep_name, dep_ver));
            graph.add_entity(dep_entity);
            graph.add_edge(&pkg_id, &dep_id, EdgeKind::DependsOn);
        }
    }

    // 4. Entry points (main, module, types) -> recorded as metadata on pkg
    if let Some(main) = obj.get("main").and_then(|v| v.as_str()) {
        let entry_id = format!("{}::entry::main", rel_path);
        let mut entry_entity = CodeEntity::new(&entry_id, main, EntityType::Variable, rel_path, 1, 1);
        entry_entity.signature = Some(format!("main: {}", main));
        graph.add_entity(entry_entity);
        graph.add_edge(&pkg_id, &entry_id, EdgeKind::References);
    }
}

/// Parse `tsconfig.json` into Target configuration entity and compiler path aliases.
fn parse_tsconfig_json(
    rel_path: &str,
    file_name: &str,
    obj: &serde_json::Map<String, Value>,
    graph: &mut CodeGraph,
) {
    let config_id = format!("{}::config", rel_path);
    let mut config_entity =
        CodeEntity::new(&config_id, file_name, EntityType::Target, rel_path, 1, 1);

    let target_opt = obj
        .get("compilerOptions")
        .and_then(|v| v.as_object())
        .and_then(|co| co.get("target"))
        .and_then(|t| t.as_str())
        .unwrap_or("default");

    let module_opt = obj
        .get("compilerOptions")
        .and_then(|v| v.as_object())
        .and_then(|co| co.get("module"))
        .and_then(|m| m.as_str())
        .unwrap_or("default");

    config_entity.signature = Some(format!("tsconfig (target: {}, module: {})", target_opt, module_opt));
    graph.add_entity(config_entity);

    // Extends
    if let Some(ext) = obj.get("extends").and_then(|v| v.as_str()) {
        let ext_id = format!("{}::extends::{}", rel_path, ext);
        let mut ext_entity = CodeEntity::new(&ext_id, ext, EntityType::Module, rel_path, 1, 1);
        ext_entity.signature = Some(format!("extends: {}", ext));
        graph.add_entity(ext_entity);
        graph.add_edge(&config_id, &ext_id, EdgeKind::DependsOn);
    }

    // CompilerOptions paths (module aliases)
    if let Some(paths) = obj
        .get("compilerOptions")
        .and_then(|v| v.as_object())
        .and_then(|co| co.get("paths"))
        .and_then(|p| p.as_object())
    {
        for (alias, targets_val) in paths {
            let alias_id = format!("{}::alias::{}", rel_path, alias);
            let mut alias_entity =
                CodeEntity::new(&alias_id, alias, EntityType::Variable, rel_path, 1, 1);
            let targets_str = if let Some(arr) = targets_val.as_array() {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            } else {
                targets_val.to_string()
            };
            alias_entity.signature = Some(format!("path alias: {} -> [{}]", alias, targets_str));
            graph.add_entity(alias_entity);
            graph.add_edge(&config_id, &alias_id, EdgeKind::Contains);
        }
    }
}

/// Parse OpenAPI 3.x / Swagger 2.0 specifications into API Module, HTTP endpoints, and schemas.
fn parse_openapi_json(
    rel_path: &str,
    obj: &serde_json::Map<String, Value>,
    graph: &mut CodeGraph,
) {
    let title = obj
        .get("info")
        .and_then(|v| v.as_object())
        .and_then(|info| info.get("title"))
        .and_then(|t| t.as_str())
        .unwrap_or("API");

    let version = obj
        .get("info")
        .and_then(|v| v.as_object())
        .and_then(|info| info.get("version"))
        .and_then(|ver| ver.as_str())
        .unwrap_or("1.0.0");

    let description = obj
        .get("info")
        .and_then(|v| v.as_object())
        .and_then(|info| info.get("description"))
        .and_then(|d| d.as_str());

    let api_id = format!("{}::api", rel_path);
    let mut api_entity = CodeEntity::new(&api_id, title, EntityType::Module, rel_path, 1, 1);
    api_entity.signature = Some(format!("OpenAPI {} ({})", title, version));
    if let Some(desc) = description {
        api_entity.docstring = Some(desc.to_string());
    }
    graph.add_entity(api_entity);

    // 1. Paths & Operations (Endpoints)
    if let Some(paths) = obj.get("paths").and_then(|v| v.as_object()) {
        for (path_str, methods_val) in paths {
            if let Some(methods) = methods_val.as_object() {
                for (method, op_val) in methods {
                    let method_upper = method.to_uppercase();
                    if !matches!(
                        method_upper.as_str(),
                        "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "OPTIONS" | "HEAD"
                    ) {
                        continue;
                    }

                    let op = op_val.as_object();
                    let op_id_opt = op.and_then(|o| o.get("operationId")).and_then(|id| id.as_str());
                    let summary_opt = op.and_then(|o| o.get("summary")).and_then(|s| s.as_str());
                    let desc_opt = op.and_then(|o| o.get("description")).and_then(|d| d.as_str());

                    let endpoint_name = if let Some(op_id) = op_id_opt {
                        op_id.to_string()
                    } else {
                        format!("{} {}", method_upper, path_str)
                    };

                    let endpoint_id = format!("{}::endpoint::{} {}", rel_path, method_upper, path_str);
                    let mut ep_entity = CodeEntity::new(
                        &endpoint_id,
                        &endpoint_name,
                        EntityType::Function,
                        rel_path,
                        1,
                        1,
                    );

                    ep_entity.signature = Some(if let Some(sum) = summary_opt {
                        format!("{} {} - {}", method_upper, path_str, sum)
                    } else {
                        format!("{} {}", method_upper, path_str)
                    });

                    if let Some(d) = desc_opt {
                        ep_entity.docstring = Some(d.to_string());
                    }

                    graph.add_entity(ep_entity);
                    graph.add_edge(&api_id, &endpoint_id, EdgeKind::Contains);
                }
            }
        }
    }

    // 2. Components / Definitions Schemas
    let schemas = obj
        .get("components")
        .and_then(|c| c.as_object())
        .and_then(|c| c.get("schemas"))
        .and_then(|s| s.as_object())
        .or_else(|| {
            obj.get("definitions").and_then(|d| d.as_object())
        });

    if let Some(schemas_map) = schemas {
        for (schema_name, _schema_val) in schemas_map {
            let schema_id = format!("{}::schema::{}", rel_path, schema_name);
            let mut schema_entity =
                CodeEntity::new(&schema_id, schema_name, EntityType::Struct, rel_path, 1, 1);
            schema_entity.signature = Some(format!("schema {}", schema_name));
            graph.add_entity(schema_entity);
            graph.add_edge(&api_id, &schema_id, EdgeKind::Contains);
        }
    }
}

/// Parse standalone JSON Schema (`*.schema.json`) into Struct and property Variables.
fn parse_json_schema(
    rel_path: &str,
    file_name: &str,
    obj: &serde_json::Map<String, Value>,
    graph: &mut CodeGraph,
) {
    let title = obj
        .get("title")
        .and_then(|t| t.as_str())
        .unwrap_or_else(|| file_name.strip_suffix(".schema.json").unwrap_or(file_name));

    let desc = obj.get("description").and_then(|d| d.as_str());

    let schema_id = format!("{}::schema", rel_path);
    let mut schema_entity = CodeEntity::new(&schema_id, title, EntityType::Struct, rel_path, 1, 1);
    schema_entity.signature = Some(format!("schema: {}", title));
    if let Some(d) = desc {
        schema_entity.docstring = Some(d.to_string());
    }
    graph.add_entity(schema_entity);

    if let Some(props) = obj.get("properties").and_then(|p| p.as_object()) {
        for (prop_name, prop_val) in props {
            let prop_type = prop_val
                .as_object()
                .and_then(|p| p.get("type"))
                .and_then(|t| t.as_str())
                .unwrap_or("any");

            let prop_id = format!("{}::prop::{}", rel_path, prop_name);
            let mut prop_entity =
                CodeEntity::new(&prop_id, prop_name, EntityType::Variable, rel_path, 1, 1);
            prop_entity.signature = Some(format!("{}: {}", prop_name, prop_type));
            graph.add_entity(prop_entity);
            graph.add_edge(&schema_id, &prop_id, EdgeKind::Contains);
        }
    }
}

/// Parse `composer.json` into Module entity and dependencies.
fn parse_composer_json(rel_path: &str, obj: &serde_json::Map<String, Value>, graph: &mut CodeGraph) {
    let name = obj
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("composer-package");
    let desc = obj.get("description").and_then(|d| d.as_str());

    let pkg_id = format!("{}::package", rel_path);
    let mut pkg_entity = CodeEntity::new(&pkg_id, name, EntityType::Module, rel_path, 1, 1);
    pkg_entity.signature = Some(format!("composer package {}", name));
    if let Some(d) = desc {
        pkg_entity.docstring = Some(d.to_string());
    }
    graph.add_entity(pkg_entity);

    if let Some(reqs) = obj.get("require").and_then(|r| r.as_object()) {
        for (dep_name, dep_ver_val) in reqs {
            let dep_ver = dep_ver_val.as_str().unwrap_or("*");
            let dep_id = format!("{}::dep::{}", rel_path, dep_name);
            let mut dep_entity =
                CodeEntity::new(&dep_id, dep_name, EntityType::Module, rel_path, 1, 1);
            dep_entity.signature = Some(format!("dep: {} ({})", dep_name, dep_ver));
            graph.add_entity(dep_entity);
            graph.add_edge(&pkg_id, &dep_id, EdgeKind::DependsOn);
        }
    }
}

/// Parse `deno.json` / `deno.jsonc` into Module entity and task targets / import map.
fn parse_deno_json(rel_path: &str, obj: &serde_json::Map<String, Value>, graph: &mut CodeGraph) {
    let name = obj.get("name").and_then(|n| n.as_str()).unwrap_or("deno-module");
    let config_id = format!("{}::config", rel_path);
    let mut config_entity = CodeEntity::new(&config_id, name, EntityType::Module, rel_path, 1, 1);
    config_entity.signature = Some(format!("deno module {}", name));
    graph.add_entity(config_entity);

    if let Some(tasks) = obj.get("tasks").and_then(|t| t.as_object()) {
        for (task_name, task_cmd_val) in tasks {
            let cmd = task_cmd_val.as_str().unwrap_or("");
            let task_id = format!("{}::task::{}", rel_path, task_name);
            let mut task_entity =
                CodeEntity::new(&task_id, task_name, EntityType::Target, rel_path, 1, 1);
            task_entity.signature = Some(format!("deno task {}: {}", task_name, cmd));
            graph.add_entity(task_entity);
            graph.add_edge(&config_id, &task_id, EdgeKind::Contains);
        }
    }

    if let Some(imports) = obj.get("imports").and_then(|i| i.as_object()) {
        for (alias, target_val) in imports {
            let target = target_val.as_str().unwrap_or("");
            let alias_id = format!("{}::import::{}", rel_path, alias);
            let mut alias_entity =
                CodeEntity::new(&alias_id, alias, EntityType::Variable, rel_path, 1, 1);
            alias_entity.signature = Some(format!("import {} -> {}", alias, target));
            graph.add_entity(alias_entity);
            graph.add_edge(&config_id, &alias_id, EdgeKind::DependsOn);
        }
    }
}
