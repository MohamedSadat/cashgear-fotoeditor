//! The only JavaScript-facing engine API.
#[cfg(target_arch = "wasm32")]
mod bridge {
    use std::cell::RefCell;
    use wasm_bindgen::prelude::*;

    thread_local! {
        static ERROR_CALLBACK: RefCell<Option<js_sys::Function>> = const { RefCell::new(None) };
    }

    fn report_error(message: String) {
        ERROR_CALLBACK.with(|callback| {
            if let Some(callback) = callback.borrow().as_ref() {
                let _ = callback.call1(&JsValue::NULL, &JsValue::from_str(&message));
            }
        });
    }

    #[wasm_bindgen]
    pub struct Engine {
        renderer: fotoeditor_gpu::Renderer,
    }

    #[wasm_bindgen]
    impl Engine {
        pub async fn create(
            canvas: web_sys::OffscreenCanvas,
            preference: String,
            diagnostic: bool,
            on_error: js_sys::Function,
        ) -> Result<Engine, JsValue> {
            ERROR_CALLBACK.with(|callback| *callback.borrow_mut() = Some(on_error));
            std::panic::set_hook(Box::new(|info| report_error(format!("Rust panic: {info}"))));
            let renderer =
                fotoeditor_gpu::Renderer::new(canvas, &preference, diagnostic, report_error)
                    .await
                    .map_err(|error| JsValue::from_str(&error))?;
            Ok(Self { renderer })
        }
        pub fn backend(&self) -> String {
            self.renderer.backend().into()
        }
        pub fn adapter_name(&self) -> String {
            self.renderer.adapter_name().into()
        }
        pub fn width(&self) -> u32 {
            self.renderer.width()
        }
        pub fn height(&self) -> u32 {
            self.renderer.height()
        }
        pub fn resize(&mut self, width: u32, height: u32) {
            self.renderer.resize(width, height);
        }
        pub fn frame(&mut self) -> Result<bool, JsValue> {
            self.renderer
                .frame()
                .map_err(|error| JsValue::from_str(&error))
        }
        pub fn dispose(&self) {
            self.renderer.dispose();
            ERROR_CALLBACK.with(|callback| *callback.borrow_mut() = None);
        }
        pub fn diagnostic_device_loss(&self) {
            self.renderer.simulate_device_loss();
            report_error("GPU device lost (diagnostic). Retry to restart the engine.".into());
        }
        pub fn diagnostic_panic(&self) {
            panic!("diagnostic engine panic");
        }
    }
}
