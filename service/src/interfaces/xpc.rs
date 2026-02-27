//! macOS XPC IPC server for FireBox management protocol.

use std::ffi::{CStr, CString};
use std::os::raw::c_void;
use std::ptr;

use xpc_connection_sys::{
    XPC_CONNECTION_MACH_SERVICE_LISTENER,
    dispatch_queue_create,
    xpc_array_append_value, xpc_array_create, xpc_array_get_count, xpc_array_get_value,
    xpc_bool_create, xpc_bool_get_value,
    xpc_connection_create_mach_service, xpc_connection_resume,
    xpc_connection_send_message, xpc_connection_set_event_handler,
    xpc_connection_t, xpc_object_t,
    xpc_dictionary_create, xpc_dictionary_create_reply,
    xpc_dictionary_get_string, xpc_dictionary_get_value,
    xpc_dictionary_set_value,
    xpc_get_type, xpc_int64_create, xpc_int64_get_value,
    xpc_release, xpc_string_create, xpc_string_get_string_ptr,
    _xpc_type_connection, _xpc_type_error,
};

use crate::middleware::{config, metrics};
use crate::middleware::route;
use crate::providers::config as pconfig;

pub const SERVICE_NAME: &str = "com.firebox.service\0";

// ---------------------------------------------------------------------------
// Unsafe helpers
// ---------------------------------------------------------------------------

unsafe fn cstr(s: &str) -> CString {
    CString::new(s).unwrap_or_default()
}

unsafe fn new_dict() -> xpc_object_t {
    xpc_dictionary_create(ptr::null(), ptr::null_mut() as *mut *mut c_void, 0)
}

unsafe fn new_array() -> xpc_object_t {
    xpc_array_create(ptr::null_mut() as *mut *mut _, 0)
}

unsafe fn dict_set_str(dict: xpc_object_t, key: &str, val: &str) {
    let k = cstr(key);
    let v = cstr(val);
    let xv = xpc_string_create(v.as_ptr());
    xpc_dictionary_set_value(dict, k.as_ptr(), xv);
    xpc_release(xv);
}

unsafe fn dict_set_bool(dict: xpc_object_t, key: &str, val: bool) {
    let k = cstr(key);
    let xv = xpc_bool_create(val);
    xpc_dictionary_set_value(dict, k.as_ptr(), xv);
    xpc_release(xv);
}

unsafe fn dict_set_i64(dict: xpc_object_t, key: &str, val: i64) {
    let k = cstr(key);
    let xv = xpc_int64_create(val);
    xpc_dictionary_set_value(dict, k.as_ptr(), xv);
    xpc_release(xv);
}

unsafe fn dict_set_obj(dict: xpc_object_t, key: &str, val: xpc_object_t) {
    let k = cstr(key);
    xpc_dictionary_set_value(dict, k.as_ptr(), val);
    xpc_release(val);
}

unsafe fn array_append_obj(arr: xpc_object_t, val: xpc_object_t) {
    xpc_array_append_value(arr, val);
    xpc_release(val);
}

unsafe fn dict_get_str(dict: xpc_object_t, key: &str) -> Option<String> {
    let k = cstr(key);
    let ptr = xpc_dictionary_get_string(dict, k.as_ptr());
    if ptr.is_null() {
        None
    } else {
        Some(CStr::from_ptr(ptr).to_string_lossy().into_owned())
    }
}

unsafe fn dict_get_obj(dict: xpc_object_t, key: &str) -> Option<xpc_object_t> {
    let k = cstr(key);
    let v = xpc_dictionary_get_value(dict, k.as_ptr());
    if v.is_null() { None } else { Some(v) }
}

unsafe fn dict_get_bool(dict: xpc_object_t, key: &str) -> Option<bool> {
    dict_get_obj(dict, key).map(|v| xpc_bool_get_value(v))
}

unsafe fn dict_get_i64(dict: xpc_object_t, key: &str) -> Option<i64> {
    dict_get_obj(dict, key).map(|v| xpc_int64_get_value(v))
}

// ---------------------------------------------------------------------------
// Command handlers
// ---------------------------------------------------------------------------

async fn handle_request(request: xpc_object_t) -> xpc_object_t {
    let cmd = unsafe { dict_get_str(request, "cmd") }.unwrap_or_default();
    match cmd.as_str() {
        "ping"                 => handle_ping(),
        "get_metrics"          => handle_get_metrics(),
        "list_providers"       => handle_list_providers().await,
        "add_api_key_provider" => handle_add_api_key_provider(request).await,
        "delete_provider"      => handle_delete_provider(request).await,
        "list_route_rules"     => handle_list_route_rules().await,
        "set_route_rule"       => handle_set_route_rule(request).await,
        "delete_route_rule"    => handle_delete_route_rule(request).await,
        "list_connections"     => handle_list_connections(),
        _ => unsafe {
            let r = new_dict();
            dict_set_bool(r, "success", false);
            dict_set_str(r, "message", &format!("unknown command: {cmd}"));
            r
        },
    }
}

fn handle_ping() -> xpc_object_t {
    unsafe {
        let r = new_dict();
        dict_set_bool(r, "success", true);
        r
    }
}

fn handle_get_metrics() -> xpc_object_t {
    let snap = metrics::get_snapshot();
    unsafe {
        let r = new_dict();
        dict_set_bool(r, "success", true);
        dict_set_i64(r, "window_start_ms",         snap.window_start_ms as i64);
        dict_set_i64(r, "window_end_ms",           snap.window_end_ms as i64);
        dict_set_i64(r, "requests_total",          snap.requests_total as i64);
        dict_set_i64(r, "requests_failed",         snap.requests_failed as i64);
        dict_set_i64(r, "prompt_tokens_total",     snap.prompt_tokens_total as i64);
        dict_set_i64(r, "completion_tokens_total", snap.completion_tokens_total as i64);
        dict_set_i64(r, "cost_total_microcents",   (snap.cost_total * 1_000_000.0) as i64);
        r
    }
}

async fn handle_list_providers() -> xpc_object_t {
    let index = pconfig::load_provider_index().await;
    let cfg_data = config::load_config().await.unwrap_or_default();

    unsafe {
        let r = new_dict();
        dict_set_bool(r, "success", true);
        let arr = new_array();

        for profile_id in &index {
            let json_val: Option<serde_json::Value> = cfg_data
                .providers
                .get(profile_id)
                .and_then(|j| serde_json::from_str(j).ok());

            let type_slug = json_val
                .as_ref()
                .and_then(|v| v.get("provider").and_then(|p| p.as_str()))
                .unwrap_or("unknown")
                .to_string();

            let display_name = cfg_data
                .display_names
                .get(profile_id)
                .cloned()
                .unwrap_or_else(|| profile_id.clone());

            let base_url = json_val
                .as_ref()
                .and_then(|v| v.get("base_url").and_then(|u| u.as_str()))
                .unwrap_or("")
                .to_string();

            let provider_type: i64 = match type_slug.as_str() {
                "copilot" | "dashscope" => 2,
                "llamacpp" => 3,
                _ => 1,
            };

            let entry = new_dict();
            dict_set_str(entry, "provider_id", profile_id);
            dict_set_str(entry, "name",        &display_name);
            dict_set_i64(entry, "type",        provider_type);
            dict_set_str(entry, "base_url",    &base_url);
            array_append_obj(arr, entry);
        }

        dict_set_obj(r, "providers", arr);
        r
    }
}

async fn handle_add_api_key_provider(req: xpc_object_t) -> xpc_object_t {
    let name     = unsafe { dict_get_str(req, "name") }.unwrap_or_default();
    let ptype    = unsafe { dict_get_str(req, "provider_type") }.unwrap_or_default();
    let api_key  = unsafe { dict_get_str(req, "api_key") }.unwrap_or_default();
    let base_url = unsafe { dict_get_str(req, "base_url") };

    if name.is_empty() || ptype.is_empty() {
        return unsafe {
            let r = new_dict();
            dict_set_bool(r, "success", false);
            dict_set_str(r, "message", "name and provider_type are required");
            r
        };
    }

    let cfg = match ptype.as_str() {
        "openai"    => pconfig::ProviderConfig::openai(&api_key, base_url),
        "anthropic" => pconfig::ProviderConfig::anthropic(&api_key, base_url),
        "ollama"    => pconfig::ProviderConfig::ollama(base_url),
        other => {
            return unsafe {
                let r = new_dict();
                dict_set_bool(r, "success", false);
                dict_set_str(r, "message", &format!("unsupported provider_type: {other}"));
                r
            };
        }
    };

    let profile_id = name.to_lowercase().replace(' ', "_");

    match pconfig::configure_provider(&profile_id, &cfg).await {
        Ok(_) => {
            let _ = pconfig::add_to_provider_index(&profile_id).await;
            let pid = profile_id.clone();
            let nm  = name.clone();
            let _ = config::update_config(move |d| {
                d.display_names.insert(pid, nm);
            }).await;
            unsafe {
                let r = new_dict();
                dict_set_bool(r, "success", true);
                dict_set_str(r, "provider_id", &profile_id);
                r
            }
        }
        Err(e) => unsafe {
            let r = new_dict();
            dict_set_bool(r, "success", false);
            dict_set_str(r, "message", &e.to_string());
            r
        },
    }
}

async fn handle_delete_provider(req: xpc_object_t) -> xpc_object_t {
    let provider_id = unsafe { dict_get_str(req, "provider_id") }.unwrap_or_default();
    if provider_id.is_empty() {
        return unsafe {
            let r = new_dict();
            dict_set_bool(r, "success", false);
            dict_set_str(r, "message", "provider_id is required");
            r
        };
    }
    let _ = pconfig::remove_provider(&provider_id).await;
    let _ = pconfig::remove_from_provider_index(&provider_id).await;
    unsafe {
        let r = new_dict();
        dict_set_bool(r, "success", true);
        r
    }
}

async fn handle_list_route_rules() -> xpc_object_t {
    match route::get_all_rules().await {
        Ok(rules) => unsafe {
            let r = new_dict();
            dict_set_bool(r, "success", true);
            let arr = new_array();
            for rule in &rules {
                let entry = new_dict();
                dict_set_str(entry, "virtual_model_id", &rule.virtual_model_id);
                dict_set_str(entry, "display_name",     &rule.display_name);
                // targets array
                let targets_arr = new_array();
                for t in &rule.targets {
                    let te = new_dict();
                    dict_set_str(te, "provider_id", &t.provider_id);
                    dict_set_str(te, "model_id",    &t.model_id);
                    array_append_obj(targets_arr, te);
                }
                dict_set_obj(entry, "targets", targets_arr);
                array_append_obj(arr, entry);
            }
            dict_set_obj(r, "rules", arr);
            r
        },
        Err(e) => unsafe {
            let r = new_dict();
            dict_set_bool(r, "success", false);
            dict_set_str(r, "message", &e.to_string());
            r
        },
    }
}

async fn handle_set_route_rule(req: xpc_object_t) -> xpc_object_t {
    let virtual_model_id = unsafe { dict_get_str(req, "virtual_model_id") }.unwrap_or_default();
    let display_name     = unsafe { dict_get_str(req, "display_name") }.unwrap_or_else(|| virtual_model_id.clone());

    if virtual_model_id.is_empty() {
        return unsafe {
            let r = new_dict();
            dict_set_bool(r, "success", false);
            dict_set_str(r, "message", "virtual_model_id is required");
            r
        };
    }

    // Parse targets array from XPC
    let mut targets = Vec::new();
    unsafe {
        if let Some(arr) = dict_get_obj(req, "targets") {
            let count = xpc_array_get_count(arr) as usize;
            for i in 0..count {
                let entry = xpc_array_get_value(arr, i as u64);
                if !entry.is_null() {
                    let provider_id = dict_get_str(entry, "provider_id").unwrap_or_default();
                    let model_id    = dict_get_str(entry, "model_id").unwrap_or_default();
                    if !provider_id.is_empty() && !model_id.is_empty() {
                        targets.push(route::RouteTarget { provider_id, model_id });
                    }
                }
            }
        }
    }

    match route::set_route_rules(
        &virtual_model_id,
        &display_name,
        route::RouteCapabilities::default(),
        route::RouteMetadata::default(),
        targets,
    ).await {
        Ok(_) => unsafe {
            let r = new_dict();
            dict_set_bool(r, "success", true);
            r
        },
        Err(e) => unsafe {
            let r = new_dict();
            dict_set_bool(r, "success", false);
            dict_set_str(r, "message", &e.to_string());
            r
        },
    }
}

async fn handle_delete_route_rule(req: xpc_object_t) -> xpc_object_t {
    let alias = unsafe { dict_get_str(req, "virtual_model_id") }.unwrap_or_default();
    match route::delete_route_rules(&alias).await {
        Ok(_) => unsafe {
            let r = new_dict();
            dict_set_bool(r, "success", true);
            r
        },
        Err(e) => unsafe {
            let r = new_dict();
            dict_set_bool(r, "success", false);
            dict_set_str(r, "message", &e.to_string());
            r
        },
    }
}

fn handle_list_connections() -> xpc_object_t {
    // Connections are tracked by the HTTP layer; return empty list for now.
    unsafe {
        let r = new_dict();
        dict_set_bool(r, "success", true);
        let arr = new_array();
        dict_set_obj(r, "connections", arr);
        r
    }
}

// ---------------------------------------------------------------------------
// XPC listener
// ---------------------------------------------------------------------------

/// Start the XPC listener. Blocks the calling thread (runs the dispatch main loop).
pub fn run_listener() {
    use block::ConcreteBlock;

    let service_name = CString::new("com.firebox.service").expect("CString");

    unsafe {
        let queue_label = CString::new("com.firebox.service.queue").expect("CString");
        let queue = dispatch_queue_create(queue_label.as_ptr(), ptr::null_mut());

        let listener = xpc_connection_create_mach_service(
            service_name.as_ptr(),
            queue,
            u64::from(XPC_CONNECTION_MACH_SERVICE_LISTENER),
        );

        // Handler for each new client connection
        let mut conn_handler = ConcreteBlock::new(move |client: xpc_connection_t| {
            // Handler for messages from this client
            let mut msg_handler = ConcreteBlock::new(move |event: xpc_object_t| {
                let event_type = xpc_get_type(event);
                if event_type == &_xpc_type_error as *const _ {
                    return;
                }
                if event_type != &_xpc_type_connection as *const _ {
                    // All XPC callbacks run on the dispatch queue thread.
                    // Build a single-threaded tokio runtime here so raw pointers
                    // never need to be Send.
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .expect("tokio runtime");
                    let reply_body = rt.block_on(handle_request(event));
                    let reply = xpc_dictionary_create_reply(event);
                    if !reply.is_null() {
                        let k = CString::new("body").unwrap();
                        xpc_dictionary_set_value(reply, k.as_ptr(), reply_body);
                        xpc_release(reply_body);
                        xpc_connection_send_message(client, reply);
                        xpc_release(reply);
                    } else {
                        xpc_release(reply_body);
                    }
                }
            });
            let msg_block = &mut *msg_handler;
            xpc_connection_set_event_handler(
                client,
                msg_block as *mut block::Block<_, _> as *mut c_void,
            );
            xpc_connection_resume(client);
        });

        let conn_block = &mut *conn_handler;
        xpc_connection_set_event_handler(
            listener,
            conn_block as *mut block::Block<_, _> as *mut c_void,
        );
        xpc_connection_resume(listener);

        // Block the thread forever so the dispatch queue keeps running
        loop {
            std::thread::sleep(std::time::Duration::from_secs(3600));
        }
    }
}
