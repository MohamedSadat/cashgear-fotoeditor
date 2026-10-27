//! Each cancellable codec runs in a disposable WASM instance with a heap ceiling.
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst};
use wasm_bindgen::prelude::*;

struct BoundedAllocator;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static LIMIT: AtomicUsize = AtomicUsize::new(usize::MAX);
static REFUSED: AtomicBool = AtomicBool::new(false);
fn reserve(bytes: usize) -> bool {
    let result = LIVE.try_update(SeqCst, SeqCst, |live| {
        live.checked_add(bytes).filter(|n| *n <= LIMIT.load(SeqCst))
    });
    match result {
        Ok(old) => {
            PEAK.fetch_max(old + bytes, SeqCst);
            true
        }
        Err(_) => {
            REFUSED.store(true, SeqCst);
            false
        }
    }
}
// Reallocation charges both blocks until success, covering moving reallocations.
unsafe impl GlobalAlloc for BoundedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if !reserve(layout.size()) {
            return std::ptr::null_mut();
        }
        let ptr = unsafe { System.alloc(layout) };
        if ptr.is_null() {
            LIVE.fetch_sub(layout.size(), SeqCst);
        }
        ptr
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if !reserve(layout.size()) {
            return std::ptr::null_mut();
        }
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if ptr.is_null() {
            LIVE.fetch_sub(layout.size(), SeqCst);
        }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe {
            System.dealloc(ptr, layout);
        }
        LIVE.fetch_sub(layout.size(), SeqCst);
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if !reserve(size) {
            return std::ptr::null_mut();
        }
        let new = unsafe { System.realloc(ptr, layout, size) };
        LIVE.fetch_sub(if new.is_null() { size } else { layout.size() }, SeqCst);
        new
    }
}
#[global_allocator]
static ALLOCATOR: BoundedAllocator = BoundedAllocator;
thread_local! { static OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) }; }
#[wasm_bindgen]
pub fn codec_limit(bytes: u32) -> Result<(), JsValue> {
    if bytes as usize <= LIVE.load(SeqCst) {
        return Err(JsValue::from_str("Not enough memory for this export"));
    }
    REFUSED.store(false, SeqCst);
    PEAK.store(LIVE.load(SeqCst), SeqCst);
    LIMIT.store(bytes as usize, SeqCst);
    Ok(())
}
#[wasm_bindgen]
pub fn codec_refused() -> bool {
    REFUSED.load(SeqCst)
}
#[wasm_bindgen]
pub fn codec_peak() -> u32 {
    PEAK.load(SeqCst) as u32
}
pub fn store_output(bytes: Vec<u8>) -> u32 {
    let len = bytes.len() as u32;
    OUTPUT.with(|output| *output.borrow_mut() = bytes);
    len
}
pub fn output_capacity() -> u64 {
    OUTPUT.with(|output| output.borrow().capacity() as u64)
}
#[wasm_bindgen]
pub fn codec_clear() {
    OUTPUT.with(|output| *output.borrow_mut() = Vec::new());
}
#[wasm_bindgen]
pub fn codec_chunk(offset: u32) -> Vec<u8> {
    OUTPUT.with(|output| {
        let output = output.borrow();
        let start = (offset as usize).min(output.len());
        output[start..(start + 1_048_576).min(output.len())].to_vec()
    })
}
#[wasm_bindgen]
pub fn codec_encode(
    rgba: &[u8],
    width: u32,
    height: u32,
    dpi: f64,
    options: &str,
) -> Result<u32, JsValue> {
    if options.len() > 4096 {
        return Err(JsValue::from_str("Invalid export options"));
    }
    let options = serde_json::from_str(options)
        .map_err(|e| JsValue::from_str(&format!("Invalid export options: {e}")))?;
    let bytes = fotoeditor_io::encode_image_rgba(rgba, width, height, dpi, &options)
        .map_err(|e| JsValue::from_str(&e))?;
    Ok(store_output(bytes))
}
