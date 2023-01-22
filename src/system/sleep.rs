use core::time::Duration;

pub async fn sleep(duration: &Duration) {
    use wasm_bindgen::closure::Closure;

    let (sender, receiver) = futures_channel::oneshot::channel();
    let mut sender = Some(sender);
    let callback: Closure<dyn FnMut()> = Closure::new(move || {
        if let Some(s) = sender.take() {
            s.send(()).expect("Unable to send wake-up");
        }
    });
    #[allow(clippy::cast_precision_loss)]
    let millis = (duration.as_micros() as f64) / 1000.0;
    let millis: js_sys::Number = millis.into();
    ffi::set_timeout(callback.as_ref(), millis);
    receiver.await.expect("Failed to receive wake-up");
}

mod ffi {
    use js_sys::Number;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_name = "setTimeout")]
        pub fn set_timeout(callback: &JsValue, milliseconds: Number);
    }
}
