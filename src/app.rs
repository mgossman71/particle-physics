use leptos::prelude::*;
use leptos::html::Canvas;
use leptos_meta::{provide_meta_context, MetaTags, Stylesheet, Title};
use leptos_router::{
    components::{Route, Router, Routes},
    StaticSegment,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use crate::reactor::{ReactorStats, Status, AMBIENT_TEMP, DEFAULT_PUMP, MELTDOWN_TEMP};
use crate::shift::{ShiftState, ShiftStats};

pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <AutoReload options=options.clone() />
                <HydrationScripts options/>
                <MetaTags/>
            </head>
            <body><App/></body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    view! {
        <Stylesheet id="leptos" href="/pkg/my_leptos_app.css"/>
        <Title text="Particle Sim • Rust + Leptos"/>
        <Router>
            <main>
                <Routes fallback=|| "Page not found.".into_view()>
                    <Route path=StaticSegment("") view=HomePage/>
                </Routes>
            </main>
        </Router>
    }
}

pub const MODE_PLEXUS: u8 = 0;
pub const MODE_REACTOR: u8 = 1;

#[derive(Clone)]
pub struct SimSettings {
    pub mode: u8,
    pub count: usize,
    pub conn_dist: f64,
    pub speed: f64,
    pub color_scheme: u8,
    pub repel: bool,
    /// 0.0 = short trails, 1.0 = long trails
    pub trail: f64,
    /// Reactor: operator's rod setting, 0.0 withdrawn to 1.0 fully inserted
    pub rods: f64,
    /// Reactor: fuel loading, 0.0..1.0
    pub fuel: f64,
    /// Reactor: coolant pump speed, 0.0..1.0
    pub pump: f64,
    /// Reactor: bumped by buttons; the sim acts on each change
    pub scram_seq: u32,
    pub reset_seq: u32,
    pub shift_seq: u32,
    pub free_play_seq: u32,
    /// Sound toggle, and whether the user has pressed anything yet (browsers
    /// refuse to start audio before that)
    pub sound_on: bool,
    pub unlocked: bool,
}
impl Default for SimSettings {
    fn default() -> Self {
        Self {
            mode: MODE_PLEXUS, count: 300, conn_dist: 115.0, speed: 1.0, color_scheme: 0,
            repel: false, trail: 0.62, rods: 0.5, fuel: 0.75, pump: DEFAULT_PUMP,
            scram_seq: 0, reset_seq: 0, shift_seq: 0, free_play_seq: 0,
            sound_on: true, unlocked: false,
        }
    }
}

/// Mouse state shared with the animation loop
#[derive(Default)]
pub struct Pointer {
    pub pos: Option<(f64, f64)>,
    pub clicks: Vec<(f64, f64)>,
}

/// Signals the animation loop writes back to the UI
#[derive(Clone, Copy)]
pub struct SimOutputs {
    pub fps: RwSignal<u32>,
    pub stats: RwSignal<ReactorStats>,
    pub shift: RwSignal<Option<ShiftStats>>,
    pub best: RwSignal<u32>,
    pub new_best: RwSignal<bool>,
}

#[derive(Clone, Copy)]
pub struct Particle {
    pub x: f64, pub y: f64,
    pub vx: f64, pub vy: f64,
    /// Index into the palette's hue buckets (0..HUE_BUCKETS)
    pub hue_bucket: u8,
    pub size: f64,
}

/// Top of the temperature gauge; the meltdown mark sits just below it
const GAUGE_MAX_TEMP: f64 = 1300.0;
/// Fissions per second that fill the Cherenkov glow
const FULL_POWER: f64 = 50.0;
/// Top of the output gauge
const GAUGE_MAX_MW: f64 = 700.0;

/// Percentage position of a value on a 0..max gauge
fn gauge_pct(v: f64, max: f64) -> f64 { (v / max).clamp(0.0, 1.0) * 100.0 }

fn clock_label(hour: f64) -> String {
    let mins = (hour * 60.0) as u32;
    format!("{:02}:{:02}", mins / 60 % 24, mins % 60)
}

#[component]
fn HomePage() -> impl IntoView {
    let canvas_ref = NodeRef::<Canvas>::new();
    let mode         = RwSignal::new(MODE_PLEXUS);
    let count        = RwSignal::new(300usize);
    let conn_dist    = RwSignal::new(115f64);
    let speed        = RwSignal::new(1.0f64);
    let trail        = RwSignal::new(0.62f64);
    let color_scheme = RwSignal::new(0u8);
    let is_repel     = RwSignal::new(false);
    let rods         = RwSignal::new(0.5f64);
    let fuel         = RwSignal::new(0.75f64);
    let pump         = RwSignal::new(DEFAULT_PUMP);
    let scram_seq    = RwSignal::new(0u32);
    let reset_seq    = RwSignal::new(0u32);
    let shift_seq    = RwSignal::new(0u32);
    let free_play_seq = RwSignal::new(0u32);
    let sound_on     = RwSignal::new(true);
    let unlocked     = RwSignal::new(false);
    let fps          = RwSignal::new(0u32);
    let stats        = RwSignal::new(ReactorStats::default());
    let shift        = RwSignal::new(None::<ShiftStats>);
    let best         = RwSignal::new(0u32);
    let new_best     = RwSignal::new(false);
    let outputs = SimOutputs { fps, stats, shift, best, new_best };

    let shared: Rc<RefCell<SimSettings>> = Rc::new(RefCell::new(SimSettings::default()));
    let pointer: Rc<RefCell<Pointer>> = Rc::new(RefCell::new(Pointer::default()));

    let s_sync = shared.clone();
    Effect::new(move |_| {
        *s_sync.borrow_mut() = SimSettings {
            mode: mode.get(),
            count: count.get(), conn_dist: conn_dist.get(),
            speed: speed.get(), color_scheme: color_scheme.get(),
            repel: is_repel.get(), trail: trail.get(),
            rods: rods.get(), fuel: fuel.get(), pump: pump.get(),
            scram_seq: scram_seq.get(), reset_seq: reset_seq.get(),
            shift_seq: shift_seq.get(), free_play_seq: free_play_seq.get(),
            sound_on: sound_on.get(), unlocked: unlocked.get(),
        };
    });

    let s_start = shared.clone();
    let p_start = pointer.clone();
    let started = Rc::new(Cell::new(false));
    Effect::new(move |_| {
        if !started.get() {
            started.set(try_start(canvas_ref, s_start.clone(), p_start.clone(), outputs));
        }
    });

    let p_move  = pointer.clone();
    let p_leave = pointer.clone();
    let p_click = pointer.clone();
    // Cursor offset from screen centre, -0.5..0.5, drives the starfield parallax
    let parallax = RwSignal::new((0.0f64, 0.0f64));
    let star_shift = move |depth: f64| {
        let (x, y) = parallax.get();
        format!("transform: translate3d({:.1}px, {:.1}px, 0)", -x * depth, -y * depth)
    };

    let is_reactor = move || mode.get() == MODE_REACTOR;
    let melting = move || is_reactor() && stats.with(|s| s.status == Status::Meltdown);
    let root_class = move || {
        let theme = if is_reactor() { "reactor" } else { ["neon", "fire", "ocean", "rainbow"][color_scheme.get() as usize] };
        format!("sim-root theme-{theme}{}", if melting() { " meltdown" } else { "" })
    };
    // Cherenkov glow brightness follows reactor power
    let root_style = move || {
        let power = if is_reactor() { stats.with(|s| s.power) / FULL_POWER } else { 0.0 };
        format!("--power: {:.2}", power.min(1.0))
    };
    let temp_frac = move || {
        ((stats.with(|s| s.temp) - AMBIENT_TEMP) / (GAUGE_MAX_TEMP - AMBIENT_TEMP)).clamp(0.0, 1.0)
    };
    let meltdown_mark = format!("left: {:.1}%", (MELTDOWN_TEMP - AMBIENT_TEMP) / (GAUGE_MAX_TEMP - AMBIENT_TEMP) * 100.0);

    view! {
        <div class=root_class style=root_style
            on:pointerdown=move |_| if !unlocked.get_untracked() { unlocked.set(true); }
        >
            <div class="bg-nebula"></div>
            <div class="bg-cherenkov"></div>
            <div class="bg-stars bg-stars-far" style=move || star_shift(8.0)></div>
            <div class="bg-stars bg-stars-near" style=move || star_shift(22.0)></div>
            <canvas node_ref=canvas_ref class="particle-canvas"
                on:mousemove=move |ev| {
                    let (x, y) = (ev.client_x() as f64, ev.client_y() as f64);
                    p_move.borrow_mut().pos = Some((x, y));
                    let win = window();
                    let w = win.inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(1.0);
                    let h = win.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(1.0);
                    parallax.set((x / w - 0.5, y / h - 0.5));
                }
                on:mouseleave=move |_| {
                    p_leave.borrow_mut().pos = None;
                    parallax.set((0.0, 0.0));
                }
                on:click=move |ev| {
                    p_click.borrow_mut().clicks.push((ev.client_x() as f64, ev.client_y() as f64));
                }
            />
            <div class="bg-vignette"></div>
            <div class="hud-bar">
                <h1 class="hud-title">"🦀 Particle Physics"</h1>
                <div class="hud-right">
                    <button class="sound-btn" class:hidden=move || !is_reactor()
                        title="Toggle sound"
                        on:click=move |_| sound_on.update(|on| *on = !*on)
                    >{move || if sound_on.get() { "🔊 Sound" } else { "🔇 Muted" }}</button>
                    <div class="fps-badge">{move || format!("{} fps", fps.get())}</div>
                </div>
            </div>

            <div class="reactor-hud" class:hidden=move || !is_reactor()>
                <div class="hud-top">
                    <div class=move || format!("status-chip {}", stats.with(|s| s.status.css()))>
                        {move || stats.with(|s| s.status.label())}
                    </div>
                    <div class="fault-tags">
                        <span class="fault-tag" class:hidden=move || !stats.with(|s| s.pump_failed)>"PUMP"</span>
                        <span class="fault-tag" class:hidden=move || !stats.with(|s| s.rod_stuck)>"ROD"</span>
                    </div>
                </div>
                <div class="gauge">
                    <div class="ctrl-label">
                        <span>"Core temp"</span>
                        <span class="ctrl-val">{move || format!("{:.0}°C", stats.with(|s| s.temp))}</span>
                    </div>
                    <div class="gauge-track">
                        <div class="gauge-fill gauge-temp" style=move || format!("width: {:.1}%", temp_frac() * 100.0)></div>
                        <div class="gauge-mark" style=meltdown_mark></div>
                    </div>
                </div>
                <div class="gauge">
                    <div class="ctrl-label">
                        <span>"Output"</span>
                        <span class="ctrl-val">{move || format!("{:.0} MW", stats.with(|s| s.output_mw))}</span>
                    </div>
                    <div class="gauge-track">
                        <div class="gauge-band" class:in-band=move || shift.with(|s| s.is_some_and(|s| s.in_band))
                            style=move || shift.with(|s| match s {
                                Some(s) if s.state == ShiftState::Running => format!(
                                    "left: {:.1}%; width: {:.1}%",
                                    gauge_pct(s.band_lo, GAUGE_MAX_MW),
                                    gauge_pct(s.band_hi, GAUGE_MAX_MW) - gauge_pct(s.band_lo, GAUGE_MAX_MW),
                                ),
                                _ => "display: none".to_string(),
                            })></div>
                        <div class="gauge-fill gauge-power"
                            style=move || format!("width: {:.1}%", gauge_pct(stats.with(|s| s.output_mw), GAUGE_MAX_MW))></div>
                    </div>
                </div>
                <div class="readouts">
                    <span>"n " <b>{move || stats.with(|s| s.neutrons)}</b></span>
                    <span>"Rods " <b>{move || format!("{:.0}%", stats.with(|s| s.rods) * 100.0)}</b></span>
                    <span title="Xenon-135 poison">"Xe " <b class:xe-high=move || stats.with(|s| s.xenon > 0.12)>
                        {move || format!("{:.0}%", stats.with(|s| s.xenon) * 100.0)}
                    </b></span>
                </div>

                <div class="shift-box">
                    {move || match shift.get() {
                        Some(s) if s.state == ShiftState::Running => view! {
                            <div class="shift-row">
                                <span class="shift-clock">{clock_label(s.clock)}</span>
                                <span class="shift-demand">"Demand " <b>{format!("{:.0} MW", s.demand_mw)}</b></span>
                            </div>
                            <div class="shift-progress"><div style=format!("width: {:.1}%", s.progress * 100.0)></div></div>
                            <div class="shift-row">
                                <span class="shift-score">{s.score}</span>
                                <span class="shift-mult" class:hot={s.multiplier > 1}>{format!("×{}", s.multiplier)}</span>
                                <span class={if s.in_band { "band-flag ok" } else { "band-flag miss" }}>
                                    {if s.in_band { "● ON TARGET" } else { "○ OFF TARGET" }}
                                </span>
                            </div>
                        }.into_any(),
                        _ => view! {
                            <button class="shift-start" on:click=move |_| shift_seq.update(|n| *n += 1)>"▶ Start shift"</button>
                            <div class="shift-best">{move || match best.get() { 0 => "Follow the grid's demand for a day".to_string(), b => format!("Best score {b}") }}</div>
                        }.into_any(),
                    }}
                </div>
            </div>

            // Incident / startup banner, top centre
            {move || {
                let s = shift.get().filter(|s| s.state == ShiftState::Running && is_reactor())?;
                Some(match s.incident {
                    Some(inc) => view! {
                        <div class="banner banner-alert">
                            <div class="banner-title">{format!("⚠ {}", inc.title())}<span class="banner-time">{format!("{:.0}s", s.incident_left.ceil())}</span></div>
                            <div class="banner-body">{inc.advice()}</div>
                        </div>
                    }.into_any(),
                    None if s.startup => view! {
                        <div class="banner">
                            <div class="banner-title">"06:00 · Startup"</div>
                            <div class="banner-body">"Withdraw the rods to bring output into the green band. Stay in it to build your multiplier."</div>
                        </div>
                    }.into_any(),
                    None => return None,
                })
            }}

            // End-of-shift report
            {move || {
                let s = shift.get().filter(|s| s.state != ShiftState::Running && is_reactor())?;
                let failed = s.state == ShiftState::Failed;
                Some(view! {
                    <div class="shift-card-wrap">
                        <div class="shift-card" class:failed=failed>
                            <div class="card-title">{if failed { "☢ Shift failed: core meltdown" } else { "Shift complete" }}</div>
                            <div class="card-stars">{(0..3).map(|i| if i < s.stars { "★" } else { "☆" }).collect::<String>()}</div>
                            <div class="card-score">{s.score}</div>
                            <div class="card-best">{move || if new_best.get() { "New best score!".to_string() } else { format!("Best {}", best.get()) }}</div>
                            <div class="card-stat">{format!("On target {:.0}% of the shift", s.in_band_frac * 100.0)}</div>
                            {failed.then(|| view! { <div class="card-stat">"Score halved for the meltdown"</div> })}
                            <div class="card-actions">
                                <button class="shift-start" on:click=move |_| shift_seq.update(|n| *n += 1)>"▶ New shift"</button>
                                <button class="card-free" on:click=move |_| free_play_seq.update(|n| *n += 1)>"Free play"</button>
                            </div>
                        </div>
                    </div>
                })
            }}

            <div class="ctrl-panel">
                <div class="mode-switch">
                    <button class="mode-tab" class:active=move || !is_reactor()
                        on:click=move |_| mode.set(MODE_PLEXUS)>"✦ Plexus"</button>
                    <button class="mode-tab mode-tab-reactor" class:active=is_reactor
                        on:click=move |_| mode.set(MODE_REACTOR)>"☢ Reactor"</button>
                </div>
                <div class:hidden=is_reactor>
                    <div class="ctrl-row">
                        <div class="ctrl-label">
                            <span>"Particles"</span>
                            <span class="ctrl-val">{move || count.get()}</span>
                        </div>
                        <input class="slider" type="range" min="40" max="2500" step="10" value="300"
                            on:input=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<usize>() { count.set(v); }
                            }
                        />
                    </div>
                    <div class="ctrl-row">
                        <div class="ctrl-label">
                            <span>"Link Range"</span>
                            <span class="ctrl-val">{move || format!("{}px", conn_dist.get() as u32)}</span>
                        </div>
                        <input class="slider" type="range" min="40" max="220" step="5" value="115"
                            on:input=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<f64>() { conn_dist.set(v); }
                            }
                        />
                    </div>
                </div>
                <div class:hidden=move || !is_reactor()>
                    <div class="ctrl-row">
                        <div class="ctrl-label">
                            <span>"Control Rods"</span>
                            <span class="ctrl-val">{move || format!("{:.0}% in", rods.get() * 100.0)}</span>
                        </div>
                        <input class="slider slider-rods" type="range" min="0" max="100" step="1" value="50"
                            on:input=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<f64>() { rods.set(v / 100.0); }
                            }
                        />
                    </div>
                    <div class="ctrl-row">
                        <div class="ctrl-label">
                            <span>"Fuel"</span>
                            <span class="ctrl-val">{move || format!("{:.0}%", fuel.get() * 100.0)}</span>
                        </div>
                        <input class="slider" type="range" min="20" max="100" step="1" value="75"
                            on:input=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<f64>() { fuel.set(v / 100.0); }
                            }
                        />
                    </div>
                    <div class="ctrl-row">
                        <div class="ctrl-label">
                            <span>"Coolant Pump"</span>
                            <span class="ctrl-val">{move || format!("{:.0}%", pump.get() * 100.0)}</span>
                        </div>
                        <input class="slider slider-pump" type="range" min="0" max="100" step="1" value="60"
                            on:input=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<f64>() { pump.set(v / 100.0); }
                            }
                        />
                    </div>
                </div>
                <div class="ctrl-row">
                    <div class="ctrl-label">
                        <span>"Speed"</span>
                        <span class="ctrl-val">{move || format!("{:.1}x", speed.get())}</span>
                    </div>
                    <input class="slider" type="range" min="1" max="30" step="1" value="10"
                        on:input=move |ev| {
                            if let Ok(v) = event_target_value(&ev).parse::<f64>() { speed.set(v / 10.0); }
                        }
                    />
                </div>
                <div class="ctrl-row">
                    <div class="ctrl-label">
                        <span>"Trails"</span>
                        <span class="ctrl-val">{move || format!("{}%", (trail.get() * 100.0).round() as u32)}</span>
                    </div>
                    <input class="slider" type="range" min="0" max="100" step="1" value="62"
                        on:input=move |ev| {
                            if let Ok(v) = event_target_value(&ev).parse::<f64>() { trail.set(v / 100.0); }
                        }
                    />
                </div>
                <div class:hidden=is_reactor>
                    <div class="ctrl-row">
                        <span class="ctrl-label-single">"Palette"</span>
                        <div class="palette-row">
                            <button class="pal-btn pal-neon"    class:active=move || color_scheme.get() == 0 on:click=move |_| color_scheme.set(0)>"◈ Neon"</button>
                            <button class="pal-btn pal-fire"    class:active=move || color_scheme.get() == 1 on:click=move |_| color_scheme.set(1)>"🔥 Fire"</button>
                            <button class="pal-btn pal-ocean"   class:active=move || color_scheme.get() == 2 on:click=move |_| color_scheme.set(2)>"🌊 Ocean"</button>
                            <button class="pal-btn pal-rainbow" class:active=move || color_scheme.get() == 3 on:click=move |_| color_scheme.set(3)>"🌈 Rnbw"</button>
                        </div>
                    </div>
                    <div class="ctrl-row">
                        <button class="mode-btn" class:repel=move || is_repel.get()
                            on:click=move |_| is_repel.update(|r| *r = !*r)
                        >{move || if is_repel.get() { "⊖ Repel" } else { "⊕ Attract" }}</button>
                    </div>
                </div>
                <div class="ctrl-row" class:hidden=move || !is_reactor()>
                    <button class="scram-btn" class:armed=move || stats.with(|s| s.scrammed)
                        prop:disabled=melting
                        on:click=move |_| {
                            if stats.get_untracked().scrammed { reset_seq.update(|n| *n += 1); }
                            else { scram_seq.update(|n| *n += 1); }
                        }
                    >{move || {
                        if melting() { "☢ Cooling down…" }
                        else if stats.with(|s| s.scrammed) { "↺ Reset SCRAM" }
                        else { "⚠ SCRAM" }
                    }}</button>
                </div>
            </div>
            {move || if is_reactor() {
                view! { <div class="hint-pill">"☢ Click to fire neutrons · rods slow the reaction · start a shift to play"</div> }.into_any()
            } else {
                view! { <div class="hint-pill">"🖱 Move your mouse to interact"</div> }.into_any()
            }}
        </div>
    }
}

// ── Entry point: starts animation only when canvas is available (hydrate) ─
#[cfg(feature = "hydrate")]
fn try_start(
    canvas_ref: NodeRef<Canvas>,
    shared: Rc<RefCell<SimSettings>>,
    pointer: Rc<RefCell<Pointer>>,
    outputs: SimOutputs,
) -> bool {
    if let Some(el) = canvas_ref.get() {
        use web_sys::HtmlCanvasElement;
        let canvas: &HtmlCanvasElement = el.as_ref();
        start_simulation(canvas.clone(), shared, pointer, outputs);
        return true;
    }
    false
}

#[cfg(not(feature = "hydrate"))]
fn try_start(
    _canvas_ref: NodeRef<Canvas>,
    _shared: Rc<RefCell<SimSettings>>,
    _pointer: Rc<RefCell<Pointer>>,
    _outputs: SimOutputs,
) -> bool { false }

// ── Best shift score, kept in this browser only ───────────────────────────
#[cfg(feature = "hydrate")]
const BEST_KEY: &str = "reactor-best-score";

#[cfg(feature = "hydrate")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

#[cfg(feature = "hydrate")]
fn load_best() -> u32 {
    local_storage()
        .and_then(|s| s.get_item(BEST_KEY).ok().flatten())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

#[cfg(feature = "hydrate")]
fn save_best(score: u32) {
    if let Some(s) = local_storage() { let _ = s.set_item(BEST_KEY, &score.to_string()); }
}

// ── Palette buckets ───────────────────────────────────────────────────────
// Colours are quantized so sprites and line styles can be built once per
// palette instead of formatted per particle / per line every frame.
#[cfg(feature = "hydrate")]
const HUE_BUCKETS: usize = 12;
#[cfg(feature = "hydrate")]
const ALPHA_BUCKETS: usize = 6;
#[cfg(feature = "hydrate")]
const LINE_ALPHA: f64 = 0.4;
#[cfg(feature = "hydrate")]
const SPRITE_PX: u32 = 64;
#[cfg(feature = "hydrate")]
const MOUSE_RADIUS: f64 = 180.0;
/// Neutrons fired per click in reactor mode
#[cfg(feature = "hydrate")]
const CLICK_NEUTRONS: usize = 12;

/// (start hue, hue span) for each palette
#[cfg(feature = "hydrate")]
fn hue_range(scheme: u8) -> (f64, f64) {
    match scheme {
        0 => (180.0, 120.0),
        1 => (0.0, 60.0),
        2 => (170.0, 70.0),
        _ => (0.0, 360.0),
    }
}

#[cfg(feature = "hydrate")]
fn bucket_hue(scheme: u8, bucket: usize) -> f64 {
    let (start, span) = hue_range(scheme);
    start + span * (bucket as f64 + 0.5) / HUE_BUCKETS as f64
}

#[cfg(feature = "hydrate")]
fn line_color(hue: f64, alpha: f64) -> String {
    format!("hsla({:.0}, 80%, 65%, {:.3})", hue, alpha)
}

/// Stroke styles indexed by `hue_bucket * ALPHA_BUCKETS + alpha_bucket`
#[cfg(feature = "hydrate")]
fn line_styles(scheme: u8) -> Vec<String> {
    let mut styles = Vec::with_capacity(HUE_BUCKETS * ALPHA_BUCKETS);
    for hb in 0..HUE_BUCKETS {
        for ab in 0..ALPHA_BUCKETS {
            let alpha = (ab as f64 + 0.5) / ALPHA_BUCKETS as f64 * LINE_ALPHA;
            styles.push(line_color(bucket_hue(scheme, hb), alpha));
        }
    }
    styles
}

// ── WASM simulation (hydrate only) ────────────────────────────────────────
/// Pre-rendered radial-gradient glow (replaces shadowBlur)
#[cfg(feature = "hydrate")]
fn glow_sprite(hue: f64) -> web_sys::HtmlCanvasElement {
    use wasm_bindgen::JsCast;
    use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

    let doc = web_sys::window().unwrap().document().unwrap();
    let r = SPRITE_PX as f64 / 2.0;
    let sprite: HtmlCanvasElement = doc.create_element("canvas").unwrap().unchecked_into();
    sprite.set_width(SPRITE_PX);
    sprite.set_height(SPRITE_PX);
    let sctx: CanvasRenderingContext2d =
        sprite.get_context("2d").unwrap().unwrap().unchecked_into();
    let grad = sctx.create_radial_gradient(r, r, 0.0, r, r, r).unwrap();
    let _ = grad.add_color_stop(0.0,  &format!("hsla({:.0}, 100%, 92%, 1)", hue));
    let _ = grad.add_color_stop(0.16, &format!("hsla({:.0}, 100%, 70%, 0.95)", hue));
    let _ = grad.add_color_stop(0.3,  &format!("hsla({:.0}, 100%, 60%, 0.35)", hue));
    let _ = grad.add_color_stop(1.0,  &format!("hsla({:.0}, 100%, 50%, 0)", hue));
    sctx.set_fill_style_canvas_gradient(&grad);
    sctx.fill_rect(0.0, 0.0, SPRITE_PX as f64, SPRITE_PX as f64);
    sprite
}

/// One glow sprite per hue bucket of a palette
#[cfg(feature = "hydrate")]
fn build_sprites(scheme: u8) -> Vec<web_sys::HtmlCanvasElement> {
    (0..HUE_BUCKETS).map(|b| glow_sprite(bucket_hue(scheme, b))).collect()
}

#[cfg(feature = "hydrate")]
fn start_simulation(
    canvas: web_sys::HtmlCanvasElement,
    shared: Rc<RefCell<SimSettings>>,
    pointer: Rc<RefCell<Pointer>>,
    outputs: SimOutputs,
) {
    use crate::audio::Sound;
    use crate::grid::Grid;
    use crate::reactor::Reactor;
    use crate::shift::Shift;
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;
    use web_sys::CanvasRenderingContext2d;

    let win = web_sys::window().unwrap();
    let w = win.inner_width().unwrap().as_f64().unwrap_or(1920.0);
    let h = win.inner_height().unwrap().as_f64().unwrap_or(1080.0);

    let ctx: CanvasRenderingContext2d =
        canvas.get_context("2d").unwrap().unwrap().unchecked_into();

    let init_settings = shared.borrow().clone();
    let mut particles: Vec<Particle> =
        (0..init_settings.count).map(|_| new_particle(w, h)).collect();

    let mut last_time: f64 = 0.0;
    let mut frames: u32 = 0;
    let mut last_fps: f64 = 0.0;
    let mut last_scheme = init_settings.color_scheme;
    let mut sprites = build_sprites(last_scheme);
    let mut styles = line_styles(last_scheme);
    // CSS size + DPR the backing store was last sized for (0 forces a resize)
    let mut sized_for = (0.0f64, 0.0f64, 0.0f64);

    // Reused every frame: spatial grid and line batches
    let mut grid = Grid::default();
    let mut segments: Vec<Vec<(f64, f64, f64, f64)>> =
        vec![Vec::new(); HUE_BUCKETS * ALPHA_BUCKETS];

    // Reactor mode: created on entry, dropped on exit so each visit starts cold
    let mut reactor: Option<Reactor> = None;
    let hot_glow = glow_sprite(30.0);
    let mut last_stats: f64 = 0.0;
    let mut seen_scram = init_settings.scram_seq;
    let mut seen_reset = init_settings.reset_seq;
    let mut seen_shift = init_settings.shift_seq;
    let mut seen_free_play = init_settings.free_play_seq;
    let mut shift: Option<Shift> = None;
    let mut best = load_best();
    outputs.best.set(best);
    // Created after the first pointer press; browsers block audio before that
    let mut sound: Option<Sound> = None;
    let mut sound_tried = false;
    let mut sound_live = false;
    let seed = || (js_sys::Math::random() * u64::MAX as f64) as u64;

    let shared_c  = shared.clone();
    let pointer_c = pointer.clone();
    let canvas_c  = canvas.clone();

    let closure: Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>> =
        Rc::new(RefCell::new(None));
    let closure_ref = closure.clone();

    *closure.borrow_mut() = Some(Closure::wrap(Box::new(move |time: f64| {
        let settings = shared_c.borrow().clone();
        let (mouse_data, clicks) = {
            let mut p = pointer_c.borrow_mut();
            (p.pos, std::mem::take(&mut p.clicks))
        };

        // ── Size the backing store for HiDPI; physics stays in CSS pixels ──
        let cw = win.inner_width().unwrap().as_f64().unwrap_or(1920.0);
        let ch = win.inner_height().unwrap().as_f64().unwrap_or(1080.0);
        let dpr = win.device_pixel_ratio().clamp(1.0, 2.0);
        if sized_for != (cw, ch, dpr) {
            canvas_c.set_width((cw * dpr) as u32);
            canvas_c.set_height((ch * dpr) as u32);
            let _ = ctx.set_transform(dpr, 0.0, 0.0, dpr, 0.0, 0.0);
            sized_for = (cw, ch, dpr);
        }

        // FPS
        frames += 1;
        if time - last_fps > 1000.0 {
            outputs.fps.set(frames);
            frames = 0;
            last_fps = time;
        }

        let dt = if last_time > 0.0 { ((time - last_time) / 16.667).min(3.0) } else { 1.0 };
        last_time = time;

        // ── Fade toward transparent so the CSS background shows through ──
        // Min fade 0.08: lower values leave 8-bit alpha residue (ghost trails)
        let _ = ctx.set_global_composite_operation("destination-out");
        let fade = 0.5 - settings.trail * 0.42;
        ctx.set_fill_style_str(&format!("rgba(0,0,0,{:.3})", fade));
        ctx.fill_rect(0.0, 0.0, cw, ch);

        if settings.mode == MODE_REACTOR {
            // A new shift always starts from a cold core
            if settings.shift_seq != seen_shift {
                seen_shift = settings.shift_seq;
                reactor = None;
                shift = Some(Shift::new(seed()));
                outputs.new_best.set(false);
            }
            if settings.free_play_seq != seen_free_play {
                seen_free_play = settings.free_play_seq;
                shift = None;
            }
            let r = reactor.get_or_insert_with(|| Reactor::new(cw, ch, settings.fuel, seed()));
            r.resize(cw, ch);
            r.set_fuel(settings.fuel);
            r.pump = settings.pump;
            if settings.scram_seq != seen_scram { seen_scram = settings.scram_seq; r.scram(); }
            if settings.reset_seq != seen_reset { seen_reset = settings.reset_seq; r.reset_scram(); }
            for (x, y) in clicks { r.inject(x, y, CLICK_NEUTRONS); }
            let sim_dt = dt * settings.speed;
            r.step(sim_dt, settings.rods);

            if let Some(s) = shift.as_mut() {
                let was_running = s.state() == ShiftState::Running;
                s.step(sim_dt / 60.0, r);
                if was_running && s.state() != ShiftState::Running {
                    let score = s.stats().score;
                    if score > best {
                        best = score;
                        save_best(best);
                        outputs.best.set(best);
                        outputs.new_best.set(true);
                    }
                }
            }
            draw_reactor(&ctx, r, &hot_glow);

            let live = settings.sound_on && settings.unlocked;
            if live && !sound_tried { sound_tried = true; sound = Sound::new(); }
            if let Some(snd) = sound.as_mut() {
                if live != sound_live { snd.set_enabled(live); }
                if live {
                    let incident = shift.as_ref().is_some_and(|s| s.stats().incident.is_some());
                    snd.update(dt / 60.0, &r.stats(), incident);
                }
            }
            sound_live = live;

            if time - last_stats > 100.0 {
                outputs.stats.set(r.stats());
                outputs.shift.set(shift.as_ref().map(|s| s.stats()));
                last_stats = time;
            }
        } else {
            reactor = None;
            if shift.take().is_some() { outputs.shift.set(None); }
            if sound_live {
                if let Some(snd) = sound.as_ref() { snd.set_enabled(false); }
                sound_live = false;
            }
            step_plexus(&mut particles, &settings, mouse_data, dt, cw, ch);

            // Rebuild colour caches and reshuffle hues when the palette changes
            if settings.color_scheme != last_scheme {
                last_scheme = settings.color_scheme;
                sprites = build_sprites(last_scheme);
                styles = line_styles(last_scheme);
                for p in particles.iter_mut() { p.hue_bucket = random_bucket(); }
            }

            let _ = ctx.set_global_composite_operation("lighter");
            if let Some((mx, my)) = mouse_data { draw_cursor_ring(&ctx, mx, my, settings.repel, time); }

            // ── Collect plexus lines into colour batches ──
            for batch in segments.iter_mut() { batch.clear(); }
            let cell = settings.conn_dist;
            let cd2 = cell * cell;
            grid.rebuild(cell, cw, ch, particles.iter().map(|p| (p.x, p.y)));
            grid.for_each_pair(|i, j| {
                let (a, b) = (&particles[i], &particles[j]);
                let dx = a.x - b.x;
                let dy = a.y - b.y;
                let d2 = dx * dx + dy * dy;
                if d2 < cd2 {
                    let ab = (((1.0 - d2 / cd2) * ALPHA_BUCKETS as f64) as usize).min(ALPHA_BUCKETS - 1);
                    let hb = (a.hue_bucket as usize + b.hue_bucket as usize) / 2;
                    segments[hb * ALPHA_BUCKETS + ab].push((a.x, a.y, b.x, b.y));
                }
            });

            // ── Draw plexus lines, one stroke per colour batch ──
            ctx.set_line_width(0.6);
            for (batch, style) in segments.iter().zip(styles.iter()) {
                if batch.is_empty() { continue; }
                ctx.set_stroke_style_str(style);
                ctx.begin_path();
                for &(x1, y1, x2, y2) in batch {
                    ctx.move_to(x1, y1);
                    ctx.line_to(x2, y2);
                }
                ctx.stroke();
            }

            // ── Draw particles from glow sprites ──
            for p in particles.iter() {
                let s = p.size * 6.0;
                let _ = ctx.draw_image_with_html_canvas_element_and_dw_and_dh(
                    &sprites[p.hue_bucket as usize], p.x - s * 0.5, p.y - s * 0.5, s, s,
                );
            }
        }

        // Request next frame
        if let Some(c) = closure_ref.borrow().as_ref() {
            let _ = win.request_animation_frame(c.as_ref().unchecked_ref());
        }
    }) as Box<dyn FnMut(f64)>));

    let _ = web_sys::window().unwrap().request_animation_frame(
        closure.borrow().as_ref().unwrap().as_ref().unchecked_ref()
    );
}

/// Plexus physics: mouse attract/repel, damping, jitter and screen wrap
#[cfg(feature = "hydrate")]
fn step_plexus(
    particles: &mut Vec<Particle>,
    settings: &SimSettings,
    mouse: Option<(f64, f64)>,
    dt: f64, cw: f64, ch: f64,
) {
    while particles.len() < settings.count {
        particles.push(new_particle(cw, ch));
    }
    particles.truncate(settings.count);

    for p in particles.iter_mut() {
        if let Some((mx, my)) = mouse {
            let dx = mx - p.x;
            let dy = my - p.y;
            let d2 = dx * dx + dy * dy;
            if d2 < MOUSE_RADIUS * MOUSE_RADIUS && d2 > 4.0 {
                let d = d2.sqrt();
                let f = 600.0 / d2;
                let sign = if settings.repel { -1.0 } else { 1.0 };
                p.vx += sign * (dx / d) * f;
                p.vy += sign * (dy / d) * f;
            }
        }
        p.vx *= 0.975;
        p.vy *= 0.975;
        let spd = (p.vx * p.vx + p.vy * p.vy).sqrt();
        let max = 4.5 * settings.speed;
        if spd > max { p.vx = (p.vx / spd) * max; p.vy = (p.vy / spd) * max; }
        p.vx += (js_sys::Math::random() - 0.5) * 0.28 * settings.speed;
        p.vy += (js_sys::Math::random() - 0.5) * 0.28 * settings.speed;
        p.x += p.vx * dt;
        p.y += p.vy * dt;
        if p.x < 0.0  { p.x += cw; } if p.x > cw { p.x -= cw; }
        if p.y < 0.0  { p.y += ch; } if p.y > ch { p.y -= ch; }
    }
}

/// Soft halo plus a slowly pulsing ring marking the mouse's reach
#[cfg(feature = "hydrate")]
fn draw_cursor_ring(ctx: &web_sys::CanvasRenderingContext2d, mx: f64, my: f64, repel: bool, time: f64) {
    let (r, g, b) = if repel { (0, 212, 255) } else { (255, 107, 53) };
    if let Ok(grad) = ctx.create_radial_gradient(mx, my, 0.0, mx, my, MOUSE_RADIUS) {
        let _ = grad.add_color_stop(0.0, &format!("rgba({r},{g},{b},0.10)"));
        let _ = grad.add_color_stop(1.0, &format!("rgba({r},{g},{b},0)"));
        ctx.set_fill_style_canvas_gradient(&grad);
        ctx.fill_rect(mx - MOUSE_RADIUS, my - MOUSE_RADIUS, MOUSE_RADIUS * 2.0, MOUSE_RADIUS * 2.0);
    }
    let pulse = 0.5 + 0.5 * (time / 400.0).sin();
    ctx.set_line_width(1.0);
    ctx.set_stroke_style_str(&format!("rgba({r},{g},{b},{:.3})", 0.12 + 0.12 * pulse));
    ctx.begin_path();
    let _ = ctx.arc(mx, my, MOUSE_RADIUS * (0.94 + 0.06 * pulse), 0.0, std::f64::consts::TAU);
    ctx.stroke();
}

/// Rods, fuel, spent nuclei, fission glow and shock rings, then neutron streaks
#[cfg(feature = "hydrate")]
fn draw_reactor(
    ctx: &web_sys::CanvasRenderingContext2d,
    r: &crate::reactor::Reactor,
    hot_glow: &web_sys::HtmlCanvasElement,
) {
    use crate::reactor::{FLASH_SECS, NUCLEUS_R, ROD_WIDTH};
    use std::f64::consts::TAU;

    // Control rods: steel bodies with a boron-yellow tip, red once SCRAMmed
    let _ = ctx.set_global_composite_operation("source-over");
    let rods = r.rods();
    let half = ROD_WIDTH / 2.0;
    for (rx, bottom, stuck) in rods.iter() {
        // A jammed rod is drawn in warning orange
        ctx.set_fill_style_str(if stuck { "rgba(255,130,40,0.85)" } else { "rgba(140,155,180,0.8)" });
        ctx.fill_rect(rx - half, 0.0, ROD_WIDTH, bottom);
        ctx.set_fill_style_str(if r.scrammed { "rgba(255,70,50,0.95)" } else { "rgba(255,200,60,0.95)" });
        ctx.fill_rect(rx - half, bottom - 5.0, ROD_WIDTH, 5.0);
    }

    // Fissile fuel glows uranium-green; spent nuclei are dim grey until refuelled
    for (spent, style) in [(false, "hsla(95, 85%, 55%, 0.9)"), (true, "rgba(120,130,150,0.55)")] {
        ctx.set_fill_style_str(style);
        ctx.begin_path();
        for n in r.nuclei.iter().filter(|n| (n.spent > 0.0) == spent) {
            ctx.move_to(n.x + NUCLEUS_R, n.y);
            let _ = ctx.arc(n.x, n.y, NUCLEUS_R, 0.0, TAU);
        }
        ctx.fill();
    }

    // Heat glow on freshly split nuclei and the neighbours they knock into
    let _ = ctx.set_global_composite_operation("lighter");
    for n in r.nuclei.iter().filter(|n| n.flash > 0.02) {
        let s = NUCLEUS_R * (3.0 + 5.0 * n.flash);
        ctx.set_global_alpha(n.flash);
        let _ = ctx.draw_image_with_html_canvas_element_and_dw_and_dh(hot_glow, n.x - s * 0.5, n.y - s * 0.5, s, s);
    }
    ctx.set_global_alpha(1.0);

    // Shock rings expanding from each fission
    ctx.set_line_width(1.5);
    for f in r.flashes.iter() {
        let t = f.age / FLASH_SECS;
        ctx.set_stroke_style_str(&format!("rgba(255,190,110,{:.3})", (1.0 - t) * 0.7));
        ctx.begin_path();
        let _ = ctx.arc(f.x, f.y, NUCLEUS_R + t * 55.0, 0.0, TAU);
        ctx.stroke();
    }

    // Neutrons: Cherenkov-blue streaks from where they were last frame
    ctx.set_line_cap("round");
    ctx.set_line_width(1.8);
    ctx.set_stroke_style_str("rgba(140,215,255,0.95)");
    ctx.begin_path();
    for n in r.neutrons.iter() {
        ctx.move_to(n.px, n.py);
        ctx.line_to(n.x, n.y);
    }
    ctx.stroke();
    ctx.set_line_cap("butt");
}

#[cfg(feature = "hydrate")]
fn random_bucket() -> u8 {
    ((js_sys::Math::random() * HUE_BUCKETS as f64) as usize).min(HUE_BUCKETS - 1) as u8
}

#[cfg(feature = "hydrate")]
fn new_particle(w: f64, h: f64) -> Particle {
    let rnd = js_sys::Math::random;
    Particle {
        x: rnd() * w, y: rnd() * h,
        vx: (rnd() - 0.5) * 1.8, vy: (rnd() - 0.5) * 1.8,
        hue_bucket: random_bucket(), size: 1.5 + rnd() * 2.5,
    }
}
