//! CAPABILITY protocol handlers — model listing, completion, streaming, embeddings.

use std::collections::{HashMap, VecDeque};
use std::ffi::CStr;
use std::sync::{Arc, LazyLock};

use tokio::sync::{Mutex, Notify};
use tokio::task::JoinHandle;
use xpc_connection_sys::xpc_object_t;

use crate::middleware::{metadata as meta_mw, route};
use crate::providers::config as pconfig;
use crate::providers::{
    ChatMessage, CompletionRequest, EmbeddingRequest, StreamEvent, Tool, ToolCall,
    ToolCallFunction, ToolFunction, Usage,
};

use super::codec::*;
use super::xpc::GLOBAL_RT;

// ---------------------------------------------------------------------------
// Input validation limits (DoS protection)
// ---------------------------------------------------------------------------

/// Maximum length (bytes) for string fields like model_id, stream_id.
const MAX_ID_LENGTH: usize = 512;

/// Maximum number of messages in a single completion request.
const MAX_MESSAGES: usize = 256;

// ---------------------------------------------------------------------------
// Stream session state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum StreamChunk {
    Delta(String),
    ToolCalls(Vec<ToolCall>),
    Done {
        usage: Option<Usage>,
        finish_reason: Option<String>,
    },
    Error(String),
}

/// Mutable stream state protected by a single lock to avoid race conditions
/// between `pending` and `done`.
pub struct StreamState {
    pub pending: VecDeque<StreamChunk>,
    pub done: bool,
}

pub struct StreamSession {
    pub provider_id: String,
    pub model_id: String,
    pub messages: Mutex<Vec<ChatMessage>>,
    pub tools: Mutex<Vec<Tool>>,
    pub state: Mutex<StreamState>,
    pub notify: Notify,
    pub task: Mutex<Option<JoinHandle<()>>>,
}

impl StreamSession {
    fn new(provider_id: String, model_id: String) -> Arc<Self> {
        Arc::new(Self {
            provider_id,
            model_id,
            messages: Mutex::new(Vec::new()),
            tools: Mutex::new(Vec::new()),
            state: Mutex::new(StreamState {
                pending: VecDeque::new(),
                done: false,
            }),
            notify: Notify::new(),
            task: Mutex::new(None),
        })
    }
}

static SESSIONS: LazyLock<Mutex<HashMap<String, Arc<StreamSession>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn new_stream_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let addr = &nanos as *const u32 as usize;
    format!("s-{addr:x}-{nanos:x}")
}

// ---------------------------------------------------------------------------
// list_available_models
// ---------------------------------------------------------------------------

pub async fn handle_list_available_models() -> xpc_object_t {
    let index = pconfig::load_provider_index().await;
    let mut models_out: Vec<(String, String, String, Option<u32>)> = Vec::new();

    for profile_id in &index {
        let provider = match pconfig::load_provider(profile_id).await {
            Ok(p) => p,
            Err(_) => continue,
        };
        let models = provider.list_models_dyn().await.unwrap_or_default();
        for m in models {
            if route::is_model_enabled(profile_id, &m.id).await {
                models_out.push((m.id, profile_id.clone(), m.owner, m.context_window));
            }
        }
    }

    unsafe {
        let body = dict_new();
        let arr = array_new();
        for (mid, pid, owner, cw) in &models_out {
            let entry = dict_new();
            dict_set_str(entry, "model_id", mid);
            dict_set_str(entry, "provider_id", pid);
            dict_set_str(entry, "owner", owner);
            if let Some(c) = cw {
                dict_set_i64(entry, "context_window", *c as i64);
            }
            array_append(arr, entry);
        }
        dict_set_obj(body, "models", arr);
        response_ok(body)
    }
}

// ---------------------------------------------------------------------------
// get_model_metadata
// ---------------------------------------------------------------------------

pub async fn handle_get_model_metadata(req: xpc_object_t) -> xpc_object_t {
    let model_id = unsafe { dict_get_str(req, "model_id").unwrap_or_default() };
    if model_id.is_empty() {
        return unsafe { response_err("model_id is required") };
    }

    let mut mgr = meta_mw::MetadataManager::new();

    let found = mgr.find_model(&model_id).await.unwrap_or(None);
    unsafe {
        let body = dict_new();
        let obj = dict_new();
        if let Some(model) = found {
            dict_set_str(obj, "id", &model.id);
            dict_set_str(obj, "name", &model.name);
            if let Some(ref lim) = model.limit {
                if lim.context > 0 {
                    dict_set_i64(obj, "context_window", lim.context as i64);
                }
            }
            let supports_vision = model
                .modalities
                .as_ref()
                .map(|m| m.input.iter().any(|s| s == "image"))
                .unwrap_or(false);
            dict_set_bool(obj, "supports_vision", supports_vision);
            dict_set_bool(obj, "supports_tool_calling", model.tool_call);
        } else {
            dict_set_str(obj, "id", &model_id);
            dict_set_str(obj, "name", &model_id);
        }
        dict_set_obj(body, "model", obj);
        response_ok(body)
    }
}

// ---------------------------------------------------------------------------
// complete (non-streaming)
// ---------------------------------------------------------------------------

pub async fn handle_complete(req: xpc_object_t) -> xpc_object_t {
    let (model_id, temperature, max_tokens) = unsafe {
        (
            dict_get_str(req, "model_id").unwrap_or_default(),
            dict_get_f64(req, "temperature"),
            dict_get_i64(req, "max_tokens").map(|v| v as u32),
        )
    };

    if model_id.is_empty() {
        return unsafe { response_err("model_id is required") };
    }
    if model_id.len() > MAX_ID_LENGTH {
        return unsafe { response_err("model_id too long") };
    }

    let messages = unsafe { decode_messages(req) };
    if messages.len() > MAX_MESSAGES {
        return unsafe { response_err("too many messages") };
    }
    let tools = unsafe { decode_tools(req) };

    let (provider_id, real_model) = match route::resolve_alias(&model_id).await {
        Ok(pair) => pair,
        Err(e) => return unsafe { response_err(&e.to_string()) },
    };

    let provider = match pconfig::load_provider(&provider_id).await {
        Ok(p) => p,
        Err(e) => return unsafe { response_err(&e.to_string()) },
    };

    let request = CompletionRequest {
        model: real_model,
        messages,
        max_tokens,
        temperature,
        stream: false,
        tools: if tools.is_empty() { None } else { Some(tools) },
    };

    match provider.complete_dyn("ipc", &request).await {
        Ok(resp) => {
            let choice = resp.choices.first();
            unsafe {
                let body = dict_new();
                let completion = dict_new();
                if let Some(c) = choice {
                    dict_set_str(completion, "role", &c.message.role);
                    dict_set_str(completion, "content", &c.message.content);
                    dict_set_str(
                        completion,
                        "finish_reason",
                        c.finish_reason.as_deref().unwrap_or("stop"),
                    );
                    if let Some(fr) = c.finish_reason.as_deref() {
                        dict_set_str(body, "finish_reason", fr);
                    }
                    if let Some(ref tcs) = c.message.tool_calls {
                        let tc_arr = array_new();
                        for tc in tcs {
                            array_append(tc_arr, encode_tool_call(tc));
                        }
                        dict_set_obj(completion, "tool_calls", tc_arr);
                    }
                }
                dict_set_obj(body, "completion", completion);
                if let Some(u) = resp.usage {
                    let uo = dict_new();
                    dict_set_i64(uo, "prompt_tokens", u.prompt_tokens as i64);
                    dict_set_i64(uo, "completion_tokens", u.completion_tokens as i64);
                    dict_set_i64(uo, "total_tokens", u.total_tokens as i64);
                    dict_set_obj(body, "usage", uo);
                    let pt = u.prompt_tokens as u64;
                    let ct = u.completion_tokens as u64;
                    GLOBAL_RT.spawn(async move {
                        crate::middleware::metrics::record_minute_bucket(1, 0, pt, ct, 0.0).await;
                    });
                }
                response_ok(body)
            }
        }
        Err(e) => {
            GLOBAL_RT.spawn(async move {
                crate::middleware::metrics::record_minute_bucket(1, 1, 0, 0, 0.0).await;
            });
            unsafe { response_err(&e.to_string()) }
        }
    }
}

// ---------------------------------------------------------------------------
// create_stream
// ---------------------------------------------------------------------------

pub async fn handle_create_stream(req: xpc_object_t) -> xpc_object_t {
    let (model_id, temperature, max_tokens) = unsafe {
        (
            dict_get_str(req, "model_id").unwrap_or_default(),
            dict_get_f64(req, "temperature"),
            dict_get_i64(req, "max_tokens").map(|v| v as u32),
        )
    };

    if model_id.is_empty() {
        return unsafe { response_err("model_id is required") };
    }

    let (provider_id, real_model) = match route::resolve_alias(&model_id).await {
        Ok(pair) => pair,
        Err(e) => return unsafe { response_err(&e.to_string()) },
    };

    let session = StreamSession::new(provider_id, real_model);
    let stream_id = new_stream_id();
    SESSIONS.lock().await.insert(stream_id.clone(), session);

    unsafe {
        let body = dict_new();
        dict_set_str(body, "stream_id", &stream_id);
        if let Some(t) = temperature {
            dict_set_f64(body, "temperature", t);
        }
        if let Some(m) = max_tokens {
            dict_set_i64(body, "max_tokens", m as i64);
        }
        response_ok(body)
    }
}

// ---------------------------------------------------------------------------
// send_message
// ---------------------------------------------------------------------------

pub async fn handle_send_message(req: xpc_object_t) -> xpc_object_t {
    let (stream_id, temperature, max_tokens) = unsafe {
        (
            dict_get_str(req, "stream_id").unwrap_or_default(),
            dict_get_f64(req, "temperature"),
            dict_get_i64(req, "max_tokens").map(|v| v as u32),
        )
    };

    if stream_id.is_empty() {
        return unsafe { response_err("stream_id is required") };
    }

    let session = {
        let reg = SESSIONS.lock().await;
        match reg.get(&stream_id) {
            Some(s) => Arc::clone(s),
            None => return unsafe { response_err("stream_id not found") },
        }
    };

    let new_msgs = unsafe { decode_messages_or_single(req) };
    let new_tools = unsafe { decode_tools(req) };

    {
        let mut msgs = session.messages.lock().await;
        let mut tools = session.tools.lock().await;
        msgs.extend(new_msgs);
        if !new_tools.is_empty() {
            *tools = new_tools;
        }
    }

    session.state.lock().await.done = false;

    let provider_id = session.provider_id.clone();
    let real_model = session.model_id.clone();
    let all_msgs = session.messages.lock().await.clone();
    let all_tools = session.tools.lock().await.clone();
    let sess_clone = Arc::clone(&session);

    let task = GLOBAL_RT.spawn(async move {
        let provider = match pconfig::load_provider(&provider_id).await {
            Ok(p) => p,
            Err(e) => {
                let mut st = sess_clone.state.lock().await;
                st.pending.push_back(StreamChunk::Error(e.to_string()));
                st.done = true;
                sess_clone.notify.notify_one();
                return;
            }
        };

        let request = CompletionRequest {
            model: real_model,
            messages: all_msgs,
            max_tokens,
            temperature,
            stream: true,
            tools: if all_tools.is_empty() {
                None
            } else {
                Some(all_tools)
            },
        };

        match provider.complete_stream_dyn("ipc-stream", &request).await {
            Ok(stream) => {
                use tokio_stream::StreamExt as _;
                let mut tool_buf: Vec<ToolCall> = Vec::new();
                tokio::pin!(stream);

                while let Some(event) = stream.next().await {
                    match event {
                        Ok(StreamEvent::Delta { content }) => {
                            sess_clone
                                .state
                                .lock()
                                .await
                                .pending
                                .push_back(StreamChunk::Delta(content));
                            sess_clone.notify.notify_one();
                        }
                        Ok(StreamEvent::ToolCalls { tool_calls }) => {
                            tool_buf.extend(tool_calls);
                        }
                        Ok(StreamEvent::Done) => {
                            let mut st = sess_clone.state.lock().await;
                            if !tool_buf.is_empty() {
                                let tcs = std::mem::take(&mut tool_buf);
                                st.pending.push_back(StreamChunk::ToolCalls(tcs));
                                sess_clone.notify.notify_one();
                            }
                            st.pending.push_back(StreamChunk::Done {
                                usage: None,
                                finish_reason: Some("stop".to_string()),
                            });
                            sess_clone.notify.notify_one();
                            break;
                        }
                        Ok(StreamEvent::Error { message }) => {
                            sess_clone
                                .state
                                .lock()
                                .await
                                .pending
                                .push_back(StreamChunk::Error(message));
                            sess_clone.notify.notify_one();
                            break;
                        }
                        Err(e) => {
                            sess_clone
                                .state
                                .lock()
                                .await
                                .pending
                                .push_back(StreamChunk::Error(e.to_string()));
                            sess_clone.notify.notify_one();
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                sess_clone
                    .state
                    .lock()
                    .await
                    .pending
                    .push_back(StreamChunk::Error(e.to_string()));
                sess_clone.notify.notify_one();
            }
        }
        sess_clone.state.lock().await.done = true;
    });

    *session.task.lock().await = Some(task);
    unsafe { response_ok(dict_new()) }
}

// ---------------------------------------------------------------------------
// receive_stream (long-polling)
// ---------------------------------------------------------------------------

pub async fn handle_receive_stream(req: xpc_object_t) -> xpc_object_t {
    let (stream_id, timeout_ms) = unsafe {
        (
            dict_get_str(req, "stream_id").unwrap_or_default(),
            dict_get_i64(req, "timeout_ms").unwrap_or(1000) as u64,
        )
    };

    if stream_id.is_empty() {
        return unsafe { response_err("stream_id is required") };
    }

    let session = {
        let reg = SESSIONS.lock().await;
        match reg.get(&stream_id) {
            Some(s) => Arc::clone(s),
            None => return unsafe { response_err("stream_id not found") },
        }
    };

    // Try immediate pop.
    {
        let mut st = session.state.lock().await;
        if let Some(chunk) = st.pending.pop_front() {
            return encode_chunk_response(chunk);
        }
    }

    // Wait with timeout.
    let _timed_out = tokio::time::timeout(
        std::time::Duration::from_millis(timeout_ms),
        session.notify.notified(),
    )
    .await
    .is_err();

    // Try pop again after wakeup — single lock covers both pending and done.
    let mut st = session.state.lock().await;
    if let Some(chunk) = st.pending.pop_front() {
        return encode_chunk_response(chunk);
    }

    // Still nothing.
    unsafe {
        let body = dict_new();
        dict_set_bool(body, "done", st.done);
        response_ok(body)
    }
}

fn encode_chunk_response(chunk: StreamChunk) -> xpc_object_t {
    match chunk {
        StreamChunk::Delta(content) => unsafe {
            let body = dict_new();
            dict_set_str(body, "chunk", &content);
            dict_set_bool(body, "done", false);
            response_ok(body)
        },
        StreamChunk::ToolCalls(tcs) => unsafe {
            let body = dict_new();
            let tc_arr = array_new();
            for tc in &tcs {
                array_append(tc_arr, encode_tool_call(tc));
            }
            dict_set_obj(body, "tool_calls", tc_arr);
            dict_set_bool(body, "done", false);
            response_ok(body)
        },
        StreamChunk::Done {
            usage,
            finish_reason,
        } => unsafe {
            let body = dict_new();
            dict_set_bool(body, "done", true);
            if let Some(fr) = finish_reason {
                dict_set_str(body, "finish_reason", &fr);
            }
            if let Some(u) = usage {
                let uo = dict_new();
                dict_set_i64(uo, "prompt_tokens", u.prompt_tokens as i64);
                dict_set_i64(uo, "completion_tokens", u.completion_tokens as i64);
                dict_set_i64(uo, "total_tokens", u.total_tokens as i64);
                dict_set_obj(body, "usage", uo);
            }
            response_ok(body)
        },
        StreamChunk::Error(msg) => unsafe { response_err(&msg) },
    }
}

// ---------------------------------------------------------------------------
// close_stream
// ---------------------------------------------------------------------------

pub async fn handle_close_stream(req: xpc_object_t) -> xpc_object_t {
    let stream_id = unsafe { dict_get_str(req, "stream_id").unwrap_or_default() };
    if stream_id.is_empty() {
        return unsafe { response_err("stream_id is required") };
    }
    if let Some(s) = SESSIONS.lock().await.remove(&stream_id) {
        // Signal the task to stop gracefully.
        s.state.lock().await.done = true;
        s.notify.notify_one();
        // Wait up to 5 seconds for the task to finish.
        if let Some(h) = s.task.lock().await.take() {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(5), h).await;
        }
    }
    unsafe { response_ok(dict_new()) }
}

// ---------------------------------------------------------------------------
// embed
// ---------------------------------------------------------------------------

pub async fn handle_embed(req: xpc_object_t) -> xpc_object_t {
    let (model_id, encoding_format) = unsafe {
        (
            dict_get_str(req, "model_id").unwrap_or_default(),
            dict_get_str(req, "encoding_format"),
        )
    };

    if model_id.is_empty() {
        return unsafe { response_err("model_id is required") };
    }

    let inputs = unsafe {
        let mut v = Vec::new();
        if let Some(arr) = dict_get_obj(req, "inputs") {
            let count = array_len(arr);
            for i in 0..count {
                if let Some(elem) = array_get(arr, i) {
                    let ptr = xpc_connection_sys::xpc_string_get_string_ptr(elem);
                    if !ptr.is_null() {
                        v.push(CStr::from_ptr(ptr).to_string_lossy().into_owned());
                    }
                }
            }
        }
        v
    };

    if inputs.is_empty() {
        return unsafe { response_err("inputs must be a non-empty array of strings") };
    }

    let (provider_id, real_model) = match route::resolve_alias(&model_id).await {
        Ok(pair) => pair,
        Err(e) => return unsafe { response_err(&e.to_string()) },
    };

    let provider = match pconfig::load_provider(&provider_id).await {
        Ok(p) => p,
        Err(e) => return unsafe { response_err(&e.to_string()) },
    };

    let embed_req = EmbeddingRequest {
        model: real_model,
        input: inputs,
        encoding_format,
    };

    match provider.embed_dyn("ipc", &embed_req).await {
        Ok(resp) => unsafe {
            let body = dict_new();
            let embs = array_new();
            for e in &resp.data {
                let eobj = dict_new();
                let vec_arr = array_new();
                dict_set_i64(eobj, "index", e.index as i64);
                for &v in &e.embedding {
                    // xpc_double_create_fn is re-exported from codec (extern "C")
                    array_append(vec_arr, xpc_double_create(v));
                }
                dict_set_obj(eobj, "embedding", vec_arr);
                array_append(embs, eobj);
            }
            dict_set_obj(body, "embeddings", embs);
            if let Some(u) = resp.usage {
                let uo = dict_new();
                dict_set_i64(uo, "prompt_tokens", u.prompt_tokens as i64);
                dict_set_i64(uo, "completion_tokens", u.completion_tokens as i64);
                dict_set_i64(uo, "total_tokens", u.total_tokens as i64);
                dict_set_obj(body, "usage", uo);
            }
            response_ok(body)
        },
        Err(e) => unsafe { response_err(&e.to_string()) },
    }
}

// ---------------------------------------------------------------------------
// Decode helpers
// ---------------------------------------------------------------------------

unsafe fn decode_messages(req: xpc_object_t) -> Vec<ChatMessage> {
    unsafe {
        let mut out = Vec::new();
        let Some(arr) = dict_get_obj(req, "messages") else {
            return out;
        };
        let count = array_len(arr);
        for i in 0..count {
            let Some(entry) = array_get(arr, i) else {
                continue;
            };
            let role = dict_get_str(entry, "role").unwrap_or_else(|| "user".to_string());
            let content = dict_get_str(entry, "content").unwrap_or_default();
            let tcid = dict_get_str(entry, "tool_call_id");
            let name = dict_get_str(entry, "name");

            let tool_calls = if let Some(tc_arr) = dict_get_obj(entry, "tool_calls") {
                let n = array_len(tc_arr);
                let mut tcs = Vec::new();
                for j in 0..n {
                    let Some(tco) = array_get(tc_arr, j) else {
                        continue;
                    };
                    let tc_id = dict_get_str(tco, "id").unwrap_or_default();
                    let fn_name = dict_get_str(tco, "name").unwrap_or_default();
                    let fn_args = dict_get_str(tco, "arguments").unwrap_or_default();
                    tcs.push(ToolCall {
                        id: tc_id,
                        call_type: "function".to_string(),
                        function: ToolCallFunction {
                            name: fn_name,
                            arguments: fn_args,
                        },
                    });
                }
                if tcs.is_empty() { None } else { Some(tcs) }
            } else {
                None
            };

            out.push(ChatMessage {
                role,
                content,
                tool_calls,
                tool_call_id: tcid,
                name,
            });
        }
        out
    }
}

unsafe fn decode_messages_or_single(req: xpc_object_t) -> Vec<ChatMessage> {
    unsafe {
        let msgs = decode_messages(req);
        if !msgs.is_empty() {
            return msgs;
        }
        let mut out = Vec::new();
        if let Some(msg_obj) = dict_get_obj(req, "message") {
            let role = dict_get_str(msg_obj, "role").unwrap_or_else(|| "user".to_string());
            let content = dict_get_str(msg_obj, "content").unwrap_or_default();
            let tcid = dict_get_str(msg_obj, "tool_call_id");
            let name = dict_get_str(msg_obj, "name");
            out.push(ChatMessage {
                role,
                content,
                tool_calls: None,
                tool_call_id: tcid,
                name,
            });
        } else if let Some(content) = dict_get_str(req, "message") {
            out.push(ChatMessage {
                role: "user".to_string(),
                content,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
        }
        out
    }
}

unsafe fn decode_tools(req: xpc_object_t) -> Vec<Tool> {
    unsafe {
        let mut out = Vec::new();
        let Some(arr) = dict_get_obj(req, "tools") else {
            return out;
        };
        let count = array_len(arr);
        for i in 0..count {
            let Some(entry) = array_get(arr, i) else {
                continue;
            };
            let name = dict_get_str(entry, "name").unwrap_or_default();
            let description = dict_get_str(entry, "description");
            let params =
                dict_get_str(entry, "parameters").and_then(|j| serde_json::from_str(&j).ok());
            out.push(Tool {
                tool_type: "function".to_string(),
                function: ToolFunction {
                    name,
                    description,
                    parameters: params,
                },
            });
        }
        out
    }
}

unsafe fn encode_tool_call(tc: &ToolCall) -> xpc_object_t {
    unsafe {
        let obj = dict_new();
        dict_set_str(obj, "id", &tc.id);
        dict_set_str(obj, "type", &tc.call_type);
        dict_set_str(obj, "name", &tc.function.name);
        dict_set_str(obj, "arguments", &tc.function.arguments);
        obj
    }
}
