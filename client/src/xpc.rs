//! XPC communication layer for macOS.

use std::ffi::{CStr, CString};
use std::ptr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use xpc_connection_sys::{
    dispatch_queue_create, xpc_array_append_value, xpc_array_create, xpc_array_get_count,
    xpc_array_get_value, xpc_bool_create, xpc_bool_get_value, xpc_connection_create_mach_service,
    xpc_connection_resume, xpc_connection_send_message_with_reply,
    xpc_connection_set_event_handler, xpc_connection_t, xpc_dictionary_create,
    xpc_dictionary_get_string, xpc_dictionary_get_value, xpc_dictionary_set_value,
    xpc_int64_create, xpc_int64_get_value, xpc_object_t, xpc_release, xpc_string_create,
};

use crate::error::{Error, Result};
use crate::types::*;

const SERVICE_NAME: &str = "com.firebox.service";

// ---------------------------------------------------------------------------
// XPC bindings
// ---------------------------------------------------------------------------

unsafe extern "C" {
    fn xpc_double_create(value: f64) -> xpc_object_t;
    fn xpc_double_get_value(object: xpc_object_t) -> f64;
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

unsafe fn cstr(s: &str) -> CString {
    CString::new(s).unwrap_or_default()
}

unsafe fn dict_new() -> xpc_object_t {
    unsafe { xpc_dictionary_create(ptr::null(), ptr::null_mut(), 0) }
}

unsafe fn array_new() -> xpc_object_t {
    unsafe { xpc_array_create(ptr::null_mut(), 0) }
}

unsafe fn dict_set_str(dict: xpc_object_t, key: &str, val: &str) {
    unsafe {
        let k = cstr(key);
        let v = cstr(val);
        let xv = xpc_string_create(v.as_ptr());
        xpc_dictionary_set_value(dict, k.as_ptr(), xv);
        xpc_release(xv);
    }
}

unsafe fn dict_set_bool(dict: xpc_object_t, key: &str, val: bool) {
    unsafe {
        let k = cstr(key);
        let xv = xpc_bool_create(val);
        xpc_dictionary_set_value(dict, k.as_ptr(), xv);
        xpc_release(xv);
    }
}

unsafe fn dict_set_i64(dict: xpc_object_t, key: &str, val: i64) {
    unsafe {
        let k = cstr(key);
        let xv = xpc_int64_create(val);
        xpc_dictionary_set_value(dict, k.as_ptr(), xv);
        xpc_release(xv);
    }
}

unsafe fn dict_set_f64(dict: xpc_object_t, key: &str, val: f64) {
    unsafe {
        let k = cstr(key);
        let xv = xpc_double_create(val);
        xpc_dictionary_set_value(dict, k.as_ptr(), xv);
        xpc_release(xv);
    }
}

unsafe fn dict_set_obj(dict: xpc_object_t, key: &str, val: xpc_object_t) {
    unsafe {
        let k = cstr(key);
        xpc_dictionary_set_value(dict, k.as_ptr(), val);
    }
}

unsafe fn array_append(arr: xpc_object_t, val: xpc_object_t) {
    unsafe {
        xpc_array_append_value(arr, val);
    }
}

unsafe fn dict_get_str(dict: xpc_object_t, key: &str) -> Option<String> {
    unsafe {
        let k = cstr(key);
        let ptr = xpc_dictionary_get_string(dict, k.as_ptr());
        if ptr.is_null() {
            None
        } else {
            CStr::from_ptr(ptr).to_str().ok().map(|s| s.to_string())
        }
    }
}

unsafe fn dict_get_bool(dict: xpc_object_t, key: &str) -> Option<bool> {
    unsafe {
        let k = cstr(key);
        let obj = xpc_dictionary_get_value(dict, k.as_ptr());
        if obj.is_null() {
            None
        } else {
            Some(xpc_bool_get_value(obj))
        }
    }
}

unsafe fn dict_get_i64(dict: xpc_object_t, key: &str) -> Option<i64> {
    unsafe {
        let k = cstr(key);
        let obj = xpc_dictionary_get_value(dict, k.as_ptr());
        if obj.is_null() {
            None
        } else {
            Some(xpc_int64_get_value(obj))
        }
    }
}

unsafe fn dict_get_f64(dict: xpc_object_t, key: &str) -> Option<f64> {
    unsafe {
        let k = cstr(key);
        let obj = xpc_dictionary_get_value(dict, k.as_ptr());
        if obj.is_null() {
            None
        } else {
            Some(xpc_double_get_value(obj))
        }
    }
}

unsafe fn dict_get_obj(dict: xpc_object_t, key: &str) -> Option<xpc_object_t> {
    unsafe {
        let k = cstr(key);
        let obj = xpc_dictionary_get_value(dict, k.as_ptr());
        if obj.is_null() { None } else { Some(obj) }
    }
}

unsafe fn array_len(arr: xpc_object_t) -> usize {
    unsafe { xpc_array_get_count(arr) as usize }
}

unsafe fn array_get(arr: xpc_object_t, idx: usize) -> Option<xpc_object_t> {
    unsafe {
        let obj = xpc_array_get_value(arr, idx as u64);
        if obj.is_null() { None } else { Some(obj) }
    }
}

// ---------------------------------------------------------------------------
// XPC Connection
// ---------------------------------------------------------------------------

pub struct XpcConnection {
    conn: xpc_connection_t,
}

impl XpcConnection {
    pub fn new() -> Result<Self> {
        unsafe {
            let service_name = cstr(SERVICE_NAME);
            let conn =
                xpc_connection_create_mach_service(service_name.as_ptr(), ptr::null_mut(), 0);

            if conn.is_null() {
                return Err(Error::Ipc("Failed to create XPC connection".to_string()));
            }

            let handler = block::ConcreteBlock::new(|_event: xpc_object_t| {
                // Connection-level events (errors, etc.)
            });
            let handler = handler.copy();
            xpc_connection_set_event_handler(conn, &*handler as *const _ as *mut _);
            xpc_connection_resume(conn);

            Ok(Self { conn })
        }
    }

    fn send_request(&self, request: xpc_object_t) -> Result<xpc_object_t> {
        unsafe {
            #[allow(clippy::arc_with_non_send_sync)]
            let response = Arc::new(Mutex::new(None));
            let response_clone = response.clone();

            let queue_name = cstr("com.firebox.client.reply");
            let queue = dispatch_queue_create(queue_name.as_ptr(), ptr::null_mut());

            let reply_handler = block::ConcreteBlock::new(move |reply: xpc_object_t| {
                let mut resp = response_clone.lock().unwrap();
                *resp = Some(reply);
            });
            let reply_handler = reply_handler.copy();

            xpc_connection_send_message_with_reply(
                self.conn,
                request,
                queue,
                &*reply_handler as *const _ as *mut _,
            );

            let timeout = Duration::from_secs(30);
            let start = std::time::Instant::now();

            loop {
                {
                    let resp = response.lock().unwrap();
                    if resp.is_some() {
                        return Ok(resp.unwrap());
                    }
                }

                if start.elapsed() > timeout {
                    return Err(Error::Timeout("XPC request timed out".to_string()));
                }

                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }

    fn check_response(&self, resp: xpc_object_t) -> Result<()> {
        unsafe {
            let result = dict_get_obj(resp, "result")
                .ok_or_else(|| Error::InvalidResponse("Missing result field".to_string()))?;

            let success = dict_get_bool(result, "success").unwrap_or(false);
            if !success {
                let msg =
                    dict_get_str(result, "message").unwrap_or_else(|| "Unknown error".to_string());
                return Err(Error::Service(msg));
            }

            Ok(())
        }
    }

    // -----------------------------------------------------------------------
    // Control API
    // -----------------------------------------------------------------------

    pub fn ping(&self) -> Result<()> {
        unsafe {
            let req = dict_new();
            dict_set_str(req, "cmd", "ping");
            let resp = self.send_request(req)?;
            self.check_response(resp)?;
            xpc_release(resp);
            Ok(())
        }
    }

    pub fn add_api_key_provider(
        &self,
        name: &str,
        provider_type: &str,
        api_key: &str,
        base_url: Option<&str>,
    ) -> Result<()> {
        unsafe {
            let req = dict_new();
            dict_set_str(req, "cmd", "add_api_key_provider");
            dict_set_str(req, "name", name);
            dict_set_str(req, "provider_type", provider_type);
            dict_set_str(req, "api_key", api_key);
            if let Some(url) = base_url {
                dict_set_str(req, "base_url", url);
            }
            let resp = self.send_request(req)?;
            self.check_response(resp)?;
            xpc_release(resp);
            Ok(())
        }
    }

    pub fn add_oauth_provider(&self, name: &str, provider_type: &str) -> Result<OAuthInitResponse> {
        unsafe {
            let req = dict_new();
            dict_set_str(req, "cmd", "add_oauth_provider");
            dict_set_str(req, "name", name);
            dict_set_str(req, "provider_type", provider_type);
            let resp = self.send_request(req)?;
            self.check_response(resp)?;

            let verification_uri = dict_get_str(resp, "verification_uri")
                .ok_or_else(|| Error::InvalidResponse("Missing verification_uri".to_string()))?;
            let user_code = dict_get_str(resp, "user_code")
                .ok_or_else(|| Error::InvalidResponse("Missing user_code".to_string()))?;
            let expires_in = dict_get_i64(resp, "expires_in")
                .ok_or_else(|| Error::InvalidResponse("Missing expires_in".to_string()))?
                as u64;

            xpc_release(resp);

            Ok(OAuthInitResponse {
                verification_uri,
                user_code,
                expires_in,
            })
        }
    }

    pub fn complete_oauth(&self, profile_id: &str) -> Result<()> {
        unsafe {
            let req = dict_new();
            dict_set_str(req, "cmd", "complete_oauth");
            dict_set_str(req, "profile_id", profile_id);
            let resp = self.send_request(req)?;
            self.check_response(resp)?;
            xpc_release(resp);
            Ok(())
        }
    }

    pub fn add_local_provider(&self, name: &str, base_url: &str) -> Result<()> {
        unsafe {
            let req = dict_new();
            dict_set_str(req, "cmd", "add_local_provider");
            dict_set_str(req, "name", name);
            dict_set_str(req, "base_url", base_url);
            let resp = self.send_request(req)?;
            self.check_response(resp)?;
            xpc_release(resp);
            Ok(())
        }
    }

    pub fn list_providers(&self) -> Result<Vec<ProviderInfo>> {
        unsafe {
            let req = dict_new();
            dict_set_str(req, "cmd", "list_providers");
            let resp = self.send_request(req)?;
            self.check_response(resp)?;

            let arr = dict_get_obj(resp, "providers")
                .ok_or_else(|| Error::InvalidResponse("Missing providers array".to_string()))?;

            let mut providers = Vec::new();
            let count = array_len(arr);
            for i in 0..count {
                if let Some(item) = array_get(arr, i) {
                    let profile_id = dict_get_str(item, "profile_id").unwrap_or_default();
                    let display_name = dict_get_str(item, "display_name").unwrap_or_default();
                    let provider_type = dict_get_str(item, "provider_type").unwrap_or_default();
                    let enabled = dict_get_bool(item, "enabled").unwrap_or(false);
                    let oauth_status = dict_get_str(item, "oauth_status");

                    providers.push(ProviderInfo {
                        profile_id,
                        display_name,
                        provider_type,
                        enabled,
                        oauth_status,
                    });
                }
            }

            xpc_release(resp);
            Ok(providers)
        }
    }

    pub fn delete_provider(&self, profile_id: &str) -> Result<()> {
        unsafe {
            let req = dict_new();
            dict_set_str(req, "cmd", "delete_provider");
            dict_set_str(req, "profile_id", profile_id);
            let resp = self.send_request(req)?;
            self.check_response(resp)?;
            xpc_release(resp);
            Ok(())
        }
    }

    pub fn get_all_models(&self, force_refresh: bool) -> Result<Vec<ModelInfo>> {
        unsafe {
            let req = dict_new();
            dict_set_str(req, "cmd", "get_all_models");
            dict_set_bool(req, "force_refresh", force_refresh);
            let resp = self.send_request(req)?;
            self.check_response(resp)?;

            let arr = dict_get_obj(resp, "models")
                .ok_or_else(|| Error::InvalidResponse("Missing models array".to_string()))?;

            let mut models = Vec::new();
            let count = array_len(arr);
            for i in 0..count {
                if let Some(item) = array_get(arr, i) {
                    let model_id = dict_get_str(item, "model_id").unwrap_or_default();
                    let provider_id = dict_get_str(item, "provider_id").unwrap_or_default();
                    let display_name = dict_get_str(item, "display_name").unwrap_or_default();
                    let enabled = dict_get_bool(item, "enabled").unwrap_or(false);

                    let caps_arr = dict_get_obj(item, "capabilities");
                    let mut capabilities = Vec::new();
                    if let Some(caps) = caps_arr {
                        let cap_count = array_len(caps);
                        for j in 0..cap_count {
                            if let Some(cap) = array_get(caps, j)
                                && let Some(s) = dict_get_str(cap, "capability")
                            {
                                capabilities.push(s);
                            }
                        }
                    }

                    models.push(ModelInfo {
                        model_id,
                        provider_id,
                        display_name,
                        enabled,
                        capabilities,
                    });
                }
            }

            xpc_release(resp);
            Ok(models)
        }
    }

    pub fn set_model_enabled(&self, model_id: &str, enabled: bool) -> Result<()> {
        unsafe {
            let req = dict_new();
            dict_set_str(req, "cmd", "set_model_enabled");
            dict_set_str(req, "model_id", model_id);
            dict_set_bool(req, "enabled", enabled);
            let resp = self.send_request(req)?;
            self.check_response(resp)?;
            xpc_release(resp);
            Ok(())
        }
    }

    // -----------------------------------------------------------------------
    // Capability API
    // -----------------------------------------------------------------------

    pub fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        unsafe {
            let req = dict_new();
            dict_set_str(req, "cmd", "complete");
            dict_set_str(req, "model_id", &request.model_id);

            // Encode messages
            let msgs_arr = array_new();
            for msg in &request.messages {
                let msg_obj = dict_new();
                dict_set_str(msg_obj, "role", &msg.role);
                dict_set_str(msg_obj, "content", &msg.content);
                if let Some(name) = &msg.name {
                    dict_set_str(msg_obj, "name", name);
                }
                array_append(msgs_arr, msg_obj);
                xpc_release(msg_obj);
            }
            dict_set_obj(req, "messages", msgs_arr);
            xpc_release(msgs_arr);

            // Encode tools
            if !request.tools.is_empty() {
                let tools_arr = array_new();
                for tool in &request.tools {
                    let tool_obj = dict_new();
                    dict_set_str(tool_obj, "type", &tool.tool_type);
                    dict_set_str(tool_obj, "name", &tool.function.name);
                    if let Some(desc) = &tool.function.description {
                        dict_set_str(tool_obj, "description", desc);
                    }
                    if let Some(params) = &tool.function.parameters
                        && let Ok(json) = serde_json::to_string(params)
                    {
                        dict_set_str(tool_obj, "parameters", &json);
                    }
                    array_append(tools_arr, tool_obj);
                    xpc_release(tool_obj);
                }
                dict_set_obj(req, "tools", tools_arr);
                xpc_release(tools_arr);
            }

            if let Some(temp) = request.temperature {
                dict_set_f64(req, "temperature", temp);
            }
            if let Some(max_tok) = request.max_tokens {
                dict_set_i64(req, "max_tokens", max_tok as i64);
            }

            let resp = self.send_request(req)?;
            self.check_response(resp)?;

            let content = dict_get_str(resp, "content").unwrap_or_default();
            let finish_reason = dict_get_str(resp, "finish_reason");

            let mut tool_calls = Vec::new();
            if let Some(tc_arr) = dict_get_obj(resp, "tool_calls") {
                let count = array_len(tc_arr);
                for i in 0..count {
                    if let Some(tc) = array_get(tc_arr, i) {
                        let id = dict_get_str(tc, "id").unwrap_or_default();
                        let call_type = dict_get_str(tc, "type").unwrap_or_default();
                        let name = dict_get_str(tc, "name").unwrap_or_default();
                        let arguments = dict_get_str(tc, "arguments").unwrap_or_default();

                        tool_calls.push(ToolCall {
                            id,
                            call_type,
                            function: ToolCallFunction { name, arguments },
                        });
                    }
                }
            }

            let usage = dict_get_obj(resp, "usage").map(|usage_obj| Usage {
                prompt_tokens: dict_get_i64(usage_obj, "prompt_tokens").unwrap_or(0) as u32,
                completion_tokens: dict_get_i64(usage_obj, "completion_tokens").unwrap_or(0) as u32,
                total_tokens: dict_get_i64(usage_obj, "total_tokens").unwrap_or(0) as u32,
            });

            xpc_release(resp);

            Ok(CompletionResponse {
                content,
                tool_calls,
                usage,
                finish_reason,
            })
        }
    }

    pub fn stream_start(&self, request: &CompletionRequest) -> Result<String> {
        unsafe {
            let req = dict_new();
            dict_set_str(req, "cmd", "stream_start");
            dict_set_str(req, "model_id", &request.model_id);

            // Encode messages
            let msgs_arr = array_new();
            for msg in &request.messages {
                let msg_obj = dict_new();
                dict_set_str(msg_obj, "role", &msg.role);
                dict_set_str(msg_obj, "content", &msg.content);
                if let Some(name) = &msg.name {
                    dict_set_str(msg_obj, "name", name);
                }
                array_append(msgs_arr, msg_obj);
                xpc_release(msg_obj);
            }
            dict_set_obj(req, "messages", msgs_arr);
            xpc_release(msgs_arr);

            // Encode tools
            if !request.tools.is_empty() {
                let tools_arr = array_new();
                for tool in &request.tools {
                    let tool_obj = dict_new();
                    dict_set_str(tool_obj, "type", &tool.tool_type);
                    dict_set_str(tool_obj, "name", &tool.function.name);
                    if let Some(desc) = &tool.function.description {
                        dict_set_str(tool_obj, "description", desc);
                    }
                    if let Some(params) = &tool.function.parameters
                        && let Ok(json) = serde_json::to_string(params)
                    {
                        dict_set_str(tool_obj, "parameters", &json);
                    }
                    array_append(tools_arr, tool_obj);
                    xpc_release(tool_obj);
                }
                dict_set_obj(req, "tools", tools_arr);
                xpc_release(tools_arr);
            }

            if let Some(temp) = request.temperature {
                dict_set_f64(req, "temperature", temp);
            }
            if let Some(max_tok) = request.max_tokens {
                dict_set_i64(req, "max_tokens", max_tok as i64);
            }

            let resp = self.send_request(req)?;
            self.check_response(resp)?;

            let stream_id = dict_get_str(resp, "stream_id")
                .ok_or_else(|| Error::InvalidResponse("Missing stream_id".to_string()))?;

            xpc_release(resp);
            Ok(stream_id)
        }
    }

    pub fn stream_poll(&self, stream_id: &str) -> Result<Vec<StreamChunk>> {
        unsafe {
            let req = dict_new();
            dict_set_str(req, "cmd", "stream_poll");
            dict_set_str(req, "stream_id", stream_id);
            let resp = self.send_request(req)?;
            self.check_response(resp)?;

            let mut chunks = Vec::new();

            if let Some(chunks_arr) = dict_get_obj(resp, "chunks") {
                let count = array_len(chunks_arr);
                for i in 0..count {
                    if let Some(chunk_obj) = array_get(chunks_arr, i)
                        && let Some(chunk_type) = dict_get_str(chunk_obj, "type")
                    {
                        match chunk_type.as_str() {
                            "delta" => {
                                if let Some(text) = dict_get_str(chunk_obj, "text") {
                                    chunks.push(StreamChunk::Delta(text));
                                }
                            }
                            "tool_calls" => {
                                let mut tool_calls = Vec::new();
                                if let Some(tc_arr) = dict_get_obj(chunk_obj, "tool_calls") {
                                    let tc_count = array_len(tc_arr);
                                    for j in 0..tc_count {
                                        if let Some(tc) = array_get(tc_arr, j) {
                                            let id = dict_get_str(tc, "id").unwrap_or_default();
                                            let call_type =
                                                dict_get_str(tc, "type").unwrap_or_default();
                                            let name = dict_get_str(tc, "name").unwrap_or_default();
                                            let arguments =
                                                dict_get_str(tc, "arguments").unwrap_or_default();

                                            tool_calls.push(ToolCall {
                                                id,
                                                call_type,
                                                function: ToolCallFunction { name, arguments },
                                            });
                                        }
                                    }
                                }
                                chunks.push(StreamChunk::ToolCalls(tool_calls));
                            }
                            "done" => {
                                let usage =
                                    dict_get_obj(chunk_obj, "usage").map(|usage_obj| Usage {
                                        prompt_tokens: dict_get_i64(usage_obj, "prompt_tokens")
                                            .unwrap_or(0)
                                            as u32,
                                        completion_tokens: dict_get_i64(
                                            usage_obj,
                                            "completion_tokens",
                                        )
                                        .unwrap_or(0)
                                            as u32,
                                        total_tokens: dict_get_i64(usage_obj, "total_tokens")
                                            .unwrap_or(0)
                                            as u32,
                                    });
                                let finish_reason = dict_get_str(chunk_obj, "finish_reason");
                                chunks.push(StreamChunk::Done {
                                    usage,
                                    finish_reason,
                                });
                            }
                            "error" => {
                                if let Some(msg) = dict_get_str(chunk_obj, "message") {
                                    chunks.push(StreamChunk::Error(msg));
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }

            xpc_release(resp);
            Ok(chunks)
        }
    }

    pub fn stream_cancel(&self, stream_id: &str) -> Result<()> {
        unsafe {
            let req = dict_new();
            dict_set_str(req, "cmd", "stream_cancel");
            dict_set_str(req, "stream_id", stream_id);
            let resp = self.send_request(req)?;
            self.check_response(resp)?;
            xpc_release(resp);
            Ok(())
        }
    }

    pub fn embed(&self, request: &EmbeddingRequest) -> Result<EmbeddingResponse> {
        unsafe {
            let req = dict_new();
            dict_set_str(req, "cmd", "embed");
            dict_set_str(req, "model_id", &request.model_id);

            let input_arr = array_new();
            for text in &request.input {
                let text_obj = dict_new();
                dict_set_str(text_obj, "text", text);
                array_append(input_arr, text_obj);
                xpc_release(text_obj);
            }
            dict_set_obj(req, "input", input_arr);
            xpc_release(input_arr);

            let resp = self.send_request(req)?;
            self.check_response(resp)?;

            let mut embeddings = Vec::new();
            if let Some(emb_arr) = dict_get_obj(resp, "embeddings") {
                let count = array_len(emb_arr);
                for i in 0..count {
                    if let Some(vec_arr) = array_get(emb_arr, i) {
                        let mut vec = Vec::new();
                        let vec_len = array_len(vec_arr);
                        for j in 0..vec_len {
                            if let Some(val_obj) = array_get(vec_arr, j)
                                && let Some(val) = dict_get_f64(val_obj, "value")
                            {
                                vec.push(val);
                            }
                        }
                        embeddings.push(vec);
                    }
                }
            }

            let usage = dict_get_obj(resp, "usage").map(|usage_obj| Usage {
                prompt_tokens: dict_get_i64(usage_obj, "prompt_tokens").unwrap_or(0) as u32,
                completion_tokens: dict_get_i64(usage_obj, "completion_tokens").unwrap_or(0) as u32,
                total_tokens: dict_get_i64(usage_obj, "total_tokens").unwrap_or(0) as u32,
            });

            xpc_release(resp);

            Ok(EmbeddingResponse { embeddings, usage })
        }
    }

    pub fn get_routing_rules(&self) -> Result<Vec<RoutingRule>> {
        unsafe {
            let req = dict_new();
            dict_set_str(req, "cmd", "get_routing_rules");
            let resp = self.send_request(req)?;
            self.check_response(resp)?;

            let mut rules = Vec::new();
            if let Some(rules_arr) = dict_get_obj(resp, "rules") {
                let count = array_len(rules_arr);
                for i in 0..count {
                    if let Some(rule) = array_get(rules_arr, i) {
                        let pattern = dict_get_str(rule, "pattern").unwrap_or_default();
                        let target_provider =
                            dict_get_str(rule, "target_provider").unwrap_or_default();
                        rules.push(RoutingRule {
                            pattern,
                            target_provider,
                        });
                    }
                }
            }

            xpc_release(resp);
            Ok(rules)
        }
    }

    #[allow(dead_code)]
    pub fn add_routing_rule(&self, pattern: &str, target_provider: &str) -> Result<()> {
        unsafe {
            let req = dict_new();
            dict_set_str(req, "cmd", "add_routing_rule");
            dict_set_str(req, "pattern", pattern);
            dict_set_str(req, "target_provider", target_provider);
            let resp = self.send_request(req)?;
            self.check_response(resp)?;
            xpc_release(resp);
            Ok(())
        }
    }

    #[allow(dead_code)]
    pub fn remove_routing_rule(&self, pattern: &str) -> Result<()> {
        unsafe {
            let req = dict_new();
            dict_set_str(req, "cmd", "remove_routing_rule");
            dict_set_str(req, "pattern", pattern);
            let resp = self.send_request(req)?;
            self.check_response(resp)?;
            xpc_release(resp);
            Ok(())
        }
    }

    pub fn get_metrics(&self) -> Result<Vec<ProviderMetrics>> {
        unsafe {
            let req = dict_new();
            dict_set_str(req, "cmd", "get_metrics");
            let resp = self.send_request(req)?;
            self.check_response(resp)?;

            let mut metrics = Vec::new();
            if let Some(metrics_arr) = dict_get_obj(resp, "metrics") {
                let count = array_len(metrics_arr);
                for i in 0..count {
                    if let Some(m) = array_get(metrics_arr, i) {
                        let provider_id = dict_get_str(m, "provider_id").unwrap_or_default();
                        let requests_count = dict_get_i64(m, "requests_count").unwrap_or(0) as u64;
                        let errors_count = dict_get_i64(m, "errors_count").unwrap_or(0) as u64;
                        let total_prompt_tokens =
                            dict_get_i64(m, "total_prompt_tokens").unwrap_or(0) as u64;
                        let total_completion_tokens =
                            dict_get_i64(m, "total_completion_tokens").unwrap_or(0) as u64;

                        metrics.push(ProviderMetrics {
                            provider_id,
                            requests_count,
                            errors_count,
                            total_prompt_tokens,
                            total_completion_tokens,
                        });
                    }
                }
            }

            xpc_release(resp);
            Ok(metrics)
        }
    }

    pub fn get_allowlist(&self) -> Result<Vec<AllowlistEntry>> {
        unsafe {
            let req = dict_new();
            dict_set_str(req, "cmd", "get_allowlist");
            let resp = self.send_request(req)?;
            self.check_response(resp)?;

            let mut entries = Vec::new();
            if let Some(apps_arr) = dict_get_obj(resp, "apps") {
                let count = array_len(apps_arr);
                for i in 0..count {
                    if let Some(app) = array_get(apps_arr, i) {
                        let app_path = dict_get_str(app, "app_path").unwrap_or_default();
                        let display_name = dict_get_str(app, "display_name").unwrap_or_default();
                        entries.push(AllowlistEntry {
                            app_path,
                            display_name,
                        });
                    }
                }
            }

            xpc_release(resp);
            Ok(entries)
        }
    }

    pub fn remove_from_allowlist(&self, app_path: &str) -> Result<()> {
        unsafe {
            let req = dict_new();
            dict_set_str(req, "cmd", "remove_from_allowlist");
            dict_set_str(req, "app_path", app_path);
            let resp = self.send_request(req)?;
            self.check_response(resp)?;
            xpc_release(resp);
            Ok(())
        }
    }

    pub fn set_routing_rules(&self, rules: &[RoutingRule]) -> Result<()> {
        unsafe {
            let req = dict_new();
            dict_set_str(req, "cmd", "set_routing_rules");

            let rules_arr = array_new();
            for rule in rules {
                let rule_obj = dict_new();
                dict_set_str(rule_obj, "pattern", &rule.pattern);
                dict_set_str(rule_obj, "target_provider", &rule.target_provider);
                array_append(rules_arr, rule_obj);
                xpc_release(rule_obj);
            }
            dict_set_obj(req, "rules", rules_arr);
            xpc_release(rules_arr);

            let resp = self.send_request(req)?;
            self.check_response(resp)?;
            xpc_release(resp);
            Ok(())
        }
    }

    pub fn get_provider_metrics(&self) -> Result<Vec<ProviderMetrics>> {
        self.get_metrics()
    }
}
