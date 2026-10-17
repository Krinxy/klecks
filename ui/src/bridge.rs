// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
// Thin wrappers around window.__TAURI__ (enabled by `withGlobalTauri` in tauri.conf.json)
// and window.klecksViewer (ui/public/viewer.js).
use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch, js_name = invoke)]
    async fn invoke_raw(command: &str, args: JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = convertFileSrc)]
    fn convert_file_src(path: &str) -> String;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"], catch, js_name = listen)]
    async fn listen_raw(event: &str, handler: &Closure<dyn FnMut(JsValue)>) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = ["window", "klecksViewer"], js_name = mount)]
    fn viewer_mount(id: &str) -> bool;

    #[wasm_bindgen(js_namespace = ["window", "klecksViewer"], catch, js_name = load)]
    async fn viewer_load(url: &str, scene_scale: f64) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = ["window", "klecksViewer"], js_name = resetView)]
    fn viewer_reset_view();

    #[wasm_bindgen(js_namespace = ["window", "klecksViewer"], js_name = dispose)]
    fn viewer_dispose();
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
        Self::fire_with(command, JsValue::UNDEFINED).await
    }

    pub async fn fire_with(command: &str, args: JsValue) -> Result<(), String> {
        invoke_raw(command, args).await.map(|_| ()).map_err(Self::describe)
    }

    /// Builds `{ key: [value, ...] }` for a command argument that is a list of strings.
    pub fn args(key: &str, values: &[String]) -> JsValue {
        let list = js_sys::Array::new();
        for value in values {
            list.push(&value.into());
        }
        let object = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&object, &key.into(), &list);
        object.into()
    }

    /// Subscribes to a Tauri event for the lifetime of the page.
    pub async fn listen(event: &str, handler: impl FnMut(JsValue) + 'static) {
        let closure = Closure::<dyn FnMut(JsValue)>::new(handler);
        let _ = listen_raw(event, &closure).await;
        closure.forget();
    }

    /// Decodes `event.payload` of a Tauri event.
    pub fn payload<T: DeserializeOwned>(event: &JsValue) -> Option<T> {
        let payload = js_sys::Reflect::get(event, &"payload".into()).ok()?;
        serde_wasm_bindgen::from_value(payload).ok()
    }

    fn describe(error: JsValue) -> String {
        error.as_string().unwrap_or_else(|| format!("{error:?}"))
    }
}

/// What the viewer ended up showing.
pub struct LoadedScene {
    /// "splats" or "points"
    pub kind: String,
    pub count: u64,
}

/// The 3D scene viewer (three.js + Spark, see ui/public/viewer.js).
pub struct Viewer3d;

impl Viewer3d {
    pub fn mount(element_id: &str) -> bool {
        viewer_mount(element_id)
    }

    /// Loads a scene file from disk through Tauri's asset protocol. `scene_scale` is the typical
    /// surface distance from the filming position (0 = let the viewer guess).
    pub async fn load(path: &str, scene_scale: f64) -> Result<LoadedScene, String> {
        let loaded = viewer_load(&convert_file_src(path), scene_scale)
            .await
            .map_err(|error| {
                js_sys::Reflect::get(&error, &"message".into())
                    .ok()
                    .and_then(|message| message.as_string())
                    .unwrap_or_else(|| format!("{error:?}"))
            })?;
        let field = |name: &str| js_sys::Reflect::get(&loaded, &name.into()).unwrap_or(JsValue::UNDEFINED);
        Ok(LoadedScene {
            kind: field("kind").as_string().unwrap_or_default(),
            count: field("count").as_f64().unwrap_or(0.0) as u64,
        })
    }

    pub fn reset_view() {
        viewer_reset_view();
    }

    pub fn dispose() {
        viewer_dispose();
    }
}
