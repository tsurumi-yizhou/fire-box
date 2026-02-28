//! Windows COM IPC server — CONTROL + CAPABILITY protocols over COM LocalService.
//!
//! Each COM method takes typed BSTR/u32/f64 parameters and returns results
//! via out-pointers. No JSON serialization — all data flows as native COM types.
//!
//! COM interface:
//!   IID  {3B1A2C4D-5E6F-7A8B-9C0D-EF1234567890}
//!   CLSID {4C2B3D5E-6F7A-8B9C-0D1E-F12345678901}

// COM interface names are PascalCase by convention.
#![allow(non_snake_case)]
#![allow(clippy::too_many_arguments)]

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::{Mutex, Notify};
use tokio::task::JoinHandle;
use windows::{Win32::Foundation::*, Win32::System::Com::*, Win32::System::Threading::*, core::*};

use super::connections::{ConnectionInfo, ConnectionRegistry};
use crate::middleware::{access, config, metadata as meta_mw, metrics, route};
use crate::providers::config as pconfig;
use crate::providers::{
    ChatMessage, CompletionRequest, EmbeddingRequest, StreamEvent, Tool, ToolCall,
};

// ---------------------------------------------------------------------------
// Global tokio runtime (same pattern as XPC GLOBAL_RT)
// ---------------------------------------------------------------------------

pub static GLOBAL_RT: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("FireBox COM global tokio runtime")
});

// ---------------------------------------------------------------------------
// COM GUIDs
// ---------------------------------------------------------------------------

#[allow(dead_code)]
const IID_IFIRE_BOX_SERVICE: GUID = GUID {
    data1: 0x3B1A2C4D,
    data2: 0x5E6F,
    data3: 0x7A8B,
    data4: [0x9C, 0x0D, 0xEF, 0x12, 0x34, 0x56, 0x78, 0x90],
};

const CLSID_FIRE_BOX_SERVICE: GUID = GUID {
    data1: 0x4C2B3D5E,
    data2: 0x6F7A,
    data3: 0x8B9C,
    data4: [0x0D, 0x1E, 0xF1, 0x23, 0x45, 0x67, 0x89, 0x01],
};

// ---------------------------------------------------------------------------
// COM interface — individual typed methods, no JSON
// ---------------------------------------------------------------------------

#[interface("3B1A2C4D-5E6F-7A8B-9C0D-EF1234567890")]
unsafe trait IFireBoxService: IUnknown {
    /// Ping — returns empty BSTR on success.
    fn Ping(&self, out: *mut BSTR) -> HRESULT;

    /// Add an API-key provider. Returns provider_id in `out`.
    fn AddApiKeyProvider(
        &self,
        name: BSTR,
        provider_type: BSTR,
        api_key: BSTR,
        base_url: BSTR,
        out: *mut BSTR,
    ) -> HRESULT;

    /// Start OAuth device flow. Returns "provider_id\nuser_code\nverification_uri\nexpires_in" in `out`.
    fn AddOAuthProvider(&self, name: BSTR, provider_type: BSTR, out: *mut BSTR) -> HRESULT;

    /// Complete pending OAuth. Returns empty BSTR on success.
    fn CompleteOAuth(&self, provider_id: BSTR, out: *mut BSTR) -> HRESULT;

    /// Add a local llama.cpp provider. Returns provider_id in `out`.
    fn AddLocalProvider(
        &self,
        name: BSTR,
        model_path: BSTR,
        _context_size: u32,
        _gpu_layers: i32,
        out: *mut BSTR,
    ) -> HRESULT;

    /// List provider IDs. Returns newline-separated IDs in `out`.
    fn ListProviders(&self, out: *mut BSTR) -> HRESULT;

    /// Delete a provider. Returns empty BSTR on success.
    fn DeleteProvider(&self, provider_id: BSTR, out: *mut BSTR) -> HRESULT;

    /// Get all models for a provider. Returns "model_id\tenabled\n..." in `out`.
    fn GetAllModels(&self, provider_id: BSTR, out: *mut BSTR) -> HRESULT;

    /// Set model enabled/disabled. Returns empty BSTR on success.
    fn SetModelEnabled(
        &self,
        provider_id: BSTR,
        model_id: BSTR,
        enabled: BOOL,
        out: *mut BSTR,
    ) -> HRESULT;

    /// Set route rules. `targets` is "provider_id\tmodel_id\n..." format.
    fn SetRouteRules(
        &self,
        virtual_model_id: BSTR,
        display_name: BSTR,
        strategy: BSTR,
        targets: BSTR,
        cap_chat: BOOL,
        cap_streaming: BOOL,
        cap_embeddings: BOOL,
        cap_vision: BOOL,
        cap_tool_calling: BOOL,
        meta_context_window: u32,
        meta_pricing_tier: BSTR,
        meta_strengths: BSTR,
        meta_description: BSTR,
        out: *mut BSTR,
    ) -> HRESULT;

    /// Get route rules. If virtual_model_id is empty, returns all.
    /// Format: "vmid\tdisplay\tstrategy\tpid:mid,pid:mid\n..." in `out`.
    fn GetRouteRules(&self, virtual_model_id: BSTR, out: *mut BSTR) -> HRESULT;

    /// Delete a route. Returns empty BSTR on success.
    fn DeleteRoute(&self, virtual_model_id: BSTR, out: *mut BSTR) -> HRESULT;

    /// Get metrics snapshot. Returns tab-separated values in `out`.
    fn GetMetricsSnapshot(&self, out: *mut BSTR) -> HRESULT;

    /// Get metrics range. Returns one line per bucket in `out`.
    fn GetMetricsRange(&self, start_ms: u64, end_ms: u64, out: *mut BSTR) -> HRESULT;

    /// List connections. Returns "id\tname\tpath\treqs\tconnected_at\n..." in `out`.
    fn ListConnections(&self, out: *mut BSTR) -> HRESULT;

    /// Get allowlist. Returns "app_path\tdisplay_name\n..." in `out`.
    fn GetAllowlist(&self, out: *mut BSTR) -> HRESULT;

    /// Remove from allowlist. Returns empty BSTR on success.
    fn RemoveFromAllowlist(&self, app_path: BSTR, out: *mut BSTR) -> HRESULT;

    /// List available (enabled) models. Returns "model_id\tprovider_id\towner\tctx_window\n..." in `out`.
    fn ListAvailableModels(&self, out: *mut BSTR) -> HRESULT;

    /// Get model metadata. Returns "id\tname\tctx_window\ttool_calling\tvision" in `out`.
    fn GetModelMetadata(&self, model_id: BSTR, out: *mut BSTR) -> HRESULT;

    /// Non-streaming completion. `messages` is "role\tcontent\n..." format.
    /// Returns "role\tcontent\tfinish_reason\tprompt_tok\tcompl_tok\ttotal_tok" in `out`.
    fn Complete(
        &self,
        model_id: BSTR,
        messages: BSTR,
        max_tokens: u32,
        temperature: f64,
        out: *mut BSTR,
    ) -> HRESULT;

    /// Create a streaming session. Returns stream_id in `out`.
    fn CreateStream(
        &self,
        model_id: BSTR,
        temperature: f64,
        max_tokens: u32,
        out: *mut BSTR,
    ) -> HRESULT;

    /// Send a message to a stream. Returns empty BSTR on success.
    fn SendMessage(&self, stream_id: BSTR, role: BSTR, content: BSTR, out: *mut BSTR) -> HRESULT;

    /// Receive stream chunks. Returns "type\tdata\n..." in `out`.
    fn ReceiveStream(&self, stream_id: BSTR, timeout_ms: u32, out: *mut BSTR) -> HRESULT;

    /// Close a stream. Returns empty BSTR on success.
    fn CloseStream(&self, stream_id: BSTR, out: *mut BSTR) -> HRESULT;

    /// Embed text. Returns "pt\ttt\nemb1_dim1,dim2,...\nemb2_dim1,dim2,..." in `out`.
    fn Embed(&self, model_id: BSTR, inputs: BSTR, out: *mut BSTR) -> HRESULT;
}

// ---------------------------------------------------------------------------
// Pending OAuth state
// ---------------------------------------------------------------------------

enum PendingOAuth {
    Copilot {
        device_code: String,
        interval: u64,
        expires_in: u64,
    },
    DashScope {
        flow: crate::providers::dashscope::QwenOAuthFlow,
    },
}

static PENDING_OAUTH: LazyLock<Mutex<HashMap<String, PendingOAuth>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

// ---------------------------------------------------------------------------
// Streaming session state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum StreamChunk {
    Delta(String),
    ToolCalls(Vec<ToolCall>),
    Done { finish_reason: Option<String> },
    Error(String),
}

struct StreamSession {
    provider_id: String,
    model_id: String,
    temperature: Option<f64>,
    max_tokens: Option<u32>,
    messages: Mutex<Vec<ChatMessage>>,
    tools: Mutex<Vec<Tool>>,
    pending: Mutex<VecDeque<StreamChunk>>,
    notify: Notify,
    done: AtomicBool,
    task: Mutex<Option<JoinHandle<()>>>,
}

impl StreamSession {
    fn new(
        provider_id: String,
        model_id: String,
        temperature: Option<f64>,
        max_tokens: Option<u32>,
    ) -> Arc<Self> {
        Arc::new(Self {
            provider_id,
            model_id,
            temperature,
            max_tokens,
            messages: Mutex::new(Vec::new()),
            tools: Mutex::new(Vec::new()),
            pending: Mutex::new(VecDeque::new()),
            notify: Notify::new(),
            done: AtomicBool::new(false),
            task: Mutex::new(None),
        })
    }
}

static SESSIONS: LazyLock<Mutex<HashMap<String, Arc<StreamSession>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn new_stream_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let addr = &nanos as *const u32 as usize;
    format!("s-{addr:x}-{nanos:x}")
}

fn encode_chunk(chunk: &StreamChunk) -> String {
    match chunk {
        StreamChunk::Delta(s) => format!("delta\t{s}"),
        StreamChunk::ToolCalls(tcs) => {
            let data: Vec<String> = tcs
                .iter()
                .map(|tc| format!("{}|{}|{}", tc.id, tc.function.name, tc.function.arguments))
                .collect();
            format!("tool_calls\t{}", data.join(";"))
        }
        StreamChunk::Done { finish_reason, .. } => {
            format!("done\t{}", finish_reason.as_deref().unwrap_or_default())
        }
        StreamChunk::Error(msg) => format!("error\t{msg}"),
    }
}

// ---------------------------------------------------------------------------
// BSTR helpers
// ---------------------------------------------------------------------------

fn bstr_to_string(b: &BSTR) -> String {
    b.to_string()
}

fn string_to_bstr(s: &str) -> BSTR {
    BSTR::from(s)
}

fn write_bstr(out: *mut BSTR, s: &str) {
    unsafe { *out = string_to_bstr(s) };
}

fn opt_str(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

// ---------------------------------------------------------------------------
// TOFU access check
// ---------------------------------------------------------------------------

static CONNECTION_COUNTER: AtomicU64 = AtomicU64::new(0);

fn new_connection_id() -> String {
    let n = CONNECTION_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("conn-{n}")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Resolve the calling process PID to (exe_path, display_name).
fn resolve_caller() -> (String, String) {
    unsafe {
        // Get the caller's thread ID, then resolve to PID via the thread handle.
        let tid = match CoGetCallerTID() {
            Ok(tid) if tid != 0 => tid,
            _ => return ("unknown".into(), "unknown".into()),
        };
        let thread_handle = OpenThread(THREAD_QUERY_LIMITED_INFORMATION, false, tid);
        let Ok(thread_handle) = thread_handle else {
            return (format!("tid:{tid}"), format!("tid:{tid}"));
        };
        let pid = GetProcessIdOfThread(thread_handle);
        let _ = windows::Win32::Foundation::CloseHandle(thread_handle);
        if pid == 0 {
            return ("unknown".into(), "unknown".into());
        }
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid);
        let Ok(handle) = handle else {
            return (format!("pid:{pid}"), format!("pid:{pid}"));
        };
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        if QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
        .is_ok()
        {
            let _ = windows::Win32::Foundation::CloseHandle(handle);
            let path = String::from_utf16_lossy(&buf[..len as usize]);
            let name = path.rsplit('\\').next().unwrap_or(&path).to_string();
            (path, name)
        } else {
            let _ = windows::Win32::Foundation::CloseHandle(handle);
            (format!("pid:{pid}"), format!("pid:{pid}"))
        }
    }
}

/// Show a TOFU prompt via Helper.exe on Windows.
async fn show_tofu_prompt(app_path: &str, display_name: &str) -> bool {
    let timeout = crate::providers::consts::TOFU_PROMPT_TIMEOUT;
    let helper = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("FireBoxHelper.exe")))
        .unwrap_or_else(|| std::path::PathBuf::from("FireBoxHelper.exe"));

    let result = tokio::time::timeout(
        timeout,
        tokio::process::Command::new(&helper)
            .arg("--tofu-prompt")
            .arg(display_name)
            .arg(app_path)
            .output(),
    )
    .await;
    match result {
        Ok(Ok(o)) => o.status.success(),
        Ok(Err(_)) => true,
        Err(_) => {
            tracing::warn!(
                "TOFU prompt timed out after {}s — denying",
                timeout.as_secs()
            );
            false
        }
    }
}

// ---------------------------------------------------------------------------
// COM implementation struct
// ---------------------------------------------------------------------------

#[implement(IFireBoxService)]
struct FireBoxServiceImpl {
    registry: ConnectionRegistry,
    conn_id: String,
}

impl IFireBoxService_Impl for FireBoxServiceImpl_Impl {
    unsafe fn Ping(&self, out: *mut BSTR) -> HRESULT {
        self.registry.increment(&self.conn_id);
        write_bstr(out, "");
        S_OK
    }

    unsafe fn AddApiKeyProvider(
        &self,
        name: BSTR,
        provider_type: BSTR,
        api_key: BSTR,
        base_url: BSTR,
        out: *mut BSTR,
    ) -> HRESULT {
        self.registry.increment(&self.conn_id);
        let name = bstr_to_string(&name);
        let ptype = bstr_to_string(&provider_type);
        let api_key = bstr_to_string(&api_key);
        let base_url_s = bstr_to_string(&base_url);

        if name.is_empty() || ptype.is_empty() {
            write_bstr(out, "");
            return E_INVALIDARG;
        }

        let cfg = match ptype.as_str() {
            "openai" => pconfig::ProviderConfig::openai(&api_key, opt_str(&base_url_s)),
            "anthropic" => pconfig::ProviderConfig::anthropic(&api_key, opt_str(&base_url_s)),
            "ollama" => pconfig::ProviderConfig::ollama(opt_str(&base_url_s)),
            "vllm" => pconfig::ProviderConfig::vllm(opt_str(&api_key), opt_str(&base_url_s)),
            _ => {
                write_bstr(out, "");
                return E_INVALIDARG;
            }
        };

        let profile_id = name.to_lowercase().replace(' ', "_");

        let result = GLOBAL_RT.block_on(async {
            pconfig::configure_provider(&profile_id, &cfg).await?;
            if let Err(e) = pconfig::add_to_provider_index(&profile_id).await {
                let _ = pconfig::remove_provider(&profile_id).await;
                return Err(anyhow::anyhow!("{e}"));
            }
            let pid = profile_id.clone();
            let nm = name.clone();
            if let Err(e) = config::update_config(move |d| {
                d.display_names.insert(pid, nm);
            })
            .await
            {
                let _ = pconfig::remove_from_provider_index(&profile_id).await;
                let _ = pconfig::remove_provider(&profile_id).await;
                return Err(anyhow::anyhow!("{e}"));
            }
            Ok(profile_id)
        });

        match result {
            Ok(pid) => {
                write_bstr(out, &pid);
                S_OK
            }
            Err(e) => {
                write_bstr(out, &e.to_string());
                E_FAIL
            }
        }
    }

    unsafe fn AddOAuthProvider(&self, name: BSTR, provider_type: BSTR, out: *mut BSTR) -> HRESULT {
        self.registry.increment(&self.conn_id);
        let name = bstr_to_string(&name);
        let ptype = bstr_to_string(&provider_type);

        if name.is_empty() || ptype.is_empty() {
            write_bstr(out, "");
            return E_INVALIDARG;
        }

        let profile_id = name.to_lowercase().replace(' ', "_");

        let result = GLOBAL_RT.block_on(async {
            match ptype.as_str() {
                "copilot" => {
                    use crate::providers::copilot::CopilotProvider;
                    let dc = CopilotProvider::start_device_flow(None).await?;
                    let cfg = pconfig::ProviderConfig::copilot_pending(None);
                    pconfig::configure_provider(&profile_id, &cfg).await?;
                    if let Err(e) = pconfig::add_to_provider_index(&profile_id).await {
                        let _ = pconfig::remove_provider(&profile_id).await;
                        return Err(anyhow::anyhow!("{e}"));
                    }
                    let pid = profile_id.clone();
                    let nm = name.clone();
                    if let Err(e) = config::update_config(move |d| {
                        d.display_names.insert(pid, nm);
                    })
                    .await
                    {
                        let _ = pconfig::remove_from_provider_index(&profile_id).await;
                        let _ = pconfig::remove_provider(&profile_id).await;
                        return Err(anyhow::anyhow!("{e}"));
                    }
                    let user_code = dc.user_code.clone();
                    let uri = dc.verification_uri.clone();
                    let expires = dc.expires_in;
                    PENDING_OAUTH.lock().await.insert(
                        profile_id.clone(),
                        PendingOAuth::Copilot {
                            device_code: dc.device_code,
                            interval: dc.interval,
                            expires_in: dc.expires_in,
                        },
                    );
                    Ok(format!("{profile_id}\n{user_code}\n{uri}\n{expires}"))
                }
                "dashscope" => {
                    use crate::providers::dashscope::QwenOAuthFlow;
                    let flow = QwenOAuthFlow::start(None).await?;
                    let cfg = pconfig::ProviderConfig::DashScope(pconfig::DashScopeConfig {
                        access_token: None,
                        refresh_token: None,
                        resource_url: None,
                        expiry_date: None,
                        base_url: None,
                    });
                    pconfig::configure_provider(&profile_id, &cfg).await?;
                    if let Err(e) = pconfig::add_to_provider_index(&profile_id).await {
                        let _ = pconfig::remove_provider(&profile_id).await;
                        return Err(anyhow::anyhow!("{e}"));
                    }
                    let pid = profile_id.clone();
                    let nm = name.clone();
                    if let Err(e) = config::update_config(move |d| {
                        d.display_names.insert(pid, nm);
                    })
                    .await
                    {
                        let _ = pconfig::remove_from_provider_index(&profile_id).await;
                        let _ = pconfig::remove_provider(&profile_id).await;
                        return Err(anyhow::anyhow!("{e}"));
                    }
                    let dc = flow.device_code_response();
                    let user_code = dc.user_code.clone();
                    let uri = dc.verification_uri.clone();
                    let expires = dc.expires_in;
                    PENDING_OAUTH
                        .lock()
                        .await
                        .insert(profile_id.clone(), PendingOAuth::DashScope { flow });
                    Ok(format!("{profile_id}\n{user_code}\n{uri}\n{expires}"))
                }
                other => Err(anyhow::anyhow!("unsupported oauth provider_type: {other}")),
            }
        });

        match result {
            Ok(data) => {
                write_bstr(out, &data);
                S_OK
            }
            Err(e) => {
                write_bstr(out, &e.to_string());
                E_FAIL
            }
        }
    }

    unsafe fn CompleteOAuth(&self, provider_id: BSTR, out: *mut BSTR) -> HRESULT {
        self.registry.increment(&self.conn_id);
        let provider_id = bstr_to_string(&provider_id);
        if provider_id.is_empty() {
            write_bstr(out, "provider_id is required");
            return E_INVALIDARG;
        }

        let result = GLOBAL_RT.block_on(async {
            let pending = PENDING_OAUTH.lock().await.remove(&provider_id);
            let Some(pending) = pending else {
                return Err(anyhow::anyhow!("no pending OAuth for {provider_id}"));
            };
            match pending {
                PendingOAuth::Copilot {
                    device_code,
                    interval,
                    expires_in,
                } => {
                    use crate::providers::copilot::CopilotProvider;
                    let token =
                        CopilotProvider::poll_for_token(None, &device_code, interval, expires_in)
                            .await?;
                    let cfg = pconfig::ProviderConfig::copilot(&token, None);
                    pconfig::configure_provider(&provider_id, &cfg).await?;
                    Ok(())
                }
                PendingOAuth::DashScope { flow } => {
                    let creds = flow.wait_for_token().await?;
                    let cfg = pconfig::ProviderConfig::dashscope_oauth(
                        &creds.access_token,
                        creds.refresh_token.as_deref().unwrap_or_default(),
                        creds.expiry_date.unwrap_or(0),
                        None,
                    );
                    pconfig::configure_provider(&provider_id, &cfg).await?;
                    Ok(())
                }
            }
        });

        match result {
            Ok(()) => {
                write_bstr(out, "");
                S_OK
            }
            Err(e) => {
                write_bstr(out, &e.to_string());
                E_FAIL
            }
        }
    }

    unsafe fn AddLocalProvider(
        &self,
        name: BSTR,
        model_path: BSTR,
        _context_size: u32,
        _gpu_layers: i32,
        out: *mut BSTR,
    ) -> HRESULT {
        self.registry.increment(&self.conn_id);
        let name = bstr_to_string(&name);
        let model_path = bstr_to_string(&model_path);

        if name.is_empty() || model_path.is_empty() {
            write_bstr(out, "");
            return E_INVALIDARG;
        }

        let profile_id = name.to_lowercase().replace(' ', "_");
        let cfg = pconfig::ProviderConfig::llamacpp(&model_path);

        let result = GLOBAL_RT.block_on(async {
            pconfig::configure_provider(&profile_id, &cfg).await?;
            if let Err(e) = pconfig::add_to_provider_index(&profile_id).await {
                let _ = pconfig::remove_provider(&profile_id).await;
                return Err(anyhow::anyhow!("{e}"));
            }
            let pid = profile_id.clone();
            let nm = name.clone();
            if let Err(e) = config::update_config(move |d| {
                d.display_names.insert(pid, nm);
            })
            .await
            {
                let _ = pconfig::remove_from_provider_index(&profile_id).await;
                let _ = pconfig::remove_provider(&profile_id).await;
                return Err(anyhow::anyhow!("{e}"));
            }
            Ok(profile_id)
        });

        match result {
            Ok(pid) => {
                write_bstr(out, &pid);
                S_OK
            }
            Err(e) => {
                write_bstr(out, &e.to_string());
                E_FAIL
            }
        }
    }

    unsafe fn ListProviders(&self, out: *mut BSTR) -> HRESULT {
        self.registry.increment(&self.conn_id);
        let index = GLOBAL_RT.block_on(pconfig::load_provider_index());
        write_bstr(out, &index.join("\n"));
        S_OK
    }

    unsafe fn DeleteProvider(&self, provider_id: BSTR, out: *mut BSTR) -> HRESULT {
        self.registry.increment(&self.conn_id);
        let pid = bstr_to_string(&provider_id);
        if pid.is_empty() {
            write_bstr(out, "provider_id is required");
            return E_INVALIDARG;
        }
        let result = GLOBAL_RT.block_on(async {
            pconfig::remove_from_provider_index(&pid).await?;
            pconfig::remove_provider(&pid).await?;
            let p = pid.clone();
            config::update_config(move |d| {
                d.display_names.remove(&p);
            })
            .await?;
            Ok::<(), anyhow::Error>(())
        });
        match result {
            Ok(()) => {
                write_bstr(out, "");
                S_OK
            }
            Err(e) => {
                write_bstr(out, &e.to_string());
                E_FAIL
            }
        }
    }

    unsafe fn GetAllModels(&self, provider_id: BSTR, out: *mut BSTR) -> HRESULT {
        self.registry.increment(&self.conn_id);
        let pid = bstr_to_string(&provider_id);
        if pid.is_empty() {
            write_bstr(out, "provider_id is required");
            return E_INVALIDARG;
        }
        let result = GLOBAL_RT.block_on(async {
            let provider = pconfig::load_provider(&pid).await?;
            let models = provider.list_models_dyn().await?;
            let mut lines = Vec::new();
            for m in &models {
                let enabled = route::is_model_enabled(&pid, &m.id).await;
                lines.push(format!("{}\t{}", m.id, enabled));
            }
            Ok::<String, anyhow::Error>(lines.join("\n"))
        });
        match result {
            Ok(s) => {
                write_bstr(out, &s);
                S_OK
            }
            Err(e) => {
                write_bstr(out, &e.to_string());
                E_FAIL
            }
        }
    }

    unsafe fn SetModelEnabled(
        &self,
        provider_id: BSTR,
        model_id: BSTR,
        enabled: BOOL,
        out: *mut BSTR,
    ) -> HRESULT {
        self.registry.increment(&self.conn_id);
        let pid = bstr_to_string(&provider_id);
        let mid = bstr_to_string(&model_id);
        if pid.is_empty() || mid.is_empty() {
            write_bstr(out, "provider_id and model_id are required");
            return E_INVALIDARG;
        }
        let result = GLOBAL_RT.block_on(async {
            let provider = pconfig::load_provider(&pid).await?;
            let all: Vec<String> = provider
                .list_models_dyn()
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|m| m.id)
                .collect();
            route::toggle_model(&pid, &mid, enabled.as_bool(), &all).await
        });
        match result {
            Ok(_) => {
                write_bstr(out, "");
                S_OK
            }
            Err(e) => {
                write_bstr(out, &e.to_string());
                E_FAIL
            }
        }
    }

    unsafe fn SetRouteRules(
        &self,
        virtual_model_id: BSTR,
        display_name: BSTR,
        strategy: BSTR,
        targets: BSTR,
        cap_chat: BOOL,
        cap_streaming: BOOL,
        cap_embeddings: BOOL,
        cap_vision: BOOL,
        cap_tool_calling: BOOL,
        meta_context_window: u32,
        meta_pricing_tier: BSTR,
        meta_strengths: BSTR,
        meta_description: BSTR,
        out: *mut BSTR,
    ) -> HRESULT {
        self.registry.increment(&self.conn_id);
        let vmid = bstr_to_string(&virtual_model_id);
        if vmid.is_empty() {
            write_bstr(out, "virtual_model_id is required");
            return E_INVALIDARG;
        }
        let dn_s = bstr_to_string(&display_name);
        let dn = if dn_s.is_empty() { vmid.clone() } else { dn_s };
        let strat_s = bstr_to_string(&strategy);
        let strat = match strat_s.as_str() {
            "random" => route::RouteStrategy::Random,
            _ => route::RouteStrategy::Failover,
        };
        let targets_s = bstr_to_string(&targets);
        let tgts: Vec<route::RouteTarget> = targets_s
            .lines()
            .filter_map(|line| {
                let mut parts = line.splitn(2, '\t');
                let p = parts.next()?.to_string();
                let m = parts.next()?.to_string();
                if p.is_empty() || m.is_empty() {
                    return None;
                }
                Some(route::RouteTarget {
                    provider_id: p,
                    model_id: m,
                })
            })
            .collect();
        let caps = route::RouteCapabilities {
            chat: cap_chat.as_bool(),
            streaming: cap_streaming.as_bool(),
            embeddings: cap_embeddings.as_bool(),
            vision: cap_vision.as_bool(),
            tool_calling: cap_tool_calling.as_bool(),
        };
        let pt = bstr_to_string(&meta_pricing_tier);
        let ms = bstr_to_string(&meta_strengths);
        let md = bstr_to_string(&meta_description);
        let meta = route::RouteMetadata {
            context_window: if meta_context_window == 0 {
                None
            } else {
                Some(meta_context_window)
            },
            pricing_tier: opt_str(&pt),
            strengths: ms
                .lines()
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect(),
            description: opt_str(&md),
        };
        let result = GLOBAL_RT.block_on(route::set_route_rules_with_options(
            &vmid, &dn, caps, meta, tgts, strat,
        ));
        match result {
            Ok(_) => {
                write_bstr(out, "");
                S_OK
            }
            Err(e) => {
                write_bstr(out, &e.to_string());
                E_FAIL
            }
        }
    }

    unsafe fn GetRouteRules(&self, virtual_model_id: BSTR, out: *mut BSTR) -> HRESULT {
        self.registry.increment(&self.conn_id);
        let vmid = bstr_to_string(&virtual_model_id);
        let encode = |r: &route::RouteRule| -> String {
            let strat = match r.strategy {
                route::RouteStrategy::Random => "random",
                route::RouteStrategy::Failover => "failover",
            };
            let tgts: Vec<String> = r
                .targets
                .iter()
                .map(|t| format!("{}:{}", t.provider_id, t.model_id))
                .collect();
            format!(
                "{}\t{}\t{}\t{}",
                r.virtual_model_id,
                r.display_name,
                strat,
                tgts.join(",")
            )
        };
        let result = GLOBAL_RT.block_on(async {
            if !vmid.is_empty() {
                match route::get_route_rules(&vmid).await {
                    Ok(Some(rule)) => Ok(encode(&rule)),
                    Ok(None) => Err(anyhow::anyhow!("route '{}' not found", vmid)),
                    Err(e) => Err(e),
                }
            } else {
                match route::get_all_rules().await {
                    Ok(rules) => Ok(rules.iter().map(encode).collect::<Vec<_>>().join("\n")),
                    Err(e) => Err(e),
                }
            }
        });
        match result {
            Ok(s) => {
                write_bstr(out, &s);
                S_OK
            }
            Err(e) => {
                write_bstr(out, &e.to_string());
                E_FAIL
            }
        }
    }

    unsafe fn DeleteRoute(&self, virtual_model_id: BSTR, out: *mut BSTR) -> HRESULT {
        self.registry.increment(&self.conn_id);
        let vmid = bstr_to_string(&virtual_model_id);
        if vmid.is_empty() {
            write_bstr(out, "virtual_model_id required");
            return E_INVALIDARG;
        }
        match GLOBAL_RT.block_on(route::delete_route_rules(&vmid)) {
            Ok(()) => {
                write_bstr(out, "");
                S_OK
            }
            Err(e) => {
                write_bstr(out, &e.to_string());
                E_FAIL
            }
        }
    }

    unsafe fn GetMetricsSnapshot(&self, out: *mut BSTR) -> HRESULT {
        self.registry.increment(&self.conn_id);
        let s = metrics::get_snapshot();
        write_bstr(
            out,
            &format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                s.window_start_ms,
                s.window_end_ms,
                s.requests_total,
                s.requests_failed,
                s.prompt_tokens_total,
                s.completion_tokens_total,
                s.latency_avg_ms,
                s.cost_total
            ),
        );
        S_OK
    }

    unsafe fn GetMetricsRange(&self, start_ms: u64, end_ms: u64, out: *mut BSTR) -> HRESULT {
        self.registry.increment(&self.conn_id);
        let buckets = GLOBAL_RT.block_on(metrics::get_metrics_range(start_ms, end_ms));
        let lines: Vec<String> = buckets
            .iter()
            .map(|b| {
                format!(
                    "{}\t{}\t{}\t{}\t{}\t{}",
                    b.timestamp_ms,
                    b.requests_total,
                    b.requests_failed,
                    b.prompt_tokens,
                    b.completion_tokens,
                    b.cost_total
                )
            })
            .collect();
        write_bstr(out, &lines.join("\n"));
        S_OK
    }

    unsafe fn ListConnections(&self, out: *mut BSTR) -> HRESULT {
        self.registry.increment(&self.conn_id);
        let conns = self.registry.list();
        let lines: Vec<String> = conns
            .iter()
            .map(|c| {
                format!(
                    "{}\t{}\t{}\t{}\t{}",
                    c.connection_id, c.client_name, c.app_path, c.requests_count, c.connected_at_ms
                )
            })
            .collect();
        write_bstr(out, &lines.join("\n"));
        S_OK
    }

    unsafe fn GetAllowlist(&self, out: *mut BSTR) -> HRESULT {
        self.registry.increment(&self.conn_id);
        let entries = GLOBAL_RT.block_on(access::get_allowlist());
        let lines: Vec<String> = entries
            .iter()
            .map(|e| format!("{}\t{}", e.app_path, e.display_name))
            .collect();
        write_bstr(out, &lines.join("\n"));
        S_OK
    }

    unsafe fn RemoveFromAllowlist(&self, app_path: BSTR, out: *mut BSTR) -> HRESULT {
        self.registry.increment(&self.conn_id);
        let ap = bstr_to_string(&app_path);
        if ap.is_empty() {
            write_bstr(out, "app_path is required");
            return E_INVALIDARG;
        }
        match GLOBAL_RT.block_on(access::remove_from_allowlist(&ap)) {
            Ok(_) => {
                write_bstr(out, "");
                S_OK
            }
            Err(e) => {
                write_bstr(out, &e.to_string());
                E_FAIL
            }
        }
    }

    unsafe fn ListAvailableModels(&self, out: *mut BSTR) -> HRESULT {
        self.registry.increment(&self.conn_id);
        let result = GLOBAL_RT.block_on(async {
            let index = pconfig::load_provider_index().await;
            let mut lines = Vec::new();
            for profile_id in &index {
                let provider = match pconfig::load_provider(profile_id).await {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                let models = provider.list_models_dyn().await.unwrap_or_default();
                for m in models {
                    if route::is_model_enabled(profile_id, &m.id).await {
                        lines.push(format!(
                            "{}\t{}\t{}\t{}",
                            m.id,
                            profile_id,
                            m.owner,
                            m.context_window.unwrap_or_default()
                        ));
                    }
                }
            }
            lines
        });
        write_bstr(out, &result.join("\n"));
        S_OK
    }

    unsafe fn GetModelMetadata(&self, model_id: BSTR, out: *mut BSTR) -> HRESULT {
        self.registry.increment(&self.conn_id);
        let mid = bstr_to_string(&model_id);
        if mid.is_empty() {
            write_bstr(out, "model_id is required");
            return E_INVALIDARG;
        }
        let found = GLOBAL_RT.block_on(async {
            let mut mgr = meta_mw::MetadataManager::new();
            mgr.find_model(&mid).await.unwrap_or(None)
        });
        match found {
            Some(m) => {
                let supports_vision = m
                    .modalities
                    .as_ref()
                    .map(|md| md.input.iter().any(|s| s == "image"))
                    .unwrap_or(false);
                let ctx = m.limit.as_ref().map(|l| l.context).unwrap_or(0);
                write_bstr(
                    out,
                    &format!(
                        "{}\t{}\t{}\t{}\t{}",
                        m.id, m.name, ctx, m.tool_call, supports_vision
                    ),
                );
                S_OK
            }
            None => {
                write_bstr(out, &format!("no metadata for model '{mid}'"));
                E_FAIL
            }
        }
    }

    unsafe fn Complete(
        &self,
        model_id: BSTR,
        messages: BSTR,
        max_tokens: u32,
        temperature: f64,
        out: *mut BSTR,
    ) -> HRESULT {
        self.registry.increment(&self.conn_id);
        let mid = bstr_to_string(&model_id);
        let msgs_s = bstr_to_string(&messages);
        if mid.is_empty() {
            write_bstr(out, "model_id is required");
            return E_INVALIDARG;
        }
        let msgs: Vec<ChatMessage> = msgs_s
            .lines()
            .filter_map(|line| {
                let mut parts = line.splitn(2, '\t');
                let role = parts.next()?.to_string();
                let content = parts.next().unwrap_or_default().to_string();
                Some(ChatMessage {
                    role,
                    content,
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                })
            })
            .collect();
        let result = GLOBAL_RT.block_on(async {
            let (provider_id, real_model) = route::resolve_alias(&mid).await?;
            let provider = pconfig::load_provider(&provider_id).await?;
            let request = CompletionRequest {
                model: real_model,
                messages: msgs,
                max_tokens: if max_tokens == 0 {
                    None
                } else {
                    Some(max_tokens)
                },
                temperature: if temperature == 0.0 {
                    None
                } else {
                    Some(temperature)
                },
                stream: false,
                tools: None,
            };
            provider.complete_dyn("ipc", &request).await
        });
        match result {
            Ok(resp) => {
                let ch = resp.choices.first();
                let role = ch.map(|c| c.message.role.as_str()).unwrap_or_default();
                let content = ch.map(|c| c.message.content.as_str()).unwrap_or_default();
                let finish = ch
                    .and_then(|c| c.finish_reason.as_deref())
                    .unwrap_or_default();
                let (pt, ct, tt) = resp
                    .usage
                    .map(|u| (u.prompt_tokens, u.completion_tokens, u.total_tokens))
                    .unwrap_or((0, 0, 0));
                write_bstr(
                    out,
                    &format!("{role}\t{content}\t{finish}\t{pt}\t{ct}\t{tt}"),
                );
                S_OK
            }
            Err(e) => {
                write_bstr(out, &e.to_string());
                E_FAIL
            }
        }
    }

    unsafe fn CreateStream(
        &self,
        model_id: BSTR,
        temperature: f64,
        max_tokens: u32,
        out: *mut BSTR,
    ) -> HRESULT {
        self.registry.increment(&self.conn_id);
        let mid = bstr_to_string(&model_id);
        if mid.is_empty() {
            write_bstr(out, "model_id is required");
            return E_INVALIDARG;
        }
        let temp = if temperature == 0.0 {
            None
        } else {
            Some(temperature)
        };
        let mt = if max_tokens == 0 {
            None
        } else {
            Some(max_tokens)
        };
        let result = GLOBAL_RT.block_on(async {
            let (provider_id, real_model) = route::resolve_alias(&mid).await?;
            let session = StreamSession::new(provider_id, real_model, temp, mt);
            let stream_id = new_stream_id();
            SESSIONS.lock().await.insert(stream_id.clone(), session);
            Ok::<String, anyhow::Error>(stream_id)
        });
        match result {
            Ok(sid) => {
                write_bstr(out, &sid);
                S_OK
            }
            Err(e) => {
                write_bstr(out, &e.to_string());
                E_FAIL
            }
        }
    }

    unsafe fn SendMessage(
        &self,
        stream_id: BSTR,
        role: BSTR,
        content: BSTR,
        out: *mut BSTR,
    ) -> HRESULT {
        self.registry.increment(&self.conn_id);
        let sid = bstr_to_string(&stream_id);
        let role_s = bstr_to_string(&role);
        let content_s = bstr_to_string(&content);
        if sid.is_empty() {
            write_bstr(out, "stream_id is required");
            return E_INVALIDARG;
        }
        let result = GLOBAL_RT.block_on(async {
            let session = {
                let reg = SESSIONS.lock().await;
                reg.get(&sid)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("stream_id not found"))?
            };
            let msg = ChatMessage {
                role: if role_s.is_empty() {
                    "user".into()
                } else {
                    role_s
                },
                content: content_s,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            };
            session.messages.lock().await.push(msg);
            session.done.store(false, Ordering::SeqCst);

            let provider_id = session.provider_id.clone();
            let real_model = session.model_id.clone();
            let temperature = session.temperature;
            let max_tokens = session.max_tokens;
            let all_msgs = session.messages.lock().await.clone();
            let all_tools = session.tools.lock().await.clone();
            let sess = Arc::clone(&session);

            let task = tokio::spawn(async move {
                let provider = match pconfig::load_provider(&provider_id).await {
                    Ok(p) => p,
                    Err(e) => {
                        sess.pending
                            .lock()
                            .await
                            .push_back(StreamChunk::Error(e.to_string()));
                        sess.notify.notify_one();
                        sess.done.store(true, Ordering::SeqCst);
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
                                    sess.pending
                                        .lock()
                                        .await
                                        .push_back(StreamChunk::Delta(content));
                                    sess.notify.notify_one();
                                }
                                Ok(StreamEvent::ToolCalls { tool_calls }) => {
                                    tool_buf.extend(tool_calls);
                                }
                                Ok(StreamEvent::Done) => {
                                    if !tool_buf.is_empty() {
                                        sess.pending.lock().await.push_back(
                                            StreamChunk::ToolCalls(std::mem::take(&mut tool_buf)),
                                        );
                                        sess.notify.notify_one();
                                    }
                                    sess.pending.lock().await.push_back(StreamChunk::Done {
                                        finish_reason: Some("stop".into()),
                                    });
                                    sess.notify.notify_one();
                                    break;
                                }
                                Ok(StreamEvent::Error { message }) => {
                                    sess.pending
                                        .lock()
                                        .await
                                        .push_back(StreamChunk::Error(message));
                                    sess.notify.notify_one();
                                    break;
                                }
                                Err(e) => {
                                    sess.pending
                                        .lock()
                                        .await
                                        .push_back(StreamChunk::Error(e.to_string()));
                                    sess.notify.notify_one();
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        sess.pending
                            .lock()
                            .await
                            .push_back(StreamChunk::Error(e.to_string()));
                        sess.notify.notify_one();
                    }
                }
                sess.done.store(true, Ordering::SeqCst);
            });
            *session.task.lock().await = Some(task);
            Ok::<(), anyhow::Error>(())
        });
        match result {
            Ok(()) => {
                write_bstr(out, "");
                S_OK
            }
            Err(e) => {
                write_bstr(out, &e.to_string());
                E_FAIL
            }
        }
    }

    unsafe fn ReceiveStream(&self, stream_id: BSTR, timeout_ms: u32, out: *mut BSTR) -> HRESULT {
        self.registry.increment(&self.conn_id);
        let sid = bstr_to_string(&stream_id);
        if sid.is_empty() {
            write_bstr(out, "stream_id is required");
            return E_INVALIDARG;
        }
        let result = GLOBAL_RT.block_on(async {
            let session = {
                let reg = SESSIONS.lock().await;
                reg.get(&sid)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("stream_id not found"))?
            };
            if let Some(chunk) = session.pending.lock().await.pop_front() {
                return Ok::<String, anyhow::Error>(encode_chunk(&chunk));
            }
            let timed_out = tokio::time::timeout(
                std::time::Duration::from_millis(timeout_ms as u64),
                session.notify.notified(),
            )
            .await
            .is_err();
            if timed_out {
                if session.done.load(Ordering::SeqCst) {
                    return Ok("done\t".to_string());
                }
                return Ok("timeout\t".to_string());
            }
            if let Some(chunk) = session.pending.lock().await.pop_front() {
                Ok(encode_chunk(&chunk))
            } else {
                Ok("timeout\t".to_string())
            }
        });
        match result {
            Ok(s) => {
                write_bstr(out, &s);
                S_OK
            }
            Err(e) => {
                write_bstr(out, &e.to_string());
                E_FAIL
            }
        }
    }

    unsafe fn CloseStream(&self, stream_id: BSTR, out: *mut BSTR) -> HRESULT {
        self.registry.increment(&self.conn_id);
        let sid = bstr_to_string(&stream_id);
        if sid.is_empty() {
            write_bstr(out, "stream_id is required");
            return E_INVALIDARG;
        }
        GLOBAL_RT.block_on(async {
            if let Some(session) = SESSIONS.lock().await.remove(&sid) {
                // Signal the task to stop gracefully.
                session.done.store(true, Ordering::SeqCst);
                session.notify.notify_one();
                // Wait up to 5 seconds for the task to finish before aborting.
                if let Some(task) = session.task.lock().await.take()
                    && tokio::time::timeout(std::time::Duration::from_secs(5), task)
                        .await
                        .is_err()
                {
                    tracing::warn!("Stream task for {sid} did not finish in time, aborting");
                }
            }
        });
        write_bstr(out, "");
        S_OK
    }

    unsafe fn Embed(&self, model_id: BSTR, inputs: BSTR, out: *mut BSTR) -> HRESULT {
        self.registry.increment(&self.conn_id);
        let mid = bstr_to_string(&model_id);
        let inputs_s = bstr_to_string(&inputs);
        if mid.is_empty() {
            write_bstr(out, "model_id is required");
            return E_INVALIDARG;
        }
        let input_vec: Vec<String> = inputs_s
            .lines()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        if input_vec.is_empty() {
            write_bstr(out, "inputs must not be empty");
            return E_INVALIDARG;
        }
        let result = GLOBAL_RT.block_on(async {
            let (provider_id, real_model) = route::resolve_alias(&mid).await?;
            let provider = pconfig::load_provider(&provider_id).await?;
            let request = EmbeddingRequest {
                model: real_model,
                input: input_vec,
                encoding_format: None,
            };
            provider.embed_dyn("ipc", &request).await
        });
        match result {
            Ok(resp) => {
                let (pt, tt) = resp
                    .usage
                    .map(|u| (u.prompt_tokens, u.total_tokens))
                    .unwrap_or((0, 0));
                let emb_lines: Vec<String> = resp
                    .data
                    .iter()
                    .map(|e| {
                        e.embedding
                            .iter()
                            .map(|v| v.to_string())
                            .collect::<Vec<_>>()
                            .join(",")
                    })
                    .collect();
                write_bstr(out, &format!("{pt}\t{tt}\t{}", emb_lines.join("\n")));
                S_OK
            }
            Err(e) => {
                write_bstr(out, &e.to_string());
                E_FAIL
            }
        }
    }
}

// ---------------------------------------------------------------------------
// COM class factory
// ---------------------------------------------------------------------------

#[implement(IClassFactory)]
struct FireBoxClassFactory {
    registry: ConnectionRegistry,
}

impl IClassFactory_Impl for FireBoxClassFactory_Impl {
    fn CreateInstance(
        &self,
        _outer: Option<&IUnknown>,
        iid: *const GUID,
        ppv: *mut *mut std::ffi::c_void,
    ) -> windows::core::Result<()> {
        let (app_path, display_name) = resolve_caller();

        let decision = GLOBAL_RT.block_on(access::check_access(&app_path));
        match decision {
            access::AccessDecision::Deny => {
                return Err(windows::core::Error::new(E_ACCESSDENIED, "access denied"));
            }
            access::AccessDecision::Unknown => {
                let granted = GLOBAL_RT.block_on(show_tofu_prompt(&app_path, &display_name));
                if granted {
                    if let Err(e) =
                        GLOBAL_RT.block_on(access::grant_access(&app_path, &display_name))
                    {
                        tracing::warn!("Failed to persist TOFU grant for {app_path}: {e}");
                    }
                } else {
                    if let Err(e) =
                        GLOBAL_RT.block_on(access::deny_access(&app_path, &display_name))
                    {
                        tracing::warn!("Failed to persist TOFU deny for {app_path}: {e}");
                    }
                    return Err(windows::core::Error::new(E_ACCESSDENIED, "access denied"));
                }
            }
            access::AccessDecision::Allow => {}
        }

        let conn_id = new_connection_id();
        self.registry.add(ConnectionInfo {
            connection_id: conn_id.clone(),
            client_name: display_name,
            app_path,
            connected_at_ms: now_ms(),
            requests_count: 0,
        });

        let obj: IFireBoxService = FireBoxServiceImpl {
            registry: self.registry.clone(),
            conn_id,
        }
        .into();

        unsafe {
            let unknown: IUnknown = obj.cast()?;
            unknown.query(iid, ppv).ok()
        }
    }

    fn LockServer(&self, _lock: BOOL) -> windows::core::Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// COM server entry point
// ---------------------------------------------------------------------------

pub fn run_listener() {
    unsafe {
        let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
        if hr.is_err() {
            tracing::error!("CoInitializeEx failed: {hr:?}");
            return;
        }

        let registry = ConnectionRegistry::new();
        let factory: IClassFactory = FireBoxClassFactory { registry }.into();

        #[allow(unused_variables)]
        let cookie = match CoRegisterClassObject(
            &CLSID_FIRE_BOX_SERVICE,
            &factory,
            CLSCTX_LOCAL_SERVER,
            REGCLS_MULTIPLEUSE,
        ) {
            Ok(c) => {
                tracing::info!(
                    "FireBox COM server registered (CLSID {{{:?}}})",
                    CLSID_FIRE_BOX_SERVICE
                );
                c
            }
            Err(e) => {
                tracing::error!("CoRegisterClassObject failed: {e}");
                CoUninitialize();
                return;
            }
        };

        loop {
            std::thread::sleep(std::time::Duration::from_secs(
                crate::providers::consts::IPC_LISTENER_SLEEP_SECS,
            ));
        }

        #[allow(unreachable_code)]
        {
            CoRevokeClassObject(cookie).ok();
            CoUninitialize();
        }
    }
}
