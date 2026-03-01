//! Tests for error types.

use firebox_client::{Error, Result};

#[test]
fn test_error_display() {
    let err = Error::Ipc("connection failed".to_string());
    assert_eq!(
        err.to_string(),
        "IPC communication error: connection failed"
    );

    let err = Error::Service("invalid request".to_string());
    assert_eq!(err.to_string(), "Service returned error: invalid request");

    let err = Error::InvalidResponse("missing field".to_string());
    assert_eq!(err.to_string(), "Invalid response format: missing field");

    let err = Error::Stream("stream closed".to_string());
    assert_eq!(err.to_string(), "Stream error: stream closed");

    let err = Error::Timeout("request timeout".to_string());
    assert_eq!(err.to_string(), "Timeout: request timeout");

    let err = Error::PlatformNotSupported;
    assert_eq!(err.to_string(), "Not supported on this platform");
}

#[test]
fn test_error_from_serde_json() {
    let json_err = serde_json::from_str::<serde_json::Value>("invalid json");
    assert!(json_err.is_err());

    if let Err(e) = json_err {
        let err: Error = e.into();
        match err {
            Error::Serialization(_) => {}
            _ => panic!("Expected Serialization error"),
        }
    }
}

#[test]
fn test_result_type() {
    fn returns_ok() -> Result<i32> {
        Ok(42)
    }

    fn returns_err() -> Result<i32> {
        Err(Error::Other("test error".to_string()))
    }

    let ok_result = returns_ok();
    assert!(ok_result.is_ok());
    assert_eq!(ok_result.unwrap(), 42);

    let err_result = returns_err();
    assert!(err_result.is_err());
}

#[test]
fn test_error_variants() {
    let errors = vec![
        Error::Ipc("test".to_string()),
        Error::Service("test".to_string()),
        Error::InvalidResponse("test".to_string()),
        Error::Stream("test".to_string()),
        Error::Timeout("test".to_string()),
        Error::PlatformNotSupported,
        Error::Other("test".to_string()),
    ];

    for err in errors {
        assert!(!err.to_string().is_empty());
    }
}
