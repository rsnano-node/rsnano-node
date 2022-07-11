use std::{ffi::c_void, sync::Arc};

use num::FromPrimitive;

use crate::{
    state_block_signature_verification::{
        StateBlockSignatureVerificationResult, StateBlockSignatureVerificationValue,
    },
    Account, StateBlockSignatureVerification,
};

use super::{BlockHandle, EpochsHandle, LoggerHandle, LoggerMT, SignatureCheckerHandle};

pub struct StateBlockSignatureVerificationHandle {
    verification: StateBlockSignatureVerification,
}

#[repr(C)]
pub struct StateBlockSignatureVerificationValueDto {
    pub block: *mut BlockHandle,
    pub account: [u8; 32],
    pub verification: u8,
}

pub struct StateBlockSignatureVerificationResultHandle {
    verifications: Vec<i32>,
    hashes: Vec<[u8; 32]>,
    signatures: Vec<[u8; 64]>,
    items: Vec<StateBlockSignatureVerificationValueDto>,
}

#[repr(C)]
pub struct StateBlockSignatureVerificationResultDto {
    hashes: *const [u8; 32],
    signatures: *const [u8; 64],
    verifications: *const i32,
    items: *const StateBlockSignatureVerificationValueDto,
    size: usize,
    handle: *mut StateBlockSignatureVerificationResultHandle,
}

#[no_mangle]
pub unsafe extern "C" fn rsn_state_block_signature_verification_create(
    checker: *const SignatureCheckerHandle,
    epochs: *const EpochsHandle,
    logger: *mut LoggerHandle,
    timing_logging: bool,
    verification_size: usize,
) -> *mut StateBlockSignatureVerificationHandle {
    let checker = (*checker).checker.clone();
    let epochs = Arc::new((*epochs).epochs.clone());
    let logger = Arc::new(LoggerMT::new(Box::from_raw(logger)));
    let verification = StateBlockSignatureVerification::builder()
        .signature_checker(checker)
        .epochs(epochs)
        .logger(logger)
        .enable_timing_logging(timing_logging)
        .verification_size(verification_size)
        .spawn()
        .unwrap();
    Box::into_raw(Box::new(StateBlockSignatureVerificationHandle {
        verification,
    }))
}

#[no_mangle]
pub extern "C" fn rsn_state_block_signature_verification_destroy(
    handle: *mut StateBlockSignatureVerificationHandle,
) {
    drop(unsafe { Box::from_raw(handle) });
}

#[no_mangle]
pub extern "C" fn rsn_state_block_signature_verification_result_destroy(
    handle: *mut StateBlockSignatureVerificationResultHandle,
) {
    drop(unsafe { Box::from_raw(handle) });
}

type StateBlockVerifiedCallback =
    unsafe extern "C" fn(*mut c_void, *const StateBlockSignatureVerificationResultDto);

struct ContextHandle(*mut c_void);

impl ContextHandle {
    fn get(&self) -> *mut c_void {
        self.0
    }
}

unsafe impl Send for ContextHandle {}
unsafe impl Sync for ContextHandle {}

#[no_mangle]
pub extern "C" fn rsn_state_block_signature_verification_verified_callback(
    handle: *mut StateBlockSignatureVerificationHandle,
    callback: StateBlockVerifiedCallback,
    context: *mut c_void,
) {
    let handle = unsafe { &mut *handle };

    let context_handle = ContextHandle(context);

    let callback_adapter = Box::new(move |result: StateBlockSignatureVerificationResult| {
        let result_handle = Box::new(StateBlockSignatureVerificationResultHandle {
            verifications: result.verifications,
            hashes: result.hashes.iter().map(|x| x.to_bytes()).collect(),
            signatures: result.signatures.iter().map(|x| *x.as_bytes()).collect(),
            items: result
                .items
                .iter()
                .map(StateBlockSignatureVerificationValueDto::from)
                .collect(),
        });

        let result_dto = StateBlockSignatureVerificationResultDto {
            hashes: result_handle.hashes.as_ptr(),
            signatures: result_handle.signatures.as_ptr(),
            verifications: result_handle.verifications.as_ptr(),
            size: result_handle.verifications.len(),
            items: result_handle.items.as_ptr(),
            handle: Box::into_raw(result_handle),
        };

        unsafe {
            (callback)(context_handle.get(), &result_dto);
        }
    });

    handle
        .verification
        .set_blocks_verified_callback(callback_adapter);
}

type TransitionInactiveCallback = unsafe extern "C" fn(*mut c_void);

#[no_mangle]
pub extern "C" fn rsn_state_block_signature_verification_transition_inactive_callback(
    handle: *mut StateBlockSignatureVerificationHandle,
    callback: TransitionInactiveCallback,
    context: *mut c_void,
) {
    let handle = unsafe { &mut *handle };
    let context_handle = ContextHandle(context);

    let callback_adapter = Box::new(move || unsafe {
        (callback)(context_handle.get());
    });

    handle
        .verification
        .set_transition_inactive_callback(callback_adapter);
}

#[no_mangle]
pub extern "C" fn rsn_state_block_signature_verification_stop(
    handle: *mut StateBlockSignatureVerificationHandle,
) {
    let verification = unsafe { &mut (*handle).verification };
    verification.stop().unwrap();
}

#[no_mangle]
pub extern "C" fn rsn_state_block_signature_verification_is_active(
    handle: *const StateBlockSignatureVerificationHandle,
) -> bool {
    let verification = unsafe { &(*handle).verification };
    verification.is_active()
}

#[no_mangle]
pub extern "C" fn rsn_state_block_signature_verification_add(
    handle: *mut StateBlockSignatureVerificationHandle,
    block: *const StateBlockSignatureVerificationValueDto,
) {
    let verification = unsafe { &mut (*handle).verification };
    let block = unsafe { &*block };
    let block = StateBlockSignatureVerificationValue {
        block: unsafe { &*block.block }.block.clone(),
        account: Account::from_bytes(block.account),
        verification: FromPrimitive::from_u8(block.verification).unwrap(),
    };
    verification.add(block);
}

#[no_mangle]
pub extern "C" fn rsn_state_block_signature_verification_size(
    handle: *const StateBlockSignatureVerificationHandle,
) -> usize {
    let verification = unsafe { &(*handle).verification };
    verification.size()
}

impl From<&StateBlockSignatureVerificationValue> for StateBlockSignatureVerificationValueDto {
    fn from(value: &StateBlockSignatureVerificationValue) -> Self {
        StateBlockSignatureVerificationValueDto {
            block: Box::into_raw(Box::new(BlockHandle {
                block: value.block.clone(),
            })),
            account: value.account.to_bytes(),
            verification: value.verification as u8,
        }
    }
}
