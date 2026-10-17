// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
// Thin wrapper around window.__TAURI__ (enabled by `withGlobalTauri` in tauri.conf.json).
use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch, js_name = invoke)]
    async fn invoke_raw(command: &str, args: JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"], catch, js_name = listen)]
    async fn listen_raw(event: &str, handler: &Closure<dyn FnMut(JsValue)>) -> Result<JsValue, JsValue>;
}

pub struct TauriBridge;

impl TauriBridge {
    /// Calls a Tauri command and decodes its result.
    pub async fn call<T: DeserializeOwned>(command: &str, args: JsValue) -> Result<T, String> {
        let raw = invoke_raw(command, args).await.map_err(Self::describe)?;
        serde_wasm_bindgen::from_value(raw).map_err(|error| error.to_string())
    }

    /// Calls a Tauri command that returns nothing.
    pub async fn fire(command: &str) -> Result<(), String> {
        invoke_raw(command, JsValue::UNDEFINED)
            .await
            .map(|_| ())
            .map_err(Self::describe)
    }

    /// Builds `{ key: value }` for a command argument.
    pub fn arg(key: &str, value: &str) -> JsValue {
        let object = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&object, &key.into(), &value.into());
        object.into()
    }

    /// Subscribes to a Tauri event for the lifetime of the page.
    pub async fn listen(event: &str, mut handler: impl FnMut(JsValue) + 'static) {
        let closure = Closure::<dyn FnMut(JsValue)>::new(move |payload| handler(payload));
        let _ = listen_raw(event, &closure).await;
        closure.forget();
    }

    fn describe(error: JsValue) -> String {
        error.as_string().unwrap_or_else(|| format!("{error:?}"))
    }
}
