// gbnet_unity/src/lib.rs - Starting simple and building up!

// We'll use these later, for now just the basics
// use gbnet::{BitBuffer, BitSerialize, BitDeserialize};
use std::os::raw::{c_char, c_int};

// First, let's just test that FFI is working

/// Simple test function - adds two numbers
/// This verifies our FFI setup is working
#[no_mangle]
pub extern "C" fn gbnet_test_add(a: c_int, b: c_int) -> c_int {
    a + b
}

/// Gets the version of GBNet
/// Returns version as 0xMMNNPPPP (Major.Minor.Patch)
#[no_mangle]
pub extern "C" fn gbnet_get_version() -> u32 {
    // Version 0.1.0
    0x00_01_00_00
}

/// Simple test of our bit serialization
/// Returns the number of bytes that 28 bits would take (should be 4)
#[no_mangle]
pub extern "C" fn gbnet_test_bit_packing() -> c_int {
    // Your PlayerUpdate example: 10+10+7+1 = 28 bits
    let total_bits = 28;
    let bytes_needed = (total_bits + 7) / 8; // Round up
    bytes_needed as c_int
}

// Let's also set up a simple logging system for debugging
use std::sync::Mutex;
use once_cell::sync::Lazy;

static LAST_ERROR: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::new()));

/// Gets the last error message (for debugging)
/// Caller must free the returned string with gbnet_free_string
#[no_mangle]
pub extern "C" fn gbnet_get_last_error() -> *mut c_char {
    let error = LAST_ERROR.lock().unwrap();
    if error.is_empty() {
        std::ptr::null_mut()
    } else {
        // Convert to C string and transfer ownership
        match std::ffi::CString::new(error.as_str()) {
            Ok(c_str) => c_str.into_raw(),
            Err(_) => std::ptr::null_mut(),
        }
    }
}

/// Frees a string returned by GBNet
#[no_mangle]
pub extern "C" fn gbnet_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe {
            // Retake ownership and drop it
            let _ = std::ffi::CString::from_raw(s);
        }
    }
}

// Helper function to set error
#[allow(dead_code)]
fn set_error(msg: &str) {
    let mut error = LAST_ERROR.lock().unwrap();
    *error = msg.to_string();
}

// Clear any error
#[allow(dead_code)]
fn clear_error() {
    let mut error = LAST_ERROR.lock().unwrap();
    error.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_ffi() {
        assert_eq!(gbnet_test_add(5, 3), 8);
        assert_eq!(gbnet_get_version(), 0x00_01_00_00);
        assert_eq!(gbnet_test_bit_packing(), 4);
    }
}