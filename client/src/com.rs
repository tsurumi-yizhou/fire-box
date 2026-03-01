//! Windows COM communication layer.

use std::collections::HashMap;

use windows::Win32::Foundation::E_FAIL;
use windows::Win32::System::Com::{
    CLSCTX_LOCAL_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::core::{BSTR, HSTRING, Interface};

use crate::error::{Error, Result};
use crate::types::*;

// COM interface GUIDs (these should match the service implementation)
// TODO: Replace with actual GUIDs from service
const CLSID_FIREBOX_SERVICE: windows::core::GUID = windows::core::GUID::from_u128(0);

pub struct ComConnection {
    // TODO: Add COM interface pointer
}

impl ComConnection {
    pub fn new() -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .map_err(|e| Error::Ipc(format!("Failed to initialize COM: {}", e)))?;

            // TODO: Create COM instance
            // let service: IFireBoxService = CoCreateInstance(&CLSID_FIREBOX_SERVICE, None, CLSCTX_LOCAL_SERVER)?;

            Err(Error::Other(
                "Windows COM implementation not yet complete".to_string(),
            ))
        }
    }

    pub fn ping(&self) -> Result<()> {
        Err(Error::PlatformNotSupported)
    }

    pub fn add_api_key_provider(
        &self,
        _name: &str,
        _provider_type: &str,
        _api_key: &str,
        _base_url: Option<&str>,
    ) -> Result<()> {
        Err(Error::PlatformNotSupported)
    }

    pub fn add_oauth_provider(
        &self,
        _name: &str,
        _provider_type: &str,
    ) -> Result<OAuthInitResponse> {
        Err(Error::PlatformNotSupported)
    }

    pub fn complete_oauth(&self, _profile_id: &str) -> Result<()> {
        Err(Error::PlatformNotSupported)
    }

    pub fn add_local_provider(&self, _name: &str, _base_url: &str) -> Result<()> {
        Err(Error::PlatformNotSupported)
    }

    pub fn list_providers(&self) -> Result<Vec<ProviderInfo>> {
        Err(Error::PlatformNotSupported)
    }

    pub fn delete_provider(&self, _profile_id: &str) -> Result<()> {
        Err(Error::PlatformNotSupported)
    }

    pub fn get_all_models(&self, _force_refresh: bool) -> Result<Vec<ModelInfo>> {
        Err(Error::PlatformNotSupported)
    }

    pub fn set_model_enabled(&self, _model_id: &str, _enabled: bool) -> Result<()> {
        Err(Error::PlatformNotSupported)
    }

    pub fn complete(&self, _request: &CompletionRequest) -> Result<CompletionResponse> {
        Err(Error::PlatformNotSupported)
    }

    pub fn stream_start(&self, _request: &CompletionRequest) -> Result<String> {
        Err(Error::PlatformNotSupported)
    }

    pub fn stream_poll(&self, _stream_id: &str) -> Result<Vec<StreamChunk>> {
        Err(Error::PlatformNotSupported)
    }

    pub fn stream_cancel(&self, _stream_id: &str) -> Result<()> {
        Err(Error::PlatformNotSupported)
    }

    pub fn embed(&self, _request: &EmbeddingRequest) -> Result<EmbeddingResponse> {
        Err(Error::PlatformNotSupported)
    }

    pub fn get_routing_rules(&self) -> Result<Vec<RoutingRule>> {
        Err(Error::PlatformNotSupported)
    }

    pub fn set_routing_rules(&self, _rules: &[RoutingRule]) -> Result<()> {
        Err(Error::PlatformNotSupported)
    }

    pub fn add_routing_rule(&self, _pattern: &str, _target_provider: &str) -> Result<()> {
        Err(Error::PlatformNotSupported)
    }

    pub fn remove_routing_rule(&self, _pattern: &str) -> Result<()> {
        Err(Error::PlatformNotSupported)
    }

    pub fn get_metrics(&self) -> Result<Vec<ProviderMetrics>> {
        Err(Error::PlatformNotSupported)
    }

    pub fn get_provider_metrics(&self) -> Result<Vec<ProviderMetrics>> {
        Err(Error::PlatformNotSupported)
    }

    pub fn get_allowlist(&self) -> Result<Vec<AllowlistEntry>> {
        Err(Error::PlatformNotSupported)
    }

    pub fn remove_from_allowlist(&self, _app_path: &str) -> Result<()> {
        Err(Error::PlatformNotSupported)
    }
}
