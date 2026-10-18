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
    last: RwSignal<Option<Attempt>>,
    /// A room whose scan waits for the person's yes: (room id, name).
    pub pending: RwSignal<Option<(String, String)>>,
}

/// What was started last, so "Try again" can start it once more.
#[derive(Clone)]
enum Attempt {
    Videos(Vec<String>, String),
    Room(String, String),
}

impl ScanFlow {
    pub fn new() -> Self {
        Self {
            view: RwSignal::new(View::Menu),
            progress: RwSignal::new(None),
            last: RwSignal::new(None),
            pending: RwSignal::new(None),
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
        self.last.set(Some(Attempt::Videos(paths.clone(), label.clone())));
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

    /// Asks which films go into the room before anything starts.
    pub fn ask_scan(self, room_id: String, label: String) {
        self.pending.set(Some((room_id, label)));
    }

    pub fn cancel_ask(self) {
        self.pending.set(None);
    }

    /// Reconstructs a room with exactly these films (the ticks are remembered by the backend).
    pub fn scan_room_with(self, room_id: String, label: String, videos: Vec<String>) {
        self.last.set(Some(Attempt::Room(room_id.clone(), label.clone())));
        self.progress.set(None);
        self.view.set(View::Scanning(label));
        leptos::task::spawn_local(async move {
            let list = js_sys::Array::new();
            for name in &videos {
                list.push(&name.into());
            }
            let args = TauriBridge::object(&[("room", room_id.as_str().into()), ("videos", list.into())]);
            if let Err(message) = TauriBridge::fire_with("start_room_scan", args).await {
                self.view.set(View::Failed(message));
            }
        });
    }

    /// Reconstructs a room with the films ticked last time.
    pub fn scan_room(self, room_id: String, label: String) {
        self.last.set(Some(Attempt::Room(room_id.clone(), label.clone())));
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

    /// True when there is something to try again.
    pub fn can_retry(self) -> bool {
        self.last.get().is_some()
    }

    /// Starts the last scan once more.
    pub fn retry(self) {
        match self.last.get_untracked() {
            Some(Attempt::Videos(paths, label)) => self.start(paths, label),
            Some(Attempt::Room(id, label)) => self.scan_room(id, label),
            None => self.home(),
        }
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
