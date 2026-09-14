//! Bounded native WASM runtime for client-side mini-apps.
//!
//! Only the `links` input/output ABI is available. There is deliberately no
//! WASI, filesystem, socket, clock, randomness, identity, MLS, or key import.
//! Each invocation gets a fresh store, bounded linear memory, and finite fuel.

use std::cmp::min;
use thiserror::Error;
use wasmi::{
    Caller, Config, Engine, Extern, Linker, Module, Store, StoreLimits, StoreLimitsBuilder,
    TrapCode,
};

pub const SANDBOX_ENTRYPOINT: &str = "links_run";
pub const SANDBOX_MEMORY_EXPORT: &str = "memory";
pub const SANDBOX_MAX_MODULE_BYTES: usize = 2 * 1024 * 1024;
pub const SANDBOX_MAX_MEMORY_BYTES: usize = 16 * 1024 * 1024;
pub const SANDBOX_MAX_INPUT_BYTES: usize = 64 * 1024;
pub const SANDBOX_MAX_OUTPUT_BYTES: usize = 256 * 1024;
pub const SANDBOX_MAX_FUEL: u64 = 5_000_000;
pub const SANDBOX_MAX_NETWORK_RULES: usize = 8;
pub const SANDBOX_MAX_CRYPTO_GRANTS: usize = 8;

const INPUT_LENGTH_IMPORT: &str = "input_len";
const INPUT_READ_IMPORT: &str = "input_read";
const OUTPUT_WRITE_IMPORT: &str = "output_write";
const NETWORK_REQUEST_IMPORT: &str = "network_request";
const CRYPTO_OPERATION_IMPORT: &str = "crypto_operation";

const NETWORK_METHODS: [&str; 5] = ["GET", "POST", "PUT", "PATCH", "DELETE"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SandboxCryptoOperation {
    Hash,
    Hmac,
    Sign,
}

impl SandboxCryptoOperation {
    fn from_code(code: i32) -> Option<Self> {
        match code {
            1 => Some(Self::Hash),
            2 => Some(Self::Hmac),
            3 => Some(Self::Sign),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandboxNetworkRule {
    host: String,
    methods: Vec<String>,
    max_request_bytes: usize,
    max_response_bytes: usize,
}

impl SandboxNetworkRule {
    pub fn new(
        host: impl Into<String>,
        methods: impl IntoIterator<Item = impl Into<String>>,
        max_request_bytes: usize,
        max_response_bytes: usize,
    ) -> Result<Self, SandboxError> {
        let host = host.into().to_ascii_lowercase();
        let methods = methods
            .into_iter()
            .map(|method| method.into().to_ascii_uppercase())
            .collect::<Vec<_>>();
        if !valid_network_host(&host)
            || methods.is_empty()
            || methods.len() > NETWORK_METHODS.len()
            || methods
                .iter()
                .any(|method| !NETWORK_METHODS.contains(&method.as_str()))
            || max_request_bytes > SANDBOX_MAX_INPUT_BYTES
            || max_response_bytes > SANDBOX_MAX_OUTPUT_BYTES
        {
            return Err(SandboxError::InvalidPermission);
        }
        Ok(Self {
            host,
            methods,
            max_request_bytes,
            max_response_bytes,
        })
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn methods(&self) -> &[String] {
        &self.methods
    }

    fn allows(&self, method: &str, request_bytes: usize) -> bool {
        self.methods.iter().any(|allowed| allowed == method)
            && request_bytes <= self.max_request_bytes
    }
}

/// Opaque grant for one or more key operations. The token identifies a key
/// inside the trusted host; it is not the key and cannot be used to extract it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandboxCryptoGrant {
    capability: [u8; 32],
    operations: Vec<SandboxCryptoOperation>,
    max_input_bytes: usize,
    max_output_bytes: usize,
}

impl SandboxCryptoGrant {
    pub fn new(
        capability: [u8; 32],
        operations: impl IntoIterator<Item = SandboxCryptoOperation>,
        max_input_bytes: usize,
        max_output_bytes: usize,
    ) -> Result<Self, SandboxError> {
        let operations = operations.into_iter().collect::<Vec<_>>();
        if capability.iter().all(|byte| *byte == 0)
            || operations.is_empty()
            || operations.len() > 3
            || max_input_bytes > SANDBOX_MAX_INPUT_BYTES
            || max_output_bytes > SANDBOX_MAX_OUTPUT_BYTES
        {
            return Err(SandboxError::InvalidPermission);
        }
        Ok(Self {
            capability,
            operations,
            max_input_bytes,
            max_output_bytes,
        })
    }

    pub fn capability(&self) -> [u8; 32] {
        self.capability
    }

    pub fn operations(&self) -> &[SandboxCryptoOperation] {
        &self.operations
    }

    fn allows(&self, operation: SandboxCryptoOperation, input_bytes: usize) -> bool {
        self.operations.contains(&operation) && input_bytes <= self.max_input_bytes
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SandboxPermissions {
    network: Vec<SandboxNetworkRule>,
    crypto: Vec<SandboxCryptoGrant>,
}

impl SandboxPermissions {
    pub fn deny_all() -> Self {
        Self::default()
    }

    pub fn grant_network(&mut self, rule: SandboxNetworkRule) -> Result<(), SandboxError> {
        if self.network.len() >= SANDBOX_MAX_NETWORK_RULES
            || self.network.iter().any(|current| current.host == rule.host)
        {
            return Err(SandboxError::InvalidPermission);
        }
        self.network.push(rule);
        Ok(())
    }

    pub fn grant_crypto(&mut self, grant: SandboxCryptoGrant) -> Result<(), SandboxError> {
        if self.crypto.len() >= SANDBOX_MAX_CRYPTO_GRANTS
            || self
                .crypto
                .iter()
                .any(|current| current.capability == grant.capability)
        {
            return Err(SandboxError::InvalidPermission);
        }
        self.crypto.push(grant);
        Ok(())
    }

    pub fn network_rules(&self) -> &[SandboxNetworkRule] {
        &self.network
    }

    pub fn crypto_grants(&self) -> &[SandboxCryptoGrant] {
        &self.crypto
    }

    fn network_rule(&self, host: &str, method: &str, request_bytes: usize) -> Option<&SandboxNetworkRule> {
        self.network
            .iter()
            .find(|rule| rule.host == host && rule.allows(method, request_bytes))
    }

    fn crypto_grant(
        &self,
        capability: &[u8; 32],
        operation: SandboxCryptoOperation,
        input_bytes: usize,
    ) -> Option<&SandboxCryptoGrant> {
        self.crypto.iter().find(|grant| {
            &grant.capability == capability && grant.allows(operation, input_bytes)
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandboxNetworkRequest {
    pub url: String,
    pub method: String,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandboxNetworkResponse {
    pub body: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandboxCryptoRequest {
    pub capability: [u8; 32],
    pub operation: SandboxCryptoOperation,
    pub input: Vec<u8>,
}

/// Trusted host mediator. Implementations perform the actual network call or
/// key operation. The sandbox gets only validated request data and results.
pub trait SandboxHost: Send {
    fn network_request(
        &mut self,
        request: SandboxNetworkRequest,
    ) -> Result<SandboxNetworkResponse, SandboxError>;
    fn crypto_operation(
        &mut self,
        request: SandboxCryptoRequest,
    ) -> Result<Vec<u8>, SandboxError>;
}

struct DenyAllHost;

impl SandboxHost for DenyAllHost {
    fn network_request(
        &mut self,
        _request: SandboxNetworkRequest,
    ) -> Result<SandboxNetworkResponse, SandboxError> {
        Err(SandboxError::PermissionDenied)
    }

    fn crypto_operation(
        &mut self,
        _request: SandboxCryptoRequest,
    ) -> Result<Vec<u8>, SandboxError> {
        Err(SandboxError::PermissionDenied)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SandboxLimits {
    pub max_module_bytes: usize,
    pub max_memory_bytes: usize,
    pub max_input_bytes: usize,
    pub max_output_bytes: usize,
    pub max_fuel: u64,
}

impl Default for SandboxLimits {
    fn default() -> Self {
        Self {
            max_module_bytes: SANDBOX_MAX_MODULE_BYTES,
            max_memory_bytes: SANDBOX_MAX_MEMORY_BYTES,
            max_input_bytes: SANDBOX_MAX_INPUT_BYTES,
            max_output_bytes: SANDBOX_MAX_OUTPUT_BYTES,
            max_fuel: SANDBOX_MAX_FUEL,
        }
    }
}

impl SandboxLimits {
    fn validate(self) -> Result<Self, SandboxError> {
        if self.max_module_bytes == 0
            || self.max_module_bytes > SANDBOX_MAX_MODULE_BYTES
            || self.max_memory_bytes < 64 * 1024
            || self.max_memory_bytes > SANDBOX_MAX_MEMORY_BYTES
            || self.max_input_bytes > SANDBOX_MAX_INPUT_BYTES
            || self.max_output_bytes > SANDBOX_MAX_OUTPUT_BYTES
            || self.max_fuel == 0
            || self.max_fuel > SANDBOX_MAX_FUEL
        {
            return Err(SandboxError::InvalidLimits);
        }
        Ok(self)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SandboxError {
    #[error("invalid sandbox limits")]
    InvalidLimits,
    #[error("WASM module is too large")]
    ModuleTooLarge,
    #[error("WASM module is invalid")]
    InvalidModule,
    #[error("WASM module imports a forbidden capability")]
    ForbiddenImport,
    #[error("WASM module has no links_run entrypoint")]
    MissingEntrypoint,
    #[error("WASM module has no exported memory")]
    MissingMemory,
    #[error("sandbox input is too large")]
    InputTooLarge,
    #[error("sandbox output is too large")]
    OutputTooLarge,
    #[error("sandbox host ABI violation")]
    HostViolation,
    #[error("invalid mini-app permission")]
    InvalidPermission,
    #[error("mini-app capability denied")]
    PermissionDenied,
    #[error("invalid mini-app network request")]
    InvalidNetworkRequest,
    #[error("invalid mini-app cryptographic request")]
    InvalidCryptoRequest,
    #[error("mini-app network response is too large")]
    NetworkResponseTooLarge,
    #[error("mini-app cryptographic response is too large")]
    CryptoResponseTooLarge,
    #[error("mini-app host call failed")]
    HostCallFailed,
    #[error("sandbox fuel exhausted")]
    FuelExhausted,
    #[error("sandbox execution failed")]
    ExecutionFailed,
    #[error("sandbox guest rejected the invocation")]
    GuestRejected,
}

struct HostState<'a> {
    input: Vec<u8>,
    output: Vec<u8>,
    max_output_bytes: usize,
    permissions: SandboxPermissions,
    host: &'a mut dyn SandboxHost,
    limits: StoreLimits,
}

/// Compiled, validated WASM module. It can be reused, but every `run` call
/// gets fresh guest memory and fresh host state.
pub struct SandboxRuntime {
    engine: Engine,
    module: Module,
    limits: SandboxLimits,
}

impl SandboxRuntime {
    pub fn new(wasm: &[u8], limits: SandboxLimits) -> Result<Self, SandboxError> {
        let limits = limits.validate()?;
        if wasm.len() > limits.max_module_bytes {
            return Err(SandboxError::ModuleTooLarge);
        }

        let mut config = Config::default();
        config
            .consume_fuel(true)
            .wasm_multi_memory(false)
            .wasm_reference_types(false)
            .wasm_tail_call(false)
            .wasm_memory64(false)
            .set_max_recursion_depth(256)
            .set_max_stack_height(512 * 1024);
        let engine = Engine::new(&config);
        let module = Module::new(&engine, wasm).map_err(|_| SandboxError::InvalidModule)?;

        for import in module.imports() {
            if import.module() != "links"
                || !matches!(
                    import.name(),
                    INPUT_LENGTH_IMPORT
                        | INPUT_READ_IMPORT
                        | OUTPUT_WRITE_IMPORT
                        | NETWORK_REQUEST_IMPORT
                        | CRYPTO_OPERATION_IMPORT
                )
            {
                return Err(SandboxError::ForbiddenImport);
            }
        }
        if module.get_export(SANDBOX_ENTRYPOINT).is_none() {
            return Err(SandboxError::MissingEntrypoint);
        }
        if module.get_export(SANDBOX_MEMORY_EXPORT).is_none() {
            return Err(SandboxError::MissingMemory);
        }

        Ok(Self {
            engine,
            module,
            limits,
        })
    }

    pub fn limits(&self) -> SandboxLimits {
        self.limits
    }

    pub fn run(&self, input: &[u8]) -> Result<SandboxOutput, SandboxError> {
        if input.len() > self.limits.max_input_bytes {
            return Err(SandboxError::InputTooLarge);
        }
        let store_limits = StoreLimitsBuilder::new()
            .memory_size(self.limits.max_memory_bytes)
            .table_elements(1)
            .instances(1)
            .tables(1)
            .memories(1)
            .trap_on_grow_failure(true)
            .build();
        let mut store = Store::new(
            &self.engine,
            HostState {
                input: input.to_vec(),
                output: Vec::with_capacity(self.limits.max_output_bytes),
                max_output_bytes: self.limits.max_output_bytes,
                limits: store_limits,
            },
        );
        store.limiter(|state| &mut state.limits);
        store
            .set_fuel(self.limits.max_fuel)
            .map_err(|_| SandboxError::ExecutionFailed)?;

        let mut linker = Linker::new(&self.engine);
        linker
            .func_wrap("links", INPUT_LENGTH_IMPORT, |caller: Caller<'_, HostState>| {
                caller.data().input.len() as i32
            })
            .map_err(|_| SandboxError::ExecutionFailed)?;
        linker
            .func_wrap(
                "links",
                INPUT_READ_IMPORT,
                |mut caller: Caller<'_, HostState>, destination: i32, maximum: i32| {
                    read_input(&mut caller, destination, maximum)
                },
            )
            .map_err(|_| SandboxError::ExecutionFailed)?;
        linker
            .func_wrap(
                "links",
                OUTPUT_WRITE_IMPORT,
                |mut caller: Caller<'_, HostState>, source: i32, length: i32| {
                    write_output(&mut caller, source, length)
                },
            )
            .map_err(|_| SandboxError::ExecutionFailed)?;

        let instance = linker
            .instantiate_and_start(&mut store, &self.module)
            .map_err(map_execution_error)?;
        instance
            .get_memory(&store, SANDBOX_MEMORY_EXPORT)
            .ok_or(SandboxError::MissingMemory)?;
        let entrypoint = instance
            .get_typed_func::<(), i32>(&store, SANDBOX_ENTRYPOINT)
            .map_err(|_| SandboxError::MissingEntrypoint)?;
        let status = entrypoint
            .call(&mut store, ())
            .map_err(map_execution_error)?;
        if status != 0 {
            return Err(SandboxError::GuestRejected);
        }
        let host = store.into_data();
        Ok(SandboxOutput { bytes: host.output })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct SandboxOutput {
    pub bytes: Vec<u8>,
}

fn read_input(
    caller: &mut Caller<'_, HostState>,
    destination: i32,
    maximum: i32,
) -> Result<i32, wasmi::Error> {
    if destination < 0 || maximum < 0 {
        return Err(wasmi::Error::new("invalid input buffer"));
    }
    let input = caller.data().input.clone();
    let length = min(maximum as usize, input.len());
    let memory = caller
        .get_export(SANDBOX_MEMORY_EXPORT)
        .and_then(Extern::into_memory)
        .ok_or_else(|| wasmi::Error::new("memory export missing"))?;
    memory
        .write(caller, destination as usize, &input[..length])
        .map_err(|_| wasmi::Error::new("input buffer out of bounds"))?;
    Ok(length as i32)
}

fn write_output(
    caller: &mut Caller<'_, HostState>,
    source: i32,
    length: i32,
) -> Result<i32, wasmi::Error> {
    if source < 0 || length < 0 {
        return Err(wasmi::Error::new("invalid output buffer"));
    }
    let length = length as usize;
    if length > caller.data().max_output_bytes {
        return Err(wasmi::Error::new("sandbox output limit exceeded"));
    }
    let memory = caller
        .get_export(SANDBOX_MEMORY_EXPORT)
        .and_then(Extern::into_memory)
        .ok_or_else(|| wasmi::Error::new("memory export missing"))?;
    let mut bytes = vec![0u8; length];
    memory
        .read(&*caller, source as usize, &mut bytes)
        .map_err(|_| wasmi::Error::new("output buffer out of bounds"))?;
    let host = caller.data_mut();
    if host.output.len().saturating_add(length) > host.max_output_bytes {
        return Err(wasmi::Error::new("sandbox output limit exceeded"));
    }
    host.output.extend_from_slice(&bytes);
    bytes.fill(0);
    Ok(length as i32)
}

fn map_execution_error(error: wasmi::Error) -> SandboxError {
    if error.as_trap_code() == Some(TrapCode::OutOfFuel) {
        SandboxError::FuelExhausted
    } else {
        SandboxError::ExecutionFailed
    }
}
