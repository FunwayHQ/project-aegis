//! Sprint 14: Example Edge Function - External API with Caching
//!
//! This edge function demonstrates:
//! 1. Making HTTP GET requests to external APIs
//! 2. Caching responses in DragonflyDB
//! 3. Using the host API for data and external access
//!
//! Use case: Fetch exchange rate data from an external API and cache it

use core::slice;
use serde_json::Value;

// Host API function declarations
extern "C" {
    /// Log a message to the host
    fn log(ptr: *const u8, len: u32);

    /// Get a value from cache
    /// Returns the length of the value (stored in shared buffer), or -1 if not found
    fn cache_get(key_ptr: *const u8, key_len: u32) -> i32;

    /// Set a value in cache
    /// Returns 0 on success, -1 on error
    fn cache_set(key_ptr: *const u8, key_len: u32, value_ptr: *const u8, value_len: u32, ttl: u32) -> i32;

    /// Make an HTTP GET request
    /// Returns the length of the response (stored in shared buffer), or -1 on error
    fn http_get(url_ptr: *const u8, url_len: u32) -> i32;

    /// Get data from shared buffer
    /// Returns number of bytes copied, or -1 on error
    fn get_shared_buffer(dest_ptr: *mut u8, offset: u32, length: u32) -> i32;
}

/// Helper function to log messages
fn log_message(msg: &str) {
    unsafe {
        log(msg.as_ptr(), msg.len() as u32);
    }
}

/// Helper function to get from cache
fn get_from_cache(key: &str) -> Option<Vec<u8>> {
    unsafe {
        let result_len = cache_get(key.as_ptr(), key.len() as u32);
        if result_len < 0 {
            return None;
        }

        // Allocate buffer and read from shared buffer
        let mut buffer = vec![0u8; result_len as usize];
        let copied = get_shared_buffer(buffer.as_mut_ptr(), 0, result_len as u32);
        if copied < 0 {
            return None;
        }

        Some(buffer)
    }
}

/// Helper function to set in cache
fn set_in_cache(key: &str, value: &[u8], ttl: u32) -> bool {
    unsafe {
        let result = cache_set(key.as_ptr(), key.len() as u32, value.as_ptr(), value.len() as u32, ttl);
        result == 0
    }
}

/// Helper function to make HTTP GET request
fn http_get_request(url: &str) -> Option<Vec<u8>> {
    unsafe {
        let result_len = http_get(url.as_ptr(), url.len() as u32);
        if result_len < 0 {
            return None;
        }

        // Allocate buffer and read from shared buffer
        let mut buffer = vec![0u8; result_len as usize];
        let copied = get_shared_buffer(buffer.as_mut_ptr(), 0, result_len as u32);
        if copied < 0 {
            return None;
        }

        Some(buffer)
    }
}

/// Main edge function: Fetch exchange rate data with caching
///
/// This function demonstrates a typical edge function workflow:
/// 1. Check cache first (fast path)
/// 2. If cache miss, fetch from external API (slow path)
/// 3. Cache the result for future requests
/// 4. Return the data
#[no_mangle]
pub extern "C" fn fetch_exchange_rates() -> i32 {
    log_message("Edge function: fetch_exchange_rates started");

    let cache_key = "exchange_rates:usd";

    // Try to get from cache first
    log_message("Checking cache for exchange rates...");
    if let Some(cached_data) = get_from_cache(cache_key) {
        log_message("Cache HIT! Returning cached exchange rates");
        return 0; // Success - data is in shared buffer
    }

    log_message("Cache MISS! Fetching from external API...");

    // Cache miss - fetch from external API
    // Using httpbin.org/json as a demo API (returns sample JSON)
    let api_url = "https://httpbin.org/json";

    match http_get_request(api_url) {
        Some(response_data) => {
            log_message("Successfully fetched data from external API");

            // Validate JSON response
            if let Ok(json_str) = std::str::from_utf8(&response_data) {
                if serde_json::from_str::<Value>(json_str).is_ok() {
                    log_message("Response is valid JSON");

                    // Cache the result for 60 seconds
                    if set_in_cache(cache_key, &response_data, 60) {
                        log_message("Successfully cached exchange rates");
                    } else {
                        log_message("Warning: Failed to cache data");
                    }

                    return 0; // Success
                } else {
                    log_message("Error: Invalid JSON response");
                    return -1;
                }
            } else {
                log_message("Error: Response is not valid UTF-8");
                return -1;
            }
        }
        None => {
            log_message("Error: Failed to fetch from external API");
            return -1;
        }
    }
}

/// Simple test function: Just logs a message
#[no_mangle]
pub extern "C" fn test_logging() -> i32 {
    log_message("Test logging from edge function");
    0
}

/// Test function: Cache operations
#[no_mangle]
pub extern "C" fn test_cache() -> i32 {
    log_message("Testing cache operations...");

    // Set a value
    let key = "test:key";
    let value = b"Hello from edge function!";

    if set_in_cache(key, value, 30) {
        log_message("Cache SET successful");
    } else {
        log_message("Cache SET failed");
        return -1;
    }

    // Get the value back
    if let Some(retrieved) = get_from_cache(key) {
        if retrieved == value {
            log_message("Cache GET successful - value matches!");
            return 0;
        } else {
            log_message("Cache GET returned wrong value");
            return -1;
        }
    } else {
        log_message("Cache GET failed");
        return -1;
    }
}

/// Test function: HTTP GET request
#[no_mangle]
pub extern "C" fn test_http() -> i32 {
    log_message("Testing HTTP GET request...");

    let url = "https://httpbin.org/get";

    match http_get_request(url) {
        Some(response) => {
            log_message("HTTP GET successful");

            // Try to parse as JSON
            if let Ok(json_str) = std::str::from_utf8(&response) {
                if serde_json::from_str::<Value>(json_str).is_ok() {
                    log_message("Response is valid JSON");
                    return 0;
                }
            }

            log_message("Warning: Response is not valid JSON");
            return 0; // Still success even if not JSON
        }
        None => {
            log_message("HTTP GET failed");
            return -1;
        }
    }
}

// Memory allocation functions for the host to allocate memory in Wasm
#[no_mangle]
pub extern "C" fn alloc(size: u32) -> *mut u8 {
    let mut buffer = Vec::with_capacity(size as usize);
    let ptr = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    ptr
}

#[no_mangle]
pub extern "C" fn dealloc(ptr: *mut u8, size: u32) {
    unsafe {
        let _ = Vec::from_raw_parts(ptr, size as usize, size as usize);
    }
}
