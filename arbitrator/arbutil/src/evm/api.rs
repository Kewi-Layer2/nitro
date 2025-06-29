// Copyright 2023-2024, Offchain Labs, Inc.
// For license information, see https://github.com/OffchainLabs/nitro/blob/master/LICENSE.md

use crate::{evm::user::UserOutcomeKind, Bytes20, Bytes32};
use eyre::Result;
use num_enum::IntoPrimitive;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoPrimitive)]
#[repr(u8)]
pub enum EvmApiStatus {
    Success,
    Failure,
    OutOfGas,
    WriteProtection,
}

impl From<u8> for EvmApiStatus {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::Success,
            2 => Self::OutOfGas,
            3 => Self::WriteProtection,
            _ => Self::Failure,
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(u32)]
pub enum EvmApiMethod {
    GetBytes32,
    SetTrieSlots,
    GetTransientBytes32,
    SetTransientBytes32,
    ContractCall,
    DelegateCall,
    StaticCall,
    Create1,
    Create2,
    EmitLog,
    AccountBalance,
    AccountCode,
    AccountCodeHash,
    AddPages,
    CaptureHostIO,
}

/// This offset is added to EvmApiMethod when sending a request
/// in WASM - program done is also indicated by a "request", with the
/// id below that offset, indicating program status
pub const EVM_API_METHOD_REQ_OFFSET: u32 = 0x10000000;

/// Copies data from Go into Rust.
/// Note: clone should not clone actual data, just the reader.
pub trait DataReader: Clone + Send + 'static {
    fn slice(&self) -> &[u8];
}

/// Simple implementation for `DataReader`, in case data comes from a `Vec`.
#[derive(Clone, Debug)]
pub struct VecReader(Arc<Vec<u8>>);

impl VecReader {
    pub fn new(data: Vec<u8>) -> Self {
        Self(Arc::new(data))
    }
}

impl DataReader for VecReader {
    fn slice(&self) -> &[u8] {
        self.0.as_slice()
    }
}

macro_rules! derive_math {
    ($t:ident) => {
        impl std::ops::Add for $t {
            type Output = Self;

            fn add(self, rhs: Self) -> Self {
                Self(self.0 + rhs.0)
            }
        }

        impl std::ops::AddAssign for $t {
            fn add_assign(&mut self, rhs: Self) {
                self.0 += rhs.0;
            }
        }

        impl std::ops::Sub for $t {
            type Output = Self;

            fn sub(self, rhs: Self) -> Self {
                Self(self.0 - rhs.0)
            }
        }

        impl std::ops::SubAssign for $t {
            fn sub_assign(&mut self, rhs: Self) {
                self.0 -= rhs.0;
            }
        }

        impl std::ops::Mul<u64> for $t {
            type Output = Self;

            fn mul(self, rhs: u64) -> Self {
                Self(self.0 * rhs)
            }
        }

        impl std::ops::Mul<$t> for u64 {
            type Output = $t;

            fn mul(self, rhs: $t) -> $t {
                $t(self * rhs.0)
            }
        }

        impl $t {
            /// Equivalent to the Add trait, but const.
            pub const fn add(self, rhs: Self) -> Self {
                Self(self.0 + rhs.0)
            }

            /// Equivalent to the Sub trait, but const.
            pub const fn sub(self, rhs: Self) -> Self {
                Self(self.0 - rhs.0)
            }

            pub const fn mul(self, rhs: u64) -> Self {
                Self(self.0 * rhs)
            }

            pub const fn saturating_add(self, rhs: Self) -> Self {
                Self(self.0.saturating_add(rhs.0))
            }

            pub const fn saturating_sub(self, rhs: Self) -> Self {
                Self(self.0.saturating_sub(rhs.0))
            }

            pub fn to_be_bytes(self) -> [u8; 8] {
                self.0.to_be_bytes()
            }
        }
    };
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
#[must_use]
pub struct Gas(pub u64);

derive_math!(Gas);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
#[must_use]
pub struct Ink(pub u64);

derive_math!(Ink);

pub trait EvmApi<D: DataReader>: Send + 'static {
    /// Reads the 32-byte value in the EVM state trie at offset `key`.
    /// Returns the value and the access cost in gas.
    /// Analogous to `vm.SLOAD`.
    fn get_bytes32(&mut self, key: Bytes32, evm_api_gas_to_use: Gas) -> (Bytes32, Gas);

    /// Stores the given value at the given key in Stylus VM's cache of the EVM state trie.
    /// Note that the actual values only get written after calls to `set_trie_slots`.
    fn cache_bytes32(&mut self, key: Bytes32, value: Bytes32) -> Gas;

    /// Persists any dirty values in the storage cache to the EVM state trie, dropping the cache entirely if requested.
    /// Analogous to repeated invocations of `vm.SSTORE`.
    fn flush_storage_cache(&mut self, clear: bool, gas_left: Gas) -> Result<Gas>;

    /// Reads the 32-byte value in the EVM's transient state trie at offset `key`.
    /// Analogous to `vm.TLOAD`.
    fn get_transient_bytes32(&mut self, key: Bytes32) -> Bytes32;

    /// Writes the 32-byte value in the EVM's transient state trie at offset `key`.
    /// Analogous to `vm.TSTORE`.
    fn set_transient_bytes32(&mut self, key: Bytes32, value: Bytes32) -> Result<()>;

    /// Calls the contract at the given address.
    /// Returns the EVM return data's length, the gas cost, and whether the call succeeded.
    /// Analogous to `vm.CALL`.
    fn contract_call(
        &mut self,
        contract: Bytes20,
        calldata: &[u8],
        gas_left: Gas,
        gas_req: Gas,
        value: Bytes32,
    ) -> (u32, Gas, UserOutcomeKind);

    /// Delegate-calls the contract at the given address.
    /// Returns the EVM return data's length, the gas cost, and whether the call succeeded.
    /// Analogous to `vm.DELEGATECALL`.
    fn delegate_call(
        &mut self,
        contract: Bytes20,
        calldata: &[u8],
        gas_left: Gas,
        gas_req: Gas,
    ) -> (u32, Gas, UserOutcomeKind);

    /// Static-calls the contract at the given address.
    /// Returns the EVM return data's length, the gas cost, and whether the call succeeded.
    /// Analogous to `vm.STATICCALL`.
    fn static_call(
        &mut self,
        contract: Bytes20,
        calldata: &[u8],
        gas_left: Gas,
        gas_req: Gas,
    ) -> (u32, Gas, UserOutcomeKind);

    /// Deploys a new contract using the init code provided.
    /// Returns the new contract's address on success, or the error reason on failure.
    /// In both cases the EVM return data's length and the overall gas cost are returned too.
    /// Analogous to `vm.CREATE`.
    fn create1(
        &mut self,
        code: Vec<u8>,
        endowment: Bytes32,
        gas: Gas,
    ) -> (eyre::Result<Bytes20>, u32, Gas);

    /// Deploys a new contract using the init code provided, with an address determined in part by the `salt`.
    /// Returns the new contract's address on success, or the error reason on failure.
    /// In both cases the EVM return data's length and the overall gas cost are returned too.
    /// Analogous to `vm.CREATE2`.
    fn create2(
        &mut self,
        code: Vec<u8>,
        endowment: Bytes32,
        salt: Bytes32,
        gas: Gas,
    ) -> (eyre::Result<Bytes20>, u32, Gas);

    /// Returns the EVM return data.
    /// Analogous to `vm.RETURNDATACOPY`.
    fn get_return_data(&self) -> D;

    /// Emits an EVM log with the given number of topics and data, the first bytes of which should be the topic data.
    /// Returns an error message on failure.
    /// Analogous to `vm.LOG(n)` where n ∈ [0, 4].
    fn emit_log(&mut self, data: Vec<u8>, topics: u32) -> Result<()>;

    /// Gets the balance of the given account.
    /// Returns the balance and the access cost in gas.
    /// Analogous to `vm.BALANCE`.
    fn account_balance(&mut self, address: Bytes20) -> (Bytes32, Gas);

    /// Returns the code and the access cost in gas.
    /// Analogous to `vm.EXTCODECOPY`.
    fn account_code(&mut self, arbos_version: u64, address: Bytes20, gas_left: Gas) -> (D, Gas);

    /// Gets the hash of the given address's code.
    /// Returns the hash and the access cost in gas.
    /// Analogous to `vm.EXTCODEHASH`.
    fn account_codehash(&mut self, address: Bytes20) -> (Bytes32, Gas);

    /// Determines the cost in gas of allocating additional wasm pages.
    /// Note: has the side effect of updating Geth's memory usage tracker.
    /// Not analogous to any EVM opcode.
    fn add_pages(&mut self, pages: u16) -> Gas;

    /// Captures tracing information for hostio invocations during native execution.
    fn capture_hostio(
        &mut self,
        name: &str,
        args: &[u8],
        outs: &[u8],
        start_ink: Ink,
        end_ink: Ink,
    );
}
