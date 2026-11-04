use std::path::Path;
use serde_yaml::Value;
use crate::graph::CodeGraph;
use crate::types::{CodeEntity, EdgeKind, EntityType};

/// Maximum file size for YAML manifest parsing (2MB)
const MAX_YAML_MANIFEST_SIZE: usize = 2 * 1024 * 1024;

/// Check if a given relative path and filename corresponds to a supported architectural YAML file.
pub fn is_supported_yaml_file(rel_path: &str, file_name: &str) -> bool {
    let lower_name = file_name.to_ascii_lowercase();

    // 1. Explicit exclusions for lockfiles and localization dumps
    if lower_name == "pnpm-lock.yaml"
        || lower_name == "yarn.lock"
        || lower_name.ends_with("-lock.yaml")
        || lower_name.ends_with("-lock.yml")
        || lower_name.ends_with(".lock.yaml")
        || lower_name.ends_with(".lock.yml")
    {
        return false;
    }

    let normalized_path = rel_path.replace('\\', "/");
    if normalized_path.contains("/locales/")
        || normalized_path.contains("/translations/")
        || normalized_path.starts_with("locales/")
        || normalized_path.starts_with("translations/")
    {
        return false;
    }

    // 2. CI/CD pipelines
    if normalized_path.contains(".github/workflows/")
        || normalized_path.contains(".github/actions/")
        || lower_name == ".gitlab-ci.yml"
        || lower_name == ".gitlab-ci.yaml"
        || lower_name == "azure-pipelines.yml"
        || lower_name == "azure-pipelines.yaml"
        || lower_name == "bitbucket-pipelines.yml"
    {
        return true;
    }

    // 3. Docker Compose manifests
    if lower_name == "docker-compose.yml"
        || lower_name == "docker-compose.yaml"
        || lower_name == "compose.yml"
        || lower_name == "compose.yaml"
        || ((lower_name.starts_with("docker-compose.") || lower_name.starts_with("compose."))
            && (lower_name.ends_with(".yaml") || lower_name.ends_with(".yml")))
    {
        return true;
    }

    // 4. OpenAPI & Swagger API specifications
    if lower_name == "openapi.yaml"
        || lower_name == "openapi.yml"
        || lower_name == "swagger.yaml"
        || lower_name == "swagger.yml"
        || lower_name.ends_with(".openapi.yaml")
        || lower_name.ends_with(".openapi.yml")
        || lower_name.ends_with(".swagger.yaml")
        || lower_name.ends_with(".swagger.yml")
        || ((lower_name.contains("openapi") || lower_name.contains("swagger"))
            && (lower_name.ends_with(".yaml") || lower_name.ends_with(".yml")))
    {
        return true;
    }

    // 5. Cloud, Serverless & Kubernetes / Helm manifests
    if lower_name == "serverless.yml"
        || lower_name == "serverless.yaml"
        || lower_name == "chart.yaml"
        || lower_name == "chart.yml"
        || lower_name == "kustomization.yaml"
        || lower_name == "kustomization.yml"
        || lower_name.ends_with(".schema.yaml")
        || lower_name.ends_with(".schema.yml")
    {
        return true;
    }

    false
}

/// Fast content heuristic to identify OpenAPI or Docker Compose in non-standard YAML filenames.
pub fn is_supported_yaml_content(content: &str) -> bool {
    let sample = if content.len() > 1024 {
        &content[..1024]
    } else {
        content
    };

    let sample_lower = sample.to_ascii_lowercase();
    sample_lower.contains("openapi:")
        || sample_lower.contains("swagger:")
        || (sample_lower.contains("services:") && (sample_lower.contains("version:") || sample_lower.contains("image:")))
}

/// Parse supported YAML manifests into the code graph.
pub fn parse_yaml_manifest(rel_path: &str, content: &str, graph: &mut CodeGraph) {
    if content.len() > MAX_YAML_MANIFEST_SIZE {
        return;
    }

    let parsed: Value = match serde_yaml::from_str(content) {
        Ok(v) => v,
        Err(_) => return,
    };

    let path = Path::new(rel_path);
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let normalized_path = rel_path.replace('\\', "/");

    // 1. OpenAPI & Swagger specifications
    let is_openapi = parsed.get("openapi").is_some()
        || parsed.get("swagger").is_some()
        || file_name.to_ascii_lowercase().contains("openapi")
        || file_name.to_ascii_lowercase().contains("swagger");

    if is_openapi {
        parse_openapi_yaml(rel_path, &parsed, graph);
        return;
    }

    // 2. GitHub Workflows & CI/CD
    if normalized_path.contains(".github/workflows/")
        || lower_matches(file_name, &[".gitlab-ci.yml", ".gitlab-ci.yaml", "azure-pipelines.yml", "bitbucket-pipelines.yml"])
    {
        parse_cicd_workflow_yaml(rel_path, file_name, &parsed, graph);
        return;
    }

    // 3. Docker Compose
    if is_docker_compose(file_name, &parsed) {
        parse_docker_compose_yaml(rel_path, file_name, &parsed, graph);
        return;
    }

    // 4. Serverless Framework
    if lower_matches(file_name, &["serverless.yml", "serverless.yaml"]) || parsed.get("service").is_some() && parsed.get("provider").is_some() {
        parse_serverless_yaml(rel_path, &parsed, graph);
        return;
    }

    // 5. Helm Chart
    if lower_matches(file_name, &["chart.yaml", "chart.yml"]) {
        parse_helm_chart_yaml(rel_path, &parsed, graph);
    }
}

fn lower_matches(name: &str, matches: &[&str]) -> bool {
    let lower = name.to_ascii_lowercase();
    matches.iter().any(|&m| lower == m)
}

fn is_docker_compose(file_name: &str, parsed: &Value) -> bool {
    let lower = file_name.to_ascii_lowercase();
    if lower.contains("docker-compose") || lower.starts_with("compose.") {
        return true;
    }
    parsed.get("services").and_then(|s| s.as_mapping()).is_some()
}

/// Parse GitHub Actions or GitLab CI workflow YAML into Workflow module and Job targets.
fn parse_cicd_workflow_yaml(
    rel_path: &str,
    file_name: &str,
    parsed: &Value,
    graph: &mut CodeGraph,
) {
    let workflow_name = parsed
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| Path::new(file_name).file_stem().and_then(|s| s.to_str()).unwrap_or(file_name));

    let workflow_id = format!("{}::workflow", rel_path);
    let mut wf_entity = CodeEntity::new(&workflow_id, workflow_name, EntityType::Module, rel_path, 1, 1);
    wf_entity.signature = Some(format!("workflow: {}", workflow_name));

    // Triggers (on: [push, pull_request])
    if let Some(on_val) = parsed.get("on") {
        let triggers = match on_val {
            Value::String(s) => s.clone(),
            Value::Sequence(seq) => seq
                .iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            Value::Mapping(map) => map
                .keys()
                .filter_map(|k| k.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            _ => String::new(),
        };
        if !triggers.is_empty() {
            wf_entity.metadata.insert("triggers".to_string(), triggers);
        }
    }
    graph.add_entity(wf_entity);

    // Jobs
    if let Some(jobs) = parsed.get("jobs").and_then(|j| j.as_mapping()) {
        // Pass 1: Register all jobs and action dependencies
        for (job_key, job_val) in jobs {
            let job_id_str = match job_key.as_str() {
                Some(s) => s,
                None => continue,
            };

            let job_name = job_val
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(job_id_str);

            let runs_on = job_val
                .get("runs-on")
                .and_then(|v| v.as_str())
                .unwrap_or("default-runner");

            let job_node_id = format!("{}::job::{}", rel_path, job_id_str);
            let mut job_entity =
                CodeEntity::new(&job_node_id, job_name, EntityType::Target, rel_path, 1, 1);
            job_entity.signature = Some(format!("job {}: (runs-on: {})", job_id_str, runs_on));
            graph.add_entity(job_entity);
            graph.add_edge(&workflow_id, &job_node_id, EdgeKind::Contains);

            // Steps & Action Dependencies (`uses: actions/checkout@v4`)
            if let Some(steps) = job_val.get("steps").and_then(|s| s.as_sequence()) {
                for step in steps {
                    if let Some(uses_action) = step.get("uses").and_then(|u| u.as_str()) {
                        let action_id = format!("action::{}", uses_action);
                        if graph.get_entity(&action_id).is_none() {
                            let mut action_ent = CodeEntity::new(
                                &action_id,
                                uses_action,
                                EntityType::Module,
                                rel_path,
                                1,
                                1,
                            );
                            action_ent.signature = Some(format!("action {}", uses_action));
                            graph.add_entity(action_ent);
                        }
                        graph.add_edge(&job_node_id, &action_id, EdgeKind::DependsOn);
                    }
                }
            }
        }

        // Pass 2: Connect inter-job `needs:` dependencies
        for (job_key, job_val) in jobs {
            let job_id_str = match job_key.as_str() {
                Some(s) => s,
                None => continue,
            };
            let job_node_id = format!("{}::job::{}", rel_path, job_id_str);

            if let Some(needs_val) = job_val.get("needs") {
                let mut needs_list = Vec::new();
                match needs_val {
                    Value::String(s) => needs_list.push(s.as_str()),
                    Value::Sequence(seq) => {
                        for item in seq {
                            if let Some(s) = item.as_str() {
                                needs_list.push(s);
                            }
                        }
                    }
                    _ => {}
                }

                for needed_job in needs_list {
                    let needed_id = format!("{}::job::{}", rel_path, needed_job);
                    graph.add_edge(&job_node_id, &needed_id, EdgeKind::DependsOn);
                }
            }
        }
    }
}

/// Parse Docker Compose YAML into Compose module and service modules with DependsOn edges.
fn parse_docker_compose_yaml(
    rel_path: &str,
    file_name: &str,
    parsed: &Value,
    graph: &mut CodeGraph,
) {
    let compose_id = format!("{}::compose", rel_path);
    let mut compose_entity =
        CodeEntity::new(&compose_id, file_name, EntityType::Module, rel_path, 1, 1);
    compose_entity.signature = Some(format!("docker-compose: {}", file_name));
    graph.add_entity(compose_entity);

    if let Some(services) = parsed.get("services").and_then(|s| s.as_mapping()) {
        // Pass 1: Register all service entities in the graph
        for (svc_key, svc_val) in services {
            let svc_name = match svc_key.as_str() {
                Some(s) => s,
                None => continue,
            };

            let image = svc_val
                .get("image")
                .and_then(|v| v.as_str())
                .unwrap_or("custom-build");

            let svc_id = format!("{}::service::{}", rel_path, svc_name);
            let mut svc_entity =
                CodeEntity::new(&svc_id, svc_name, EntityType::Module, rel_path, 1, 1);

            let ports_str = if let Some(ports) = svc_val.get("ports").and_then(|p| p.as_sequence()) {
                ports
                    .iter()
                    .filter_map(|p| match p {
                        Value::String(s) => Some(s.clone()),
                        Value::Number(n) => Some(n.to_string()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            } else {
                String::new()
            };

            svc_entity.signature = Some(if ports_str.is_empty() {
                format!("service {} (image: {})", svc_name, image)
            } else {
                format!("service {} (image: {}, ports: [{}])", svc_name, image, ports_str)
            });

            graph.add_entity(svc_entity);
            graph.add_edge(&compose_id, &svc_id, EdgeKind::Contains);
        }

        // Pass 2: Establish DependsOn inter-service relations
        for (svc_key, svc_val) in services {
            let svc_name = match svc_key.as_str() {
                Some(s) => s,
                None => continue,
            };
            let svc_id = format!("{}::service::{}", rel_path, svc_name);

            if let Some(deps) = svc_val.get("depends_on") {
                let mut dep_names = Vec::new();
                match deps {
                    Value::Sequence(seq) => {
                        for d in seq {
                            if let Some(s) = d.as_str() {
                                dep_names.push(s);
                            }
                        }
                    }
                    Value::Mapping(map) => {
                        for k in map.keys() {
                            if let Some(s) = k.as_str() {
                                dep_names.push(s);
                            }
                        }
                    }
                    _ => {}
                }

                for dep_svc in dep_names {
                    let dep_id = format!("{}::service::{}", rel_path, dep_svc);
                    graph.add_edge(&svc_id, &dep_id, EdgeKind::DependsOn);
                }
            }
        }
    }
}

/// Parse OpenAPI 3.x / Swagger YAML specifications into API Module, HTTP endpoints, and schemas.
fn parse_openapi_yaml(rel_path: &str, parsed: &Value, graph: &mut CodeGraph) {
    let info = parsed.get("info");
    let title = info
        .and_then(|i| i.get("title"))
        .and_then(|t| t.as_str())
        .unwrap_or("OpenAPI Specification");

    let version = info
        .and_then(|i| i.get("version"))
        .and_then(|v| match v {
            Value::String(s) => Some(s.as_str()),
            Value::Number(_) => None,
            _ => None,
        })
        .unwrap_or("1.0.0");

    let description = info
        .and_then(|i| i.get("description"))
        .and_then(|d| d.as_str());

    let api_id = format!("{}::api", rel_path);
    let mut api_entity = CodeEntity::new(&api_id, title, EntityType::Module, rel_path, 1, 1);
    api_entity.signature = Some(format!("OpenAPI {} ({})", title, version));
    if let Some(desc) = description {
        api_entity.docstring = Some(desc.to_string());
    }
    graph.add_entity(api_entity);

    // 1. Paths & Endpoints
    if let Some(paths) = parsed.get("paths").and_then(|p| p.as_mapping()) {
        for (path_key, methods_val) in paths {
            let path_str = match path_key.as_str() {
                Some(s) => s,
                None => continue,
            };

            if let Some(methods) = methods_val.as_mapping() {
                for (method_key, op_val) in methods {
                    let method_name = match method_key.as_str() {
                        Some(m) => m,
                        None => continue,
                    };

                    let method_upper = method_name.to_uppercase();
                    if !matches!(
                        method_upper.as_str(),
                        "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "OPTIONS" | "HEAD"
                    ) {
                        continue;
                    }

                    let op_id_opt = op_val.get("operationId").and_then(|id| id.as_str());
                    let summary_opt = op_val.get("summary").and_then(|s| s.as_str());
                    let desc_opt = op_val.get("description").and_then(|d| d.as_str());

                    let endpoint_name = if let Some(op_id) = op_id_opt {
                        op_id.to_string()
                    } else {
                        format!("{} {}", method_upper, path_str)
                    };

                    let endpoint_id =
                        format!("{}::endpoint::{} {}", rel_path, method_upper, path_str);
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
    let schemas = parsed
        .get("components")
        .and_then(|c| c.get("schemas"))
        .and_then(|s| s.as_mapping())
        .or_else(|| parsed.get("definitions").and_then(|d| d.as_mapping()));

    if let Some(schemas_map) = schemas {
        for (schema_key, _) in schemas_map {
            if let Some(schema_name) = schema_key.as_str() {
                let schema_id = format!("{}::schema::{}", rel_path, schema_name);
                let mut schema_entity =
                    CodeEntity::new(&schema_id, schema_name, EntityType::Struct, rel_path, 1, 1);
                schema_entity.signature = Some(format!("schema {}", schema_name));
                graph.add_entity(schema_entity);
                graph.add_edge(&api_id, &schema_id, EdgeKind::Contains);
            }
        }
    }
}

/// Parse Serverless Framework `serverless.yml` into Service module and Lambda functions.
fn parse_serverless_yaml(rel_path: &str, parsed: &Value, graph: &mut CodeGraph) {
    let service_name = parsed
        .get("service")
        .and_then(|s| match s {
            Value::String(name) => Some(name.as_str()),
            Value::Mapping(m) => m.get(&Value::String("name".to_string())).and_then(|v| v.as_str()),
            _ => None,
        })
        .unwrap_or("serverless-service");

    let service_id = format!("{}::service", rel_path);
    let mut svc_entity =
        CodeEntity::new(&service_id, service_name, EntityType::Module, rel_path, 1, 1);
    svc_entity.signature = Some(format!("serverless service: {}", service_name));
    graph.add_entity(svc_entity);

    if let Some(functions) = parsed.get("functions").and_then(|f| f.as_mapping()) {
        for (fn_key, fn_val) in functions {
            let fn_name = match fn_key.as_str() {
                Some(s) => s,
                None => continue,
            };

            let handler = fn_val
                .get("handler")
                .and_then(|h| h.as_str())
                .unwrap_or("custom-handler");

            let fn_id = format!("{}::function::{}", rel_path, fn_name);
            let mut fn_entity =
                CodeEntity::new(&fn_id, fn_name, EntityType::Function, rel_path, 1, 1);
            fn_entity.signature = Some(format!("lambda {} (handler: {})", fn_name, handler));
            graph.add_entity(fn_entity);
            graph.add_edge(&service_id, &fn_id, EdgeKind::Contains);
        }
    }
}

/// Parse Helm `Chart.yaml` into Chart module and Chart dependencies.
fn parse_helm_chart_yaml(rel_path: &str, parsed: &Value, graph: &mut CodeGraph) {
    let chart_name = parsed
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("helm-chart");

    let version = parsed
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("0.1.0");

    let chart_id = format!("{}::chart", rel_path);
    let mut chart_entity =
        CodeEntity::new(&chart_id, chart_name, EntityType::Module, rel_path, 1, 1);
    chart_entity.signature = Some(format!("helm chart {} ({})", chart_name, version));
    graph.add_entity(chart_entity);

    if let Some(deps) = parsed.get("dependencies").and_then(|d| d.as_sequence()) {
        for dep in deps {
            if let Some(dep_name) = dep.get("name").and_then(|n| n.as_str()) {
                let dep_ver = dep.get("version").and_then(|v| v.as_str()).unwrap_or("*");
                let dep_id = format!("{}::dep::{}", rel_path, dep_name);
                let mut dep_entity =
                    CodeEntity::new(&dep_id, dep_name, EntityType::Module, rel_path, 1, 1);
                dep_entity.signature = Some(format!("dep: {} ({})", dep_name, dep_ver));
                graph.add_entity(dep_entity);
                graph.add_edge(&chart_id, &dep_id, EdgeKind::DependsOn);
            }
        }
    }
}
