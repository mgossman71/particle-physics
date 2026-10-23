//! Control-room sound from WebAudio oscillators and noise: Geiger clicks that
//! follow neutron flux, a hum that rises with power, a klaxon during meltdown,
//! a thunk on SCRAM and a two-tone chime when an incident starts.
//!
//! Browsers only let audio start after the user has interacted with the page,
//! so the app creates `Sound` after the first pointer press.

use crate::reactor::{ReactorStats, Status};
use web_sys::{
    AudioBuffer, AudioContext, AudioContextState, AudioNode, BiquadFilterNode, BiquadFilterType,
    GainNode, OscillatorNode, OscillatorType,
};

const MASTER_GAIN: f32 = 0.8;
/// Fissions per second treated as full power for the hum
const FULL_POWER: f64 = 50.0;
const MAX_CLICKS_PER_SEC: f64 = 70.0;
const KLAXON_HALF_PERIOD: f64 = 0.45;

pub struct Sound {
    ctx: AudioContext,
    master: GainNode,
    hum: OscillatorNode,
    hum_filter: BiquadFilterNode,
    hum_gain: GainNode,
    klaxon: OscillatorNode,
    klaxon_gain: GainNode,
    click: AudioBuffer,
    /// Fractional Geiger clicks carried into the next frame
    click_debt: f64,
    klaxon_phase: f64,
    was_scrammed: bool,
    had_incident: bool,
}

fn connect(from: &AudioNode, to: &AudioNode) {
    let _ = from.connect_with_audio_node(to);
}

impl Sound {
    pub fn new() -> Option<Self> {
        let ctx = AudioContext::new().ok()?;
        let master = ctx.create_gain().ok()?;
        master.gain().set_value(0.0);
        connect(&master, &ctx.destination());

        // Reactor hum: a low sawtooth through a lowpass that opens with power
        let hum = ctx.create_oscillator().ok()?;
        hum.set_type(OscillatorType::Sawtooth);
        hum.frequency().set_value(45.0);
        let hum_filter = ctx.create_biquad_filter().ok()?;
        hum_filter.set_type(BiquadFilterType::Lowpass);
        hum_filter.frequency().set_value(200.0);
        let hum_gain = ctx.create_gain().ok()?;
        hum_gain.gain().set_value(0.0);
        connect(&hum, &hum_filter);
        connect(&hum_filter, &hum_gain);
        connect(&hum_gain, &master);
        hum.start().ok()?;

        let klaxon = ctx.create_oscillator().ok()?;
        klaxon.set_type(OscillatorType::Square);
        klaxon.frequency().set_value(660.0);
        let klaxon_gain = ctx.create_gain().ok()?;
        klaxon_gain.gain().set_value(0.0);
        connect(&klaxon, &klaxon_gain);
        connect(&klaxon_gain, &master);
        klaxon.start().ok()?;

        // Geiger click: 4ms of fast-decaying noise
        let rate = ctx.sample_rate();
        let len = (rate * 0.004) as usize;
        let samples: Vec<f32> = (0..len)
            .map(|i| ((js_sys::Math::random() * 2.0 - 1.0) * (-(i as f64) / (len as f64 * 0.15)).exp() * 0.6) as f32)
            .collect();
        let click = ctx.create_buffer(1, len as u32, rate).ok()?;
        click.copy_to_channel(&samples, 0).ok()?;

        Some(Self {
            ctx, master, hum, hum_filter, hum_gain, klaxon, klaxon_gain, click,
            click_debt: 0.0, klaxon_phase: 0.0, was_scrammed: false, had_incident: false,
        })
    }

    /// Fades everything in or out; resumes the context if the browser suspended it.
    pub fn set_enabled(&self, on: bool) {
        if on && self.ctx.state() == AudioContextState::Suspended { let _ = self.ctx.resume(); }
        let now = self.ctx.current_time();
        let _ = self.master.gain().set_target_at_time(if on { MASTER_GAIN } else { 0.0 }, now, 0.08);
    }

    pub fn update(&mut self, secs: f64, stats: &ReactorStats, incident: bool) {
        let now = self.ctx.current_time();
        let power = (stats.power / FULL_POWER).min(1.0);

        let _ = self.hum.frequency().set_target_at_time((45.0 + 35.0 * power) as f32, now, 0.3);
        let _ = self.hum_filter.frequency().set_target_at_time((200.0 + 900.0 * power) as f32, now, 0.3);
        let _ = self.hum_gain.gain().set_target_at_time((0.015 + 0.05 * power) as f32, now, 0.3);

        // Geiger counter: click rate follows the neutron count, plus background
        let rate = (stats.neutrons as f64 * 0.6 + 0.5).min(MAX_CLICKS_PER_SEC);
        let due = rate * secs + self.click_debt;
        let clicks = due.floor();
        self.click_debt = due - clicks;
        for _ in 0..clicks as usize {
            if let Ok(src) = self.ctx.create_buffer_source() {
                src.set_buffer(Some(&self.click));
                connect(&src, &self.master);
                let _ = src.start_with_when(now + js_sys::Math::random() * secs);
            }
        }

        // Klaxon alternates two tones while the core is melting down
        let melting = stats.status == Status::Meltdown;
        self.klaxon_phase = if melting { self.klaxon_phase + secs } else { 0.0 };
        let high = (self.klaxon_phase / KLAXON_HALF_PERIOD) as u32 % 2 == 0;
        let _ = self.klaxon.frequency().set_value_at_time(if high { 660.0 } else { 440.0 }, now);
        let _ = self.klaxon_gain.gain().set_target_at_time(if melting { 0.05 } else { 0.0 }, now, 0.03);

        if stats.scrammed && !self.was_scrammed { self.thunk(now); }
        self.was_scrammed = stats.scrammed;
        if incident && !self.had_incident {
            self.beep(now, 880.0);
            self.beep(now + 0.18, 660.0);
        }
        self.had_incident = incident;
    }

    /// Rods slamming home: a falling low sine with a quick decay
    fn thunk(&self, now: f64) {
        let (Ok(osc), Ok(gain)) = (self.ctx.create_oscillator(), self.ctx.create_gain()) else { return };
        let _ = osc.frequency().set_value_at_time(120.0, now);
        let _ = osc.frequency().exponential_ramp_to_value_at_time(35.0, now + 0.35);
        let _ = gain.gain().set_value_at_time(0.4, now);
        let _ = gain.gain().exponential_ramp_to_value_at_time(0.001, now + 0.5);
        connect(&osc, &gain);
        connect(&gain, &self.master);
        let _ = osc.start_with_when(now);
        let _ = osc.stop_with_when(now + 0.55);
    }

    fn beep(&self, at: f64, freq: f32) {
        let (Ok(osc), Ok(gain)) = (self.ctx.create_oscillator(), self.ctx.create_gain()) else { return };
        osc.set_type(OscillatorType::Square);
        osc.frequency().set_value(freq);
        let _ = gain.gain().set_value_at_time(0.06, at);
        let _ = gain.gain().exponential_ramp_to_value_at_time(0.001, at + 0.15);
        connect(&osc, &gain);
        connect(&gain, &self.master);
        let _ = osc.start_with_when(at);
        let _ = osc.stop_with_when(at + 0.16);
    }
}
