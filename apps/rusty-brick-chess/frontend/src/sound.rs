//! Chess event sounds. Bundled themes are Lichess AGPLv3+ packs.

use std::cell::RefCell;
use std::collections::HashMap;

use wasm_bindgen::JsCast;
use web_sys::HtmlAudioElement;

#[derive(Debug, Clone, Copy)]
pub enum SoundEvent {
    Move,
    Capture,
    Check,
    Checkmate,
    LowTime,
    Error,
    Victory,
    Defeat,
    Draw,
    Select,
    Confirmation,
}

impl SoundEvent {
    fn file(self) -> &'static str {
        match self {
            SoundEvent::Move => "Move",
            SoundEvent::Capture => "Capture",
            SoundEvent::Check => "Check",
            SoundEvent::Checkmate => "Checkmate",
            SoundEvent::LowTime => "LowTime",
            SoundEvent::Error => "Error",
            SoundEvent::Victory => "Victory",
            SoundEvent::Defeat => "Defeat",
            SoundEvent::Draw => "Draw",
            SoundEvent::Select => "Select",
            SoundEvent::Confirmation => "Confirmation",
        }
    }
}

struct Config {
    enabled: bool,
    theme: String,
    volume: f64,
}

thread_local! {
    static CONFIG: RefCell<Config> = RefCell::new(Config {
        enabled: true,
        theme: "sfx".into(),
        volume: 0.7,
    });
    static CACHE: RefCell<HashMap<String, HtmlAudioElement>> = RefCell::new(HashMap::new());
}

pub fn configure(enabled: bool, theme: &str, volume_percent: u32) {
    CONFIG.with(|c| {
        let mut c = c.borrow_mut();
        c.enabled = enabled;
        if !theme.is_empty() {
            c.theme = theme.to_owned();
        }
        c.volume = (volume_percent.min(100) as f64) / 100.0;
    });
}

pub fn play(event: SoundEvent) {
    let (enabled, theme, volume) = CONFIG.with(|c| {
        let c = c.borrow();
        (c.enabled, c.theme.clone(), c.volume)
    });
    if !enabled || volume <= 0.0 {
        return;
    }
    let key = format!("{theme}/{}", event.file());
    if let Some(audio) = CACHE.with(|cache| cache.borrow().get(&key).cloned()) {
        audio.set_volume(volume);
        let _ = audio.play();
        return;
    }
    let url = if theme == "custom" {
        format!("/api/sounds/custom/{}.ogg", event.file())
    } else {
        format!("/sounds/{theme}/{}.ogg", event.file())
    };
    let Some(window) = web_sys::window() else { return };
    let Some(document) = window.document() else { return };
    let Ok(element) = document.create_element("audio") else { return };
    let Ok(audio) = element.dyn_into::<HtmlAudioElement>() else {
        return;
    };
    audio.set_src(&url);
    audio.set_volume(volume);
    audio.set_preload("auto");
    let _ = audio.play();
    CACHE.with(|cache| cache.borrow_mut().insert(key, audio));
}
