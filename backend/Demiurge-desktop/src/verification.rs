//! Build-time verification shims. Runtime builds compile this module empty.

// This feature checks/tests non-OCR desktop code on targets for which ort
// ships no binary. Default runnable builds never enable this symbol.
#[cfg(feature = "check-no-ocr-link")]
#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn OrtGetApiBase() -> *const std::ffi::c_void {
    std::ptr::null()
}
