//! XPC ↔ Rust encoding/decoding helpers.
//!
//! All `unsafe` code in the IPC layer is confined to this module.
//! Higher-level handlers in `control` and `capability` use only the
//! safe wrappers defined here.

use std::ffi::{CStr, CString};
use std::os::raw::c_void;
use std::ptr;

use xpc_connection_sys::{
    xpc_array_append_value, xpc_array_create, xpc_array_get_count, xpc_array_get_value,
    xpc_bool_create, xpc_bool_get_value, xpc_dictionary_create, xpc_dictionary_get_string,
    xpc_dictionary_get_value, xpc_dictionary_set_value, xpc_int64_create, xpc_int64_get_value,
    xpc_object_t, xpc_release, xpc_string_create,
};

// ---------------------------------------------------------------------------
// Extra XPC bindings not exported by xpc-connection-sys
// ---------------------------------------------------------------------------

unsafe extern "C" {
    pub fn xpc_double_create(value: f64) -> xpc_object_t;
    pub fn xpc_double_get_value(object: xpc_object_t) -> f64;
}

// ---------------------------------------------------------------------------
// CString helpers (private)
// ---------------------------------------------------------------------------

pub(super) unsafe fn cstr(s: &str) -> CString {
    CString::new(s).unwrap_or_else(|e| {
        tracing::error!(
            "String contains interior null byte at position {}",
            e.nul_position()
        );
        CString::default()
    })
}

// ---------------------------------------------------------------------------
// Dictionary write helpers
// ---------------------------------------------------------------------------

pub unsafe fn dict_new() -> xpc_object_t {
    unsafe { xpc_dictionary_create(ptr::null(), ptr::null_mut() as *mut *mut c_void, 0) }
}

pub unsafe fn array_new() -> xpc_object_t {
    unsafe { xpc_array_create(ptr::null_mut() as *mut *mut _, 0) }
}

pub unsafe fn dict_set_str(dict: xpc_object_t, key: &str, val: &str) {
    unsafe {
        let k = cstr(key);
        let v = cstr(val);
        let xv = xpc_string_create(v.as_ptr());
        xpc_dictionary_set_value(dict, k.as_ptr(), xv);
        xpc_release(xv);
    }
}

pub unsafe fn dict_set_bool(dict: xpc_object_t, key: &str, val: bool) {
    unsafe {
        let k = cstr(key);
        let xv = xpc_bool_create(val);
        xpc_dictionary_set_value(dict, k.as_ptr(), xv);
        xpc_release(xv);
    }
}

pub unsafe fn dict_set_i64(dict: xpc_object_t, key: &str, val: i64) {
    unsafe {
        let k = cstr(key);
        let xv = xpc_int64_create(val);
        xpc_dictionary_set_value(dict, k.as_ptr(), xv);
        xpc_release(xv);
    }
}

pub unsafe fn dict_set_f64(dict: xpc_object_t, key: &str, val: f64) {
    unsafe {
        let k = cstr(key);
        let xv = xpc_double_create(val);
        xpc_dictionary_set_value(dict, k.as_ptr(), xv);
        xpc_release(xv);
    }
}

/// Transfer ownership of `val` into `dict[key]`. `val` is released after insertion.
///
/// # Safety
/// - `dict` must be a valid XPC dictionary.
/// - `val` must be a valid XPC object.
/// - Caller must NOT release `val` after this call (ownership is transferred).
pub unsafe fn dict_set_obj(dict: xpc_object_t, key: &str, val: xpc_object_t) {
    unsafe {
        let k = cstr(key);
        xpc_dictionary_set_value(dict, k.as_ptr(), val);
        xpc_release(val);
    }
}

/// Append `val` to `arr`. `val` is released after appending.
///
/// # Safety
/// - `arr` must be a valid XPC array.
/// - `val` must be a valid XPC object.
/// - Caller must NOT release `val` after this call (ownership is transferred).
pub unsafe fn array_append(arr: xpc_object_t, val: xpc_object_t) {
    unsafe {
        xpc_array_append_value(arr, val);
        xpc_release(val);
    }
}

// ---------------------------------------------------------------------------
// Dictionary read helpers
// ---------------------------------------------------------------------------

pub unsafe fn dict_get_str(dict: xpc_object_t, key: &str) -> Option<String> {
    unsafe {
        let k = cstr(key);
        let ptr = xpc_dictionary_get_string(dict, k.as_ptr());
        if ptr.is_null() {
            None
        } else {
            Some(CStr::from_ptr(ptr).to_string_lossy().into_owned())
        }
    }
}

pub unsafe fn dict_get_obj(dict: xpc_object_t, key: &str) -> Option<xpc_object_t> {
    unsafe {
        let k = cstr(key);
        let v = xpc_dictionary_get_value(dict, k.as_ptr());
        if v.is_null() { None } else { Some(v) }
    }
}

pub unsafe fn dict_get_bool(dict: xpc_object_t, key: &str) -> Option<bool> {
    unsafe { dict_get_obj(dict, key).map(|v| xpc_bool_get_value(v)) }
}

pub unsafe fn dict_get_i64(dict: xpc_object_t, key: &str) -> Option<i64> {
    unsafe { dict_get_obj(dict, key).map(|v| xpc_int64_get_value(v)) }
}

pub unsafe fn dict_get_f64(dict: xpc_object_t, key: &str) -> Option<f64> {
    unsafe { dict_get_obj(dict, key).map(|v| xpc_double_get_value(v)) }
}

pub unsafe fn array_len(arr: xpc_object_t) -> usize {
    unsafe { xpc_array_get_count(arr) as usize }
}

pub unsafe fn array_get(arr: xpc_object_t, idx: usize) -> Option<xpc_object_t> {
    unsafe {
        let v = xpc_array_get_value(arr, idx as u64);
        if v.is_null() { None } else { Some(v) }
    }
}

// ---------------------------------------------------------------------------
// Structured result builder
// ---------------------------------------------------------------------------

/// Build a `result` sub-dictionary: `{success, message?}`.
pub unsafe fn result_ok() -> xpc_object_t {
    unsafe {
        let r = dict_new();
        dict_set_bool(r, "success", true);
        r
    }
}

pub unsafe fn result_err(msg: &str) -> xpc_object_t {
    unsafe {
        let r = dict_new();
        dict_set_bool(r, "success", false);
        dict_set_str(r, "message", msg);
        r
    }
}

/// Wrap a body dict in the standard `{result: {...}, ...}` response envelope.
///
/// `body` must be a dict_new()-allocated object; ownership is transferred.
pub unsafe fn response_ok(body: xpc_object_t) -> xpc_object_t {
    unsafe {
        let result = result_ok();
        dict_set_obj(body, "result", result);
        body
    }
}

pub unsafe fn response_err(msg: &str) -> xpc_object_t {
    unsafe {
        let body = dict_new();
        let result = result_err(msg);
        dict_set_obj(body, "result", result);
        body
    }
}
