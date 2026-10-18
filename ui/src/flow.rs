// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
use leptos::prelude::*;

use crate::bridge::TauriBridge;
use crate::model::{ProgressEvent, ScanResult, View};

/// The path of one scan through the app: choose a video, watch it reconstruct, look at the result.
/// It only holds signals (which are `Copy`), so it can be handed to any closure.
#[derive(Clone, Copy)]
pub struct ScanFlow {
    pub view: RwSignal<View>,
    pub progress: RwSignal<Option<ProgressEvent>>,
}

impl ScanFlow {
    pub fn new() -> Self {
        Self {
            view: RwSignal::new(View::Menu),
            progress: RwSignal::new(None),
        }
    }

    /// Reacts to the events the backend sends while a scan runs. Call once at startup.
    pub async fn listen(self) {
        TauriBridge::listen("reconstruct://progress", move |event| {
            if let Some(update) = TauriBridge::payload::<ProgressEvent>(&event) {
                self.progress.set(Some(update));
            }
        })
        .await;
        TauriBridge::listen("reconstruct://done", move |event| {
            match TauriBridge::payload::<ScanResult>(&event) {
                Some(result) => self.view.set(View::Viewing(result)),
                None => self.view.set(View::Failed("The scan finished, but its result could not be read.".into())),
            }
        })
        .await;
        TauriBridge::listen("reconstruct://failed", move |event| {
            let message = TauriBridge::payload::<String>(&event).unwrap_or_else(|| "Unknown error.".into());
            self.view.set(View::Failed(message));
        })
        .await;
        TauriBridge::listen("reconstruct://cancelled", move |_| self.view.set(View::Menu)).await;
    }

    /// `label` is what the progress screen calls this scan ("IMG_7695.MP4", "5 videos").
    pub fn start(self, paths: Vec<String>, label: String) {
        self.progress.set(None);
        self.view.set(View::Scanning(label));
        leptos::task::spawn_local(async move {
            if let Err(message) =
                TauriBridge::fire_with("start_reconstruction", TauriBridge::args("paths", &paths)).await
            {
                self.view.set(View::Failed(message));
            }
        });
    }

    pub fn cancel(self) {
        leptos::task::spawn_local(async move {
            let _ = TauriBridge::fire("cancel_reconstruction").await;
        });
    }

    /// Reconstructs every video in a room of the library.
    pub fn scan_room(self, room_id: String, label: String) {
        self.progress.set(None);
        self.view.set(View::Scanning(label));
        leptos::task::spawn_local(async move {
            if let Err(message) =
                TauriBridge::fire_with("start_room_scan", TauriBridge::map(&[("room", &room_id)])).await
            {
                self.view.set(View::Failed(message));
            }
        });
    }

    pub fn show(self, scan: ScanResult) {
        self.view.set(View::Viewing(scan));
    }

    pub fn go(self, view: View) {
        self.view.set(view);
    }

    /// Back to the start menu.
    pub fn home(self) {
        self.view.set(View::Menu);
    }
}
