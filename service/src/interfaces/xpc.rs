//! macOS XPC IPC server — CONTROL + CAPABILITY protocols over Mach XPC.

use std::ffi::CString;
use std::os::raw::c_void;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use xpc_connection_sys::{
    _xpc_type_connection, _xpc_type_error, XPC_CONNECTION_MACH_SERVICE_LISTENER,
    dispatch_queue_create, xpc_connection_create_mach_service, xpc_connection_resume,
    xpc_connection_send_message, xpc_connection_set_event_handler, xpc_connection_t,
    xpc_dictionary_create_reply, xpc_get_type, xpc_object_t, xpc_release,
};

// xpc_connection_get_pid is a standard XPC function but not exported by
// xpc-connection-sys 0.1.1; declare it directly.
unsafe extern "C" {
    fn xpc_connection_get_pid(connection: xpc_connection_t) -> libc::pid_t;
}

use super::codec::*;
use super::connections::{ConnectionInfo, ConnectionRegistry};
use crate::middleware::access;

/// Global multi-threaded tokio runtime shared by all IPC handlers.
///
/// Used for:
/// - `block_on` in XPC message callbacks (dispatch queue threads)
/// - `.spawn` for background streaming generation tasks
pub static GLOBAL_RT: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("FireBox global tokio runtime")
});

pub fn run_listener() {
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

    /// Dispatch a request to the correct handler, using the global runtime.
    ///
    /// All XPC raw pointer access is confined to this function and the codec.
    /// Returns an owned `xpc_object_t` that the caller must release.
    unsafe fn dispatch(
        event: xpc_object_t,
        conn_id: &str,
        registry: &ConnectionRegistry,
    ) -> xpc_object_t {
        registry.increment(conn_id);
        let cmd = unsafe { dict_get_str(event, "cmd").unwrap_or_default() };

        // Run the async handler on the global runtime, blocking this dispatch thread.
        // `block_on` does NOT require the future to be Send; it runs on the calling thread.
        GLOBAL_RT.block_on(async {
            match cmd.as_str() {
                "ping" => super::control::handle_ping(),

                "add_api_key_provider" => super::control::handle_add_api_key_provider(event).await,
                "add_oauth_provider" => super::control::handle_add_oauth_provider(event).await,
                "complete_oauth" => super::control::handle_complete_oauth(event).await,
                "add_local_provider" => super::control::handle_add_local_provider(event).await,
                "list_providers" => super::control::handle_list_providers().await,
                "delete_provider" => super::control::handle_delete_provider(event).await,

                "get_all_models" => super::control::handle_get_all_models(event).await,
                "set_model_enabled" => super::control::handle_set_model_enabled(event).await,

                "set_route_rules" => super::control::handle_set_route_rules(event).await,
                "get_route_rules" => super::control::handle_get_route_rules(event).await,

                "get_metrics_snapshot" => super::control::handle_get_metrics_snapshot(),
                "get_metrics_range" => super::control::handle_get_metrics_range(event).await,

                "list_connections" => super::control::handle_list_connections(registry),
                "get_allowlist" => super::control::handle_get_allowlist().await,
                "remove_from_allowlist" => {
                    super::control::handle_remove_from_allowlist(event).await
                }

                "list_available_models" => super::capability::handle_list_available_models().await,
                "get_model_metadata" => super::capability::handle_get_model_metadata(event).await,
                "complete" => super::capability::handle_complete(event).await,
                "create_stream" => super::capability::handle_create_stream(event).await,
                "send_message" => super::capability::handle_send_message(event).await,
                "receive_stream" => super::capability::handle_receive_stream(event).await,
                "close_stream" => super::capability::handle_close_stream(event).await,
                "embed" => super::capability::handle_embed(event).await,

                _ => unsafe { response_err(&format!("unknown command: {cmd}")) },
            }
        })
    }

    // Static ASCII literals — CString::new cannot fail on these.
    let service_name = CString::new("com.firebox.service").expect("static CString literal");
    let registry = ConnectionRegistry::new();

    unsafe {
        let queue_label =
            CString::new("com.firebox.service.queue").expect("static CString literal");
        let queue = dispatch_queue_create(queue_label.as_ptr(), std::ptr::null_mut());

        let listener = xpc_connection_create_mach_service(
            service_name.as_ptr(),
            queue,
            u64::from(XPC_CONNECTION_MACH_SERVICE_LISTENER),
        );

        let registry_outer = registry.clone();

        let mut conn_handler = block::ConcreteBlock::new(move |client: xpc_connection_t| {
            let registry_inner = registry_outer.clone();

            // Resolve PID → executable path for TOFU check.
            let pid = xpc_connection_get_pid(client);
            let (app_path, display_name) = access::resolve_pid(pid as i32);

            // TOFU access check.
            let decision = GLOBAL_RT.block_on(access::check_access(&app_path));
            match decision {
                access::AccessDecision::Deny => return,
                access::AccessDecision::Unknown => {
                    // Rate-limit repeated TOFU prompts for the same app.
                    if access::is_tofu_rate_limited(&app_path) {
                        tracing::warn!("TOFU rate limit exceeded for {app_path} — denying");
                        return;
                    }
                    let ap = app_path.clone();
                    let dn = display_name.clone();
                    let granted = GLOBAL_RT.block_on(show_tofu_prompt(&ap, &dn));
                    if granted {
                        if let Err(e) =
                            GLOBAL_RT.block_on(access::grant_access(&app_path, &display_name))
                        {
                            tracing::warn!("Failed to persist TOFU grant for {app_path}: {e}");
                        }
                    } else {
                        access::record_tofu_failure(&app_path);
                        if let Err(e) =
                            GLOBAL_RT.block_on(access::deny_access(&app_path, &display_name))
                        {
                            tracing::warn!("Failed to persist TOFU deny for {app_path}: {e}");
                        }
                        return;
                    }
                }
                access::AccessDecision::Allow => {}
            }

            // Register connection.
            let conn_id = new_connection_id();
            registry_inner.add(ConnectionInfo {
                connection_id: conn_id.clone(),
                client_name: display_name,
                app_path,
                connected_at_ms: now_ms(),
                requests_count: 0,
            });

            let conn_id_msg = conn_id.clone();
            let reg_msg = registry_inner.clone();

            let mut msg_handler = block::ConcreteBlock::new(move |event: xpc_object_t| {
                let event_type = xpc_get_type(event);
                if event_type == &_xpc_type_error as *const _ {
                    reg_msg.remove(&conn_id_msg);
                    return;
                }
                if event_type == &_xpc_type_connection as *const _ {
                    return;
                }

                let reply_body = dispatch(event, &conn_id_msg, &reg_msg);
                let reply = xpc_dictionary_create_reply(event);
                if !reply.is_null() {
                    // "body" is a static ASCII string with no interior NUL bytes.
                    let k = CString::new("body").expect("static CString literal");
                    xpc_connection_sys::xpc_dictionary_set_value(reply, k.as_ptr(), reply_body);
                    xpc_release(reply_body);
                    xpc_connection_send_message(client, reply);
                    xpc_release(reply);
                } else {
                    xpc_release(reply_body);
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

        loop {
            std::thread::sleep(std::time::Duration::from_secs(
                crate::providers::consts::IPC_LISTENER_SLEEP_SECS,
            ));
        }
    }
}

/// Display a TOFU authorization dialog via `osascript`.
/// Returns `true` if the user clicked "Allow".
/// Times out after [`TOFU_PROMPT_TIMEOUT`] (auto-deny).
async fn show_tofu_prompt(app_path: &str, display_name: &str) -> bool {
    let timeout = crate::providers::consts::TOFU_PROMPT_TIMEOUT;
    let script = format!(
        concat!(
            r#"display dialog "Allow \"{}\" to access FireBox?\n\n{}" "#,
            r#"with title "FireBox Access Request" "#,
            r#"buttons {{"Deny", "Allow"}} "#,
            r#"default button "Allow" "#,
            r#"giving up after {} "#,
            r#"with icon caution"#,
        ),
        display_name,
        app_path,
        timeout.as_secs(),
    );
    let result = tokio::time::timeout(
        timeout,
        tokio::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output(),
    )
    .await;
    match result {
        Ok(Ok(o)) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            // If the dialog timed out ("gave up"), osascript returns
            // "gave up:true" — treat as deny.
            stdout.contains("Allow") && !stdout.contains("gave up:true")
        }
        Ok(Err(_)) => true, // osascript not found — allow (fallback)
        Err(_) => {
            tracing::warn!(
                "TOFU prompt timed out after {}s — denying",
                timeout.as_secs()
            );
            false
        }
    }
}
