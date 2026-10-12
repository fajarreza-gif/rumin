use std::path::Path;
use regex::Regex;
use crate::graph::{CodeGraph, CodeEntity, EntityType, EdgeKind};

/// Count comma-separated arguments in an Erlang function head taking nested tuples/lists into account.
pub fn count_erlang_args(args_str: &str) -> usize {
    let trimmed = args_str.trim();
    if trimmed.is_empty() {
        return 0;
    }
    let mut depth = 0;
    let mut count = 1;
    for ch in trimmed.chars() {
        match ch {
            '(' | '{' | '[' => depth += 1,
            ')' | '}' | ']' => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            ',' if depth == 0 => count += 1,
            _ => {}
        }
    }
    count
}

/// Parse Erlang module, header, or escript files into modules, behaviors, records, macros, types, functions, and cross-calls.
pub fn parse_erlang_module(rel_path: &str, content: &str, graph: &mut CodeGraph) {
    let mod_re = Regex::new(r#"^\s*-\s*module\s*\(\s*([a-zA-Z0-9_]+)\s*\)\s*\."#).unwrap();
    let beh_re = Regex::new(r#"^\s*-\s*behaviou?r\s*\(\s*([a-zA-Z0-9_]+)\s*\)\s*\."#).unwrap();
    let rec_re = Regex::new(r#"^\s*-\s*record\s*\(\s*([a-zA-Z0-9_]+)\s*,"#).unwrap();
    let def_re = Regex::new(r#"^\s*-\s*define\s*\(\s*([a-zA-Z0-9_]+)"#).unwrap();
    let type_re = Regex::new(r#"^\s*-\s*(?:type|opaque)\s+([a-zA-Z0-9_]+)\s*\("#).unwrap();
    let inc_re = Regex::new(r#"^\s*-\s*include(?:_lib)?\s*\(\s*"([^"]+)"\s*\)\s*\."#).unwrap();
    let func_re = Regex::new(r#"^([a-z][a-zA-Z0-9_]*)\s*\(([^)]*)\)\s*(?:when\s+[^->]+)?\s*->"#).unwrap();
    let call_re = Regex::new(r#"\b([a-z][a-zA-Z0-9_]*):([a-z][a-zA-Z0-9_]*)\s*\("#).unwrap();

    let path = Path::new(rel_path);
    let default_mod = path.file_stem().and_then(|s| s.to_str()).unwrap_or("erlang_mod");
    let mut current_module = default_mod.to_string();
    let mut module_node_id = format!("{}::{}", rel_path, current_module);
    let mut module_registered = false;

    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;
        let trimmed = line.trim();
        if trimmed.starts_with('%') || trimmed.is_empty() {
            continue;
        }

        // 1. Module declaration
        if let Some(caps) = mod_re.captures(trimmed) {
            let m_name = caps.get(1).unwrap().as_str();
            current_module = m_name.to_string();
            module_node_id = format!("{}::{}", rel_path, m_name);
            let mut entity = CodeEntity::new(&module_node_id, m_name, EntityType::Module, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            graph.add_entity(entity);
            module_registered = true;

            // Canonical alias node erlang::<module>
            let alias_id = format!("erlang::{}", m_name);
            if graph.get_entity(&alias_id).is_none() {
                let mut alias_ent = CodeEntity::new(&alias_id, m_name, EntityType::Module, rel_path, line_num, line_num);
                alias_ent.signature = Some(trimmed.to_string());
                graph.add_entity(alias_ent);
            }
            graph.add_edge(&alias_id, &module_node_id, EdgeKind::References);
            continue;
        }

        // Ensure module node is registered if encountering items before/without -module
        if !module_registered && (trimmed.starts_with('-') || func_re.is_match(trimmed)) {
            let mut entity = CodeEntity::new(&module_node_id, &current_module, EntityType::Module, rel_path, line_num, line_num);
            entity.signature = Some(format!("-module({}).", current_module));
            graph.add_entity(entity);
            module_registered = true;
        }

        // 2. Behavior declaration
        if let Some(caps) = beh_re.captures(trimmed) {
            let beh_name = caps.get(1).unwrap().as_str();
            let otp_id = format!("otp::{}", beh_name);
            if graph.get_entity(&otp_id).is_none() {
                let mut otp_ent = CodeEntity::new(&otp_id, beh_name, EntityType::Interface, rel_path, line_num, line_num);
                otp_ent.signature = Some(format!("-behaviour({}).", beh_name));
                graph.add_entity(otp_ent);
            }
            graph.add_edge(&module_node_id, &otp_id, EdgeKind::Implements);
            continue;
        }

        // 3. Record declaration
        if let Some(caps) = rec_re.captures(trimmed) {
            let rec_name = caps.get(1).unwrap().as_str();
            let rec_id = format!("{}::record::{}", rel_path, rec_name);
            let mut entity = CodeEntity::new(&rec_id, rec_name, EntityType::Struct, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            graph.add_entity(entity);
            graph.add_edge(&module_node_id, &rec_id, EdgeKind::Contains);
            continue;
        }

        // 4. Macro definition
        if let Some(caps) = def_re.captures(trimmed) {
            let macro_name = caps.get(1).unwrap().as_str();
            let macro_id = format!("{}::macro::{}", rel_path, macro_name);
            let mut entity = CodeEntity::new(&macro_id, macro_name, EntityType::Variable, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            graph.add_entity(entity);
            graph.add_edge(&module_node_id, &macro_id, EdgeKind::Contains);
            continue;
        }

        // 5. Type declaration
        if let Some(caps) = type_re.captures(trimmed) {
            let type_name = caps.get(1).unwrap().as_str();
            let type_id = format!("{}::type::{}", rel_path, type_name);
            let mut entity = CodeEntity::new(&type_id, type_name, EntityType::Struct, rel_path, line_num, line_num);
            entity.signature = Some(trimmed.to_string());
            graph.add_entity(entity);
            graph.add_edge(&module_node_id, &type_id, EdgeKind::Contains);
            continue;
        }

        // 6. Include headers
        if let Some(caps) = inc_re.captures(trimmed) {
            let header = caps.get(1).unwrap().as_str();
            let inc_id = format!("{}::include::{}", rel_path, header);
            if graph.get_entity(&inc_id).is_none() {
                let mut entity = CodeEntity::new(&inc_id, header, EntityType::Module, rel_path, line_num, line_num);
                entity.signature = Some(trimmed.to_string());
                graph.add_entity(entity);
            }
            graph.add_edge(&module_node_id, &inc_id, EdgeKind::Imports);
            continue;
        }

        // 7. Function declaration
        if let Some(caps) = func_re.captures(trimmed) {
            let func_name = caps.get(1).unwrap().as_str();
            let args_str = caps.get(2).unwrap().as_str();
            let arity = count_erlang_args(args_str);
            let full_func_name = format!("{}/{}", func_name, arity);
            let func_id = format!("{}::{}", rel_path, full_func_name);

            if graph.get_entity(&func_id).is_none() {
                let mut entity = CodeEntity::new(&func_id, &full_func_name, EntityType::Function, rel_path, line_num, line_num);
                entity.signature = Some(trimmed.to_string());
                graph.add_entity(entity);
                graph.add_edge(&module_node_id, &func_id, EdgeKind::Contains);
            }
        }

        // 8. Cross-module calls
        for call_caps in call_re.captures_iter(trimmed) {
            let target_mod = call_caps.get(1).unwrap().as_str();
            if target_mod != current_module && target_mod != "erlang" {
                let target_id = format!("erlang::{}", target_mod);
                if graph.get_entity(&target_id).is_none() {
                    let target_ent = CodeEntity::new(&target_id, target_mod, EntityType::Module, rel_path, line_num, line_num);
                    graph.add_entity(target_ent);
                }
                graph.add_edge(&module_node_id, &target_id, EdgeKind::Calls);
            }
        }
    }
}

/// Parse Erlang OTP application specification (.app / .app.src).
pub fn parse_erlang_app(rel_path: &str, content: &str, graph: &mut CodeGraph) {
    let app_re = Regex::new(r#"\{\s*application\s*,\s*([a-zA-Z0-9_]+)"#).unwrap();
    let mod_list_re = Regex::new(r#"\{\s*modules\s*,\s*\[([^\]]+)\]"#).unwrap();
    let app_list_re = Regex::new(r#"\{\s*applications\s*,\s*\[([^\]]+)\]"#).unwrap();
    let mod_cb_re = Regex::new(r#"\{\s*mod\s*,\s*\{\s*([a-zA-Z0-9_]+)"#).unwrap();

    let path = Path::new(rel_path);
    let default_app = path.file_stem().and_then(|s| s.to_str()).unwrap_or("app");

    let app_name = if let Some(caps) = app_re.captures(content) {
        caps.get(1).unwrap().as_str()
    } else {
        default_app
    };

    let app_id = format!("{}::application::{}", rel_path, app_name);
    let mut app_entity = CodeEntity::new(&app_id, app_name, EntityType::Module, rel_path, 1, 1);
    app_entity.signature = Some(format!("{{application, {}, [...]}}", app_name));
    graph.add_entity(app_entity);

    // Modules contained in the application
    if let Some(caps) = mod_list_re.captures(content) {
        let modules_str = caps.get(1).unwrap().as_str();
        for m in modules_str.split(',') {
            let m_name = m.trim().trim_matches(|c| c == '\'' || c == ' ' || c == '\n' || c == '\r');
            if !m_name.is_empty() {
                let mod_id = format!("erlang::{}", m_name);
                if graph.get_entity(&mod_id).is_none() {
                    let m_ent = CodeEntity::new(&mod_id, m_name, EntityType::Module, rel_path, 1, 1);
                    graph.add_entity(m_ent);
                }
                graph.add_edge(&app_id, &mod_id, EdgeKind::Contains);
            }
        }
    }

    // Dependent applications
    if let Some(caps) = app_list_re.captures(content) {
        let apps_str = caps.get(1).unwrap().as_str();
        for a in apps_str.split(',') {
            let dep_name = a.trim().trim_matches(|c| c == '\'' || c == ' ' || c == '\n' || c == '\r');
            if !dep_name.is_empty() && dep_name != "kernel" && dep_name != "stdlib" {
                let dep_id = format!("erlang::app::{}", dep_name);
                if graph.get_entity(&dep_id).is_none() {
                    let d_ent = CodeEntity::new(&dep_id, dep_name, EntityType::Module, rel_path, 1, 1);
                    graph.add_entity(d_ent);
                }
                graph.add_edge(&app_id, &dep_id, EdgeKind::DependsOn);
            }
        }
    }

    // Application callback module ({mod, {CallbackMod, []}})
    if let Some(caps) = mod_cb_re.captures(content) {
        let cb_mod = caps.get(1).unwrap().as_str();
        let cb_id = format!("erlang::{}", cb_mod);
        if graph.get_entity(&cb_id).is_none() {
            let cb_ent = CodeEntity::new(&cb_id, cb_mod, EntityType::Module, rel_path, 1, 1);
            graph.add_entity(cb_ent);
        }
        graph.add_edge(&app_id, &cb_id, EdgeKind::References);
    }
}

/// Parse Rebar3 build configuration (rebar.config) for dependencies.
pub fn parse_rebar_config(rel_path: &str, content: &str, graph: &mut CodeGraph) {
    let deps_re = Regex::new(r#"\{\s*deps\s*,\s*\[([^\]]+)\]"#).unwrap();
    let dep_entry_re = Regex::new(r#"(?:\{\s*([a-zA-Z0-9_]+)|([a-zA-Z0-9_]+))"#).unwrap();

    let config_id = format!("{}::config", rel_path);
    let mut entity = CodeEntity::new(&config_id, "rebar.config", EntityType::Target, rel_path, 1, 1);
    entity.signature = Some("rebar.config".to_string());
    graph.add_entity(entity);

    if let Some(caps) = deps_re.captures(content) {
        let deps_body = caps.get(1).unwrap().as_str();
        for dep_caps in dep_entry_re.captures_iter(deps_body) {
            let dep_name = dep_caps.get(1).or_else(|| dep_caps.get(2)).map(|m| m.as_str()).unwrap_or("");
            if !dep_name.is_empty() && dep_name != "git" {
                let dep_id = format!("{}::dep::{}", rel_path, dep_name);
                let mut dep_entity = CodeEntity::new(&dep_id, dep_name, EntityType::Module, rel_path, 1, 1);
                dep_entity.signature = Some(format!("dep: {}", dep_name));
                graph.add_entity(dep_entity);
                graph.add_edge(&config_id, &dep_id, EdgeKind::DependsOn);
            }
        }
    }
}

/// Check if a file is an Apigee API proxy bundle component or policy XML.
pub fn is_apigee_file(path: &Path, content: &str) -> bool {
    let components: Vec<_> = path.iter().map(|c| c.to_string_lossy().to_string()).collect();
    if components.iter().any(|c| c == "apiproxy" || c == "sharedflowbundle") {
        return true;
    }

    let trimmed = content.trim_start();
    if !trimmed.starts_with('<') {
        return false;
    }

    let apigee_roots = [
        "<APIProxy",
        "<SharedFlow",
        "<ProxyEndpoint",
        "<TargetEndpoint",
        "<AssignMessage",
        "<VerifyAPIKey",
        "<SpikeArrest",
        "<Quota",
        "<ResponseCache",
        "<OAuthV2",
        "<ServiceCallout",
        "<Javascript",
        "<JavaCallout",
        "<ExtractVariables",
        "<JSONToXML",
        "<XMLToJSON",
        "<RaiseFault",
        "<BasicAuthentication",
        "<JSONThreatProtection",
        "<XMLThreatProtection",
        "<FlowCallout",
        "<KeyValueMapOperations",
        "<AccessEntity",
        "<StatisticsCollector",
    ];

    apigee_roots.iter().any(|root| trimmed.contains(root))
}

/// Parse Apigee API proxy manifests, proxy endpoints, target endpoints, and policies.
pub fn parse_apigee(rel_path: &str, content: &str, graph: &mut CodeGraph) {
    let path = Path::new(rel_path);
    let default_name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("apigee_entity");

    let manifest_re = Regex::new(r#"<APIProxy(?:\s+[^>]*?)?\s+name=["']([^"']+)["']|<SharedFlow(?:\s+[^>]*?)?\s+name=["']([^"']+)["']"#).unwrap();
    let proxy_re = Regex::new(r#"<ProxyEndpoint(?:\s+[^>]*?)?\s+name=["']([^"']+)["']"#).unwrap();
    let target_re = Regex::new(r#"<TargetEndpoint(?:\s+[^>]*?)?\s+name=["']([^"']+)["']"#).unwrap();
    let basepath_re = Regex::new(r#"<BasePath>([^<]+)</BasePath>"#).unwrap();
    let url_re = Regex::new(r#"<URL>([^<]+)</URL>"#).unwrap();
    let step_re = Regex::new(r#"<Step>\s*<Name>([^<]+)</Name>"#).unwrap();
    let route_target_re = Regex::new(r#"<RouteRule[^>]*>[\s\S]*?<TargetEndpoint>([^<]+)</TargetEndpoint>"#).unwrap();
    let resource_url_re = Regex::new(r#"<ResourceURL>jsc://([^<]+)</ResourceURL>"#).unwrap();
    let java_class_re = Regex::new(r#"<ClassName>([^<]+)</ClassName>"#).unwrap();

    // 1. APIProxy or SharedFlow Manifest
    if let Some(caps) = manifest_re.captures(content) {
        let proxy_name = caps.get(1).or_else(|| caps.get(2)).map(|m| m.as_str()).unwrap_or(default_name);
        let manifest_id = format!("apiproxy::{}", proxy_name);
        let basepath = basepath_re.captures(content).map(|c| c.get(1).unwrap().as_str()).unwrap_or("");

        let mut entity = CodeEntity::new(&manifest_id, proxy_name, EntityType::Module, rel_path, 1, 1);
        entity.signature = Some(if basepath.is_empty() {
            format!("APIProxy {}", proxy_name)
        } else {
            format!("APIProxy {} (basepath: {})", proxy_name, basepath)
        });
        graph.add_entity(entity);

        // Connect ProxyEndpoints listed in manifest
        let pe_section_re = Regex::new(r#"<ProxyEndpoints>([\s\S]*?)</ProxyEndpoints>"#).unwrap();
        let pe_item_re = Regex::new(r#"<ProxyEndpoint>([^<]+)</ProxyEndpoint>"#).unwrap();
        if let Some(sec) = pe_section_re.captures(content) {
            for pe_caps in pe_item_re.captures_iter(sec.get(1).unwrap().as_str()) {
                let pe_name = pe_caps.get(1).unwrap().as_str().trim();
                if !pe_name.is_empty() {
                    let pe_id = format!("apiproxy::proxy::{}", pe_name);
                    if graph.get_entity(&pe_id).is_none() {
                        let pe_ent = CodeEntity::new(&pe_id, pe_name, EntityType::Interface, rel_path, 1, 1);
                        graph.add_entity(pe_ent);
                    }
                    graph.add_edge(&manifest_id, &pe_id, EdgeKind::Contains);
                }
            }
        }

        // Connect TargetEndpoints listed in manifest
        let te_section_re = Regex::new(r#"<TargetEndpoints>([\s\S]*?)</TargetEndpoints>"#).unwrap();
        let te_item_re = Regex::new(r#"<TargetEndpoint>([^<]+)</TargetEndpoint>"#).unwrap();
        if let Some(sec) = te_section_re.captures(content) {
            for te_caps in te_item_re.captures_iter(sec.get(1).unwrap().as_str()) {
                let te_name = te_caps.get(1).unwrap().as_str().trim();
                if !te_name.is_empty() {
                    let te_id = format!("apiproxy::target::{}", te_name);
                    if graph.get_entity(&te_id).is_none() {
                        let te_ent = CodeEntity::new(&te_id, te_name, EntityType::Interface, rel_path, 1, 1);
                        graph.add_entity(te_ent);
                    }
                    graph.add_edge(&manifest_id, &te_id, EdgeKind::Contains);
                }
            }
        }

        // Connect Policies listed in manifest
        let pol_section_re = Regex::new(r#"<Policies>([\s\S]*?)</Policies>"#).unwrap();
        let pol_item_re = Regex::new(r#"<Policy>([^<]+)</Policy>"#).unwrap();
        if let Some(sec) = pol_section_re.captures(content) {
            for pol_caps in pol_item_re.captures_iter(sec.get(1).unwrap().as_str()) {
                let pol_name = pol_caps.get(1).unwrap().as_str().trim();
                if !pol_name.is_empty() {
                    let pol_id = format!("apiproxy::policy::{}", pol_name);
                    if graph.get_entity(&pol_id).is_none() {
                        let pol_ent = CodeEntity::new(&pol_id, pol_name, EntityType::Rule, rel_path, 1, 1);
                        graph.add_entity(pol_ent);
                    }
                    graph.add_edge(&manifest_id, &pol_id, EdgeKind::Contains);
                }
            }
        }
        return;
    }

    // 2. ProxyEndpoint
    if content.contains("<ProxyEndpoint") {
        let proxy_name = proxy_re.captures(content).map(|c| c.get(1).unwrap().as_str()).unwrap_or(default_name);
        let proxy_id = format!("apiproxy::proxy::{}", proxy_name);
        let basepath = basepath_re.captures(content).map(|c| c.get(1).unwrap().as_str()).unwrap_or("");

        let mut entity = CodeEntity::new(&proxy_id, proxy_name, EntityType::Interface, rel_path, 1, 1);
        entity.signature = Some(if basepath.is_empty() {
            format!("ProxyEndpoint {}", proxy_name)
        } else {
            format!("ProxyEndpoint {} (basepath: {})", proxy_name, basepath)
        });
        graph.add_entity(entity);

        // Policy call steps
        for step_caps in step_re.captures_iter(content) {
            let step_name = step_caps.get(1).unwrap().as_str();
            let pol_id = format!("apiproxy::policy::{}", step_name);
            if graph.get_entity(&pol_id).is_none() {
                let pol_entity = CodeEntity::new(&pol_id, step_name, EntityType::Rule, rel_path, 1, 1);
                graph.add_entity(pol_entity);
            }
            graph.add_edge(&proxy_id, &pol_id, EdgeKind::Calls);
        }

        // TargetEndpoint route rules
        for route_caps in route_target_re.captures_iter(content) {
            let target_name = route_caps.get(1).unwrap().as_str();
            let target_id = format!("apiproxy::target::{}", target_name);
            if graph.get_entity(&target_id).is_none() {
                let target_entity = CodeEntity::new(&target_id, target_name, EntityType::Interface, rel_path, 1, 1);
                graph.add_entity(target_entity);
            }
            graph.add_edge(&proxy_id, &target_id, EdgeKind::DependsOn);
        }
        return;
    }

    // 3. TargetEndpoint
    if content.contains("<TargetEndpoint") {
        let target_name = target_re.captures(content).map(|c| c.get(1).unwrap().as_str()).unwrap_or(default_name);
        let target_id = format!("apiproxy::target::{}", target_name);
        let url = url_re.captures(content).map(|c| c.get(1).unwrap().as_str()).unwrap_or("");

        let mut entity = CodeEntity::new(&target_id, target_name, EntityType::Interface, rel_path, 1, 1);
        entity.signature = Some(if url.is_empty() {
            format!("TargetEndpoint {}", target_name)
        } else {
            format!("TargetEndpoint {} (url: {})", target_name, url)
        });
        graph.add_entity(entity);

        // Policy call steps in target
        for step_caps in step_re.captures_iter(content) {
            let step_name = step_caps.get(1).unwrap().as_str();
            let pol_id = format!("apiproxy::policy::{}", step_name);
            if graph.get_entity(&pol_id).is_none() {
                let pol_entity = CodeEntity::new(&pol_id, step_name, EntityType::Rule, rel_path, 1, 1);
                graph.add_entity(pol_entity);
            }
            graph.add_edge(&target_id, &pol_id, EdgeKind::Calls);
        }
        return;
    }

    // 4. Policy definition
    let policy_tag_re = Regex::new(r#"<([A-Za-z0-9]+)(?:\s+[^>]*?)?\s+name=["']([^"']+)["']"#).unwrap();
    if let Some(caps) = policy_tag_re.captures(content) {
        let pol_type = caps.get(1).unwrap().as_str();
        let pol_name = caps.get(2).unwrap().as_str();
        let pol_id = format!("apiproxy::policy::{}", pol_name);

        let mut entity = CodeEntity::new(&pol_id, pol_name, EntityType::Rule, rel_path, 1, 1);
        entity.signature = Some(format!("Policy {} ({})", pol_name, pol_type));
        graph.add_entity(entity);

        // If Javascript policy, extract resource URL
        if let Some(res_caps) = resource_url_re.captures(content) {
            let script_name = res_caps.get(1).unwrap().as_str();
            let res_id = format!("apiproxy::resource::{}", script_name);
            if graph.get_entity(&res_id).is_none() {
                let res_entity = CodeEntity::new(&res_id, script_name, EntityType::Function, rel_path, 1, 1);
                graph.add_entity(res_entity);
            }
            graph.add_edge(&pol_id, &res_id, EdgeKind::References);
        }

        // If JavaCallout policy, extract ClassName
        if let Some(cls_caps) = java_class_re.captures(content) {
            let class_name = cls_caps.get(1).unwrap().as_str();
            let cls_id = format!("java::{}", class_name);
            if graph.get_entity(&cls_id).is_none() {
                let cls_entity = CodeEntity::new(&cls_id, class_name, EntityType::Class, rel_path, 1, 1);
                graph.add_entity(cls_entity);
            }
            graph.add_edge(&pol_id, &cls_id, EdgeKind::References);
        }
    }
}
