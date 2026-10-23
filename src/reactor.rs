//! Reactor mode: a toy fission chain reaction.
//!
//! Fuel nuclei jostle and bounce off each other. A neutron that strikes fuel may
//! split it: the nucleus is kicked away glowing hot, turns spent until it
//! refuels, and 2–3 new neutrons fly out. Neutrons that miss are slowed by each
//! bounce (moderation), which makes them likelier to cause the next fission.
//! Control rods hanging from the top absorb neutrons, so lowering them slows the
//! reaction; SCRAM drops them all the way at once.
//!
//! Plain Rust with no web APIs so the physics can be unit tested natively.
//! Units are CSS pixels and 60fps frames (dt = 1.0 per 16.7ms), like the plexus.

use crate::grid::Grid;

pub const NUCLEUS_R: f64 = 7.0;
const NEUTRON_SPEED: f64 = 3.5;
const NEUTRON_LIFE: f64 = 3.0;
/// Fission chance per hit for fast vs slowed ("thermal") neutrons
const FISSION_P_FAST: f64 = 0.5;
const FISSION_P_SLOW: f64 = 0.95;
const SLOW_SPEED: f64 = 2.5;
/// Fraction of speed a neutron keeps after bouncing off a nucleus
const SCATTER_KEEP: f64 = 0.75;
const MIN_NEUTRON_SPEED: f64 = 1.2;
/// Chance a spent nucleus swallows a neutron instead of scattering it
const POISON_P: f64 = 0.15;
const FRAGMENT_KICK: f64 = 3.5;
const REFUEL_SECS: (f64, f64) = (4.0, 9.0);
const COOLANT_DRAG: f64 = 0.985;
const ROD_SPACING: f64 = 150.0;
pub const ROD_WIDTH: f64 = 10.0;
/// Rod travel per second as a fraction of screen height
const ROD_SPEED: f64 = 0.2;
const SCRAM_SPEED: f64 = 2.5;
const MAX_NEUTRONS: usize = 3000;
/// Spontaneous-fission neutrons per second, so a shut-down core can restart
const SOURCE_RATE: f64 = 2.0;
/// Fuel nuclei per square pixel at 100% fuel
const FUEL_DENSITY: f64 = 1.0 / 2600.0;
pub const AMBIENT_TEMP: f64 = 290.0;
const HEAT_PER_FISSION: f64 = 9.0;
/// Newton cooling rate per second: a floor from natural circulation plus the pump
const COOLING_BASE: f64 = 0.15;
const COOLING_PUMP: f64 = 0.6;
pub const DEFAULT_PUMP: f64 = 0.6;
/// Electrical output per fission/s, and the share of it a full-speed pump uses
pub const MW_PER_FISSION: f64 = 20.0;
const PUMP_COST: f64 = 0.12;
// Xenon-135 poisoning, time-compressed: fission makes iodine, iodine decays to
// xenon, xenon soaks up neutrons and is burned off by neutron flux. Cutting
// power stops the burn-off, so xenon piles up and the core goes sluggish.
const IODINE_YIELD: f64 = 0.0025;
const IODINE_DECAY: f64 = 1.0 / 8.0;
const XENON_DECAY: f64 = 1.0 / 30.0;
const XENON_BURN: f64 = 0.5;
/// Neutron count treated as "full flux" for xenon burn-off
const FLUX_REF: f64 = 30.0;
/// Neutron absorption rate per second at xenon level 1.0
const XENON_ABSORB: f64 = 0.7;
pub const MELTDOWN_TEMP: f64 = 1200.0;
const MELTDOWN_CLEAR_TEMP: f64 = 700.0;
pub const FLASH_SECS: f64 = 0.45;

/// Small xorshift64* generator, so the physics needs no JS and tests are repeatable
pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Self { Self(seed | 1) }
    pub fn f64(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[derive(Clone, Copy)]
pub struct Nucleus {
    pub x: f64, pub y: f64,
    pub vx: f64, pub vy: f64,
    /// Seconds until refuelled; 0.0 means it is fissile fuel
    pub spent: f64,
    /// Fission glow, 1.0 just after splitting, fading to 0.0
    pub flash: f64,
}

#[derive(Clone, Copy)]
pub struct Neutron {
    pub x: f64, pub y: f64,
    pub vx: f64, pub vy: f64,
    /// Position at the start of the frame, for drawing a streak
    pub px: f64, pub py: f64,
    pub life: f64,
}

/// Expanding shock ring left by a fission
#[derive(Clone, Copy)]
pub struct Flash { pub x: f64, pub y: f64, pub age: f64 }

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Status {
    #[default]
    Shutdown,
    Subcritical,
    Critical,
    Supercritical,
    Meltdown,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Shutdown => "SHUTDOWN",
            Status::Subcritical => "SUBCRITICAL",
            Status::Critical => "CRITICAL",
            Status::Supercritical => "SUPERCRITICAL",
            Status::Meltdown => "MELTDOWN",
        }
    }
    pub fn css(self) -> &'static str {
        match self {
            Status::Shutdown => "st-shutdown",
            Status::Subcritical => "st-sub",
            Status::Critical => "st-critical",
            Status::Supercritical => "st-super",
            Status::Meltdown => "st-meltdown",
        }
    }
}

/// What the control room shows; sent to the UI a few times a second
#[derive(Clone, Copy, PartialEq, Default)]
pub struct ReactorStats {
    /// Fissions per second, smoothed
    pub power: f64,
    pub neutrons: usize,
    pub temp: f64,
    /// Actual rod insertion, 0.0 (withdrawn) to 1.0 (fully in)
    pub rods: f64,
    pub status: Status,
    pub scrammed: bool,
    /// Net electrical output after the coolant pump's share
    pub output_mw: f64,
    pub xenon: f64,
    pub pump_failed: bool,
    pub rod_stuck: bool,
}

/// Control rods: evenly spaced, hanging from the top of the screen
pub struct Rods {
    pub count: usize,
    pub spacing: f64,
    pub bottom: f64,
    /// A jammed rod: (index, its own bottom y)
    pub stuck: Option<(usize, f64)>,
}

impl Rods {
    /// (x, bottom y, is stuck) for each rod
    pub fn iter(&self) -> impl Iterator<Item = (f64, f64, bool)> + '_ {
        (0..self.count).map(move |i| {
            let stuck = self.stuck.filter(|&(s, _)| s == i);
            ((i as f64 + 0.5) * self.spacing, stuck.map_or(self.bottom, |(_, b)| b), stuck.is_some())
        })
    }
    /// The x of the rod touching a circle of radius `pad` at (x, y), if any
    fn hit(&self, x: f64, y: f64, pad: f64) -> Option<f64> {
        let i = ((x / self.spacing) as usize).min(self.count - 1);
        let bottom = match self.stuck {
            Some((s, b)) if s == i => b,
            _ => self.bottom,
        };
        if y - pad > bottom { return None; }
        let rx = (i as f64 + 0.5) * self.spacing;
        ((x - rx).abs() < ROD_WIDTH / 2.0 + pad).then_some(rx)
    }
}

pub struct Reactor {
    pub nuclei: Vec<Nucleus>,
    pub neutrons: Vec<Neutron>,
    pub flashes: Vec<Flash>,
    pub rod_pos: f64,
    pub temp: f64,
    pub scrammed: bool,
    pub meltdown: bool,
    /// Coolant pump speed set by the operator, 0.0..1.0
    pub pump: f64,
    /// Scales cooling; below 1.0 while a pump has failed
    pub cooling_factor: f64,
    /// A jammed rod: (index, insertion it is stuck at)
    pub stuck_rod: Option<(usize, f64)>,
    /// Xenon-135 poison level, roughly 0.0..1.0
    pub xenon: f64,
    iodine: f64,
    w: f64,
    h: f64,
    power: f64,
    growth: f64,
    prev_neutrons: f64,
    rng: Rng,
    grid: Grid,
    born: Vec<Neutron>,
}

impl Reactor {
    /// A cold core: rods fully inserted, so it comes alive as they withdraw.
    pub fn new(w: f64, h: f64, fuel: f64, seed: u64) -> Self {
        let mut r = Self {
            nuclei: Vec::new(), neutrons: Vec::new(), flashes: Vec::new(),
            rod_pos: 1.0, temp: AMBIENT_TEMP, scrammed: false, meltdown: false,
            pump: DEFAULT_PUMP, cooling_factor: 1.0, stuck_rod: None, xenon: 0.0, iodine: 0.0,
            w, h, power: 0.0, growth: 0.0, prev_neutrons: 0.0,
            rng: Rng::new(seed), grid: Grid::default(), born: Vec::new(),
        };
        r.set_fuel(fuel);
        r
    }

    pub fn resize(&mut self, w: f64, h: f64) {
        if (w, h) == (self.w, self.h) { return; }
        self.w = w;
        self.h = h;
        for n in self.nuclei.iter_mut() {
            n.x = n.x.clamp(NUCLEUS_R, (w - NUCLEUS_R).max(NUCLEUS_R));
            n.y = n.y.clamp(NUCLEUS_R, (h - NUCLEUS_R).max(NUCLEUS_R));
        }
    }

    /// `fuel` is 0.0..1.0 of full loading; nucleus count scales with screen area.
    pub fn set_fuel(&mut self, fuel: f64) {
        let target = (fuel * self.w * self.h * FUEL_DENSITY) as usize;
        while self.nuclei.len() < target {
            let x = NUCLEUS_R + self.rng.f64() * (self.w - 2.0 * NUCLEUS_R);
            let y = NUCLEUS_R + self.rng.f64() * (self.h - 2.0 * NUCLEUS_R);
            self.nuclei.push(Nucleus { x, y, vx: 0.0, vy: 0.0, spent: 0.0, flash: 0.0 });
        }
        self.nuclei.truncate(target);
    }

    pub fn rods(&self) -> Rods {
        let count = ((self.w / ROD_SPACING).round() as usize).max(1);
        let stuck = self.stuck_rod.map(|(i, pos)| (i.min(count - 1), pos * self.h));
        Rods { count, spacing: self.w / count as f64, bottom: self.rod_pos * self.h, stuck }
    }

    /// Jams a random control rod where it currently is.
    pub fn jam_random_rod(&mut self) {
        let count = self.rods().count;
        let i = ((self.rng.f64() * count as f64) as usize).min(count - 1);
        self.stuck_rod = Some((i, self.rod_pos));
    }

    pub fn scram(&mut self) { self.scrammed = true; }

    /// Clearing a SCRAM is refused while the core is still melting down.
    pub fn reset_scram(&mut self) {
        if !self.meltdown { self.scrammed = false; }
    }

    /// Fires a burst of neutrons from (x, y), e.g. where the user clicked.
    pub fn inject(&mut self, x: f64, y: f64, count: usize) {
        for _ in 0..count {
            let n = self.make_neutron(x, y, 0.0, 1.0);
            if self.neutrons.len() < MAX_NEUTRONS { self.neutrons.push(n); }
        }
    }

    fn make_neutron(&mut self, x: f64, y: f64, offset: f64, speed_scale: f64) -> Neutron {
        let a = self.rng.f64() * std::f64::consts::TAU;
        let (s, c) = a.sin_cos();
        let v = NEUTRON_SPEED * speed_scale * (0.85 + self.rng.f64() * 0.3);
        let (x, y) = (x + c * offset, y + s * offset);
        Neutron { x, y, vx: c * v, vy: s * v, px: x, py: y, life: NEUTRON_LIFE }
    }

    pub fn stats(&self) -> ReactorStats {
        let status = if self.meltdown {
            Status::Meltdown
        } else if self.neutrons.len() < 4 && self.power < 1.0 {
            Status::Shutdown
        } else if self.growth > 0.3 {
            Status::Supercritical
        } else if self.growth < -0.3 {
            Status::Subcritical
        } else {
            Status::Critical
        };
        ReactorStats {
            power: self.power, neutrons: self.neutrons.len(), temp: self.temp,
            rods: self.rod_pos, status, scrammed: self.scrammed,
            output_mw: self.power * MW_PER_FISSION * (1.0 - PUMP_COST * self.pump),
            xenon: self.xenon,
            pump_failed: self.cooling_factor < 1.0,
            rod_stuck: self.stuck_rod.is_some(),
        }
    }

    /// Advances `dt` frames with the operator's rod setting; returns fissions this step.
    pub fn step(&mut self, dt: f64, rod_target: f64) -> usize {
        let secs = dt / 60.0;

        let target = if self.scrammed { 1.0 } else { rod_target.clamp(0.0, 1.0) };
        let travel = if self.scrammed { SCRAM_SPEED } else { ROD_SPEED } * secs;
        self.rod_pos += (target - self.rod_pos).clamp(-travel, travel);

        for n in self.neutrons.iter_mut() { n.px = n.x; n.py = n.y; }

        // Substeps of at most one frame so fast neutrons can't tunnel through nuclei
        let steps = (dt.ceil() as usize).max(1);
        let sub = dt / steps as f64;
        let mut fissions = 0;
        for _ in 0..steps { fissions += self.substep(sub); }

        // Spontaneous fission from a random fuel nucleus keeps a source of neutrons
        if self.rng.f64() < SOURCE_RATE * secs && !self.nuclei.is_empty() {
            let i = ((self.rng.f64() * self.nuclei.len() as f64) as usize).min(self.nuclei.len() - 1);
            let (x, y) = (self.nuclei[i].x, self.nuclei[i].y);
            let n = self.make_neutron(x, y, NUCLEUS_R + 2.0, 1.0);
            if self.neutrons.len() < MAX_NEUTRONS { self.neutrons.push(n); }
        }

        self.temp += HEAT_PER_FISSION * fissions as f64;
        let cooling = (COOLING_BASE + COOLING_PUMP * self.pump.clamp(0.0, 1.0)) * self.cooling_factor;
        self.temp -= cooling * (self.temp - AMBIENT_TEMP) * secs;

        // Iodine → xenon chain; flux burns xenon off while the core is running
        let decayed = self.iodine * IODINE_DECAY * secs;
        self.iodine += IODINE_YIELD * fissions as f64 - decayed;
        let flux = self.neutrons.len() as f64 / FLUX_REF;
        self.xenon += decayed - self.xenon * (XENON_DECAY + XENON_BURN * flux) * secs;
        self.xenon = self.xenon.max(0.0);
        if self.temp > MELTDOWN_TEMP { self.meltdown = true; self.scrammed = true; }
        if self.meltdown && self.temp < MELTDOWN_CLEAR_TEMP { self.meltdown = false; }

        // Smoothed power and neutron growth rate (per second, log scale) for the HUD
        if secs > 0.0 {
            let a = 1.0 - (-secs / 0.8).exp();
            self.power += (fissions as f64 / secs - self.power) * a;
            let n = self.neutrons.len() as f64;
            let g = ((n + 1.0).ln() - (self.prev_neutrons + 1.0).ln()) / secs;
            self.growth += (g - self.growth) * a;
            self.prev_neutrons = n;
        }

        for f in self.flashes.iter_mut() { f.age += secs; }
        self.flashes.retain(|f| f.age < FLASH_SECS);

        fissions
    }

    fn substep(&mut self, dt: f64) -> usize {
        let secs = dt / 60.0;
        let (w, h) = (self.w, self.h);
        let rods = self.rods();
        let jitter = 0.04 + (self.temp - AMBIENT_TEMP).max(0.0) / 1000.0 * 0.25;
        let drag = COOLANT_DRAG.powf(dt);
        let xenon_p = self.xenon * XENON_ABSORB * secs;
        let Reactor { nuclei, neutrons, flashes, rng, grid, born, .. } = self;

        // ── Nuclei: thermal jostling, coolant drag, walls and rods ──
        for n in nuclei.iter_mut() {
            n.vx = (n.vx + (rng.f64() - 0.5) * jitter * dt) * drag;
            n.vy = (n.vy + (rng.f64() - 0.5) * jitter * dt) * drag;
            n.x += n.vx * dt;
            n.y += n.vy * dt;
            if n.x < NUCLEUS_R { n.x = NUCLEUS_R; n.vx = n.vx.abs(); }
            if n.x > w - NUCLEUS_R { n.x = w - NUCLEUS_R; n.vx = -n.vx.abs(); }
            if n.y < NUCLEUS_R { n.y = NUCLEUS_R; n.vy = n.vy.abs(); }
            if n.y > h - NUCLEUS_R { n.y = h - NUCLEUS_R; n.vy = -n.vy.abs(); }
            if let Some(rx) = rods.hit(n.x, n.y, NUCLEUS_R) {
                let side = if n.x < rx { -1.0 } else { 1.0 };
                n.x = rx + side * (ROD_WIDTH / 2.0 + NUCLEUS_R);
                n.vx = side * n.vx.abs();
            }
            n.spent = (n.spent - secs).max(0.0);
            n.flash = (n.flash - secs / FLASH_SECS).max(0.0);
        }

        // ── Nucleus–nucleus bounces ──
        grid.rebuild(NUCLEUS_R * 2.0 + 2.0, w, h, nuclei.iter().map(|n| (n.x, n.y)));
        grid.for_each_pair(|i, j| collide(nuclei, i, j));

        // ── Neutrons: fly, leak, get absorbed, scatter or split fuel ──
        let mut fissions = 0;
        let mut k = 0;
        while k < neutrons.len() {
            let nt = &mut neutrons[k];
            nt.x += nt.vx * dt;
            nt.y += nt.vy * dt;
            nt.life -= secs;
            let leaked = nt.x < 0.0 || nt.x > w || nt.y < 0.0 || nt.y > h;
            let poisoned = rng.f64() < xenon_p;
            if nt.life <= 0.0 || leaked || poisoned || rods.hit(nt.x, nt.y, 1.5).is_some() {
                neutrons.swap_remove(k);
                continue;
            }

            let (x, y) = (nt.x, nt.y);
            let reach = (NUCLEUS_R + 1.0) * (NUCLEUS_R + 1.0);
            let mut hit = None;
            grid.for_each_near(x, y, |i| {
                let (dx, dy) = (x - nuclei[i].x, y - nuclei[i].y);
                if hit.is_none() && dx * dx + dy * dy < reach { hit = Some(i); }
            });
            let Some(i) = hit else { k += 1; continue };

            let nt = neutrons[k];
            let nu = &mut nuclei[i];
            let speed = nt.vx.hypot(nt.vy);
            if nu.spent == 0.0 {
                let p = if speed <= SLOW_SPEED { FISSION_P_SLOW } else { FISSION_P_FAST };
                if rng.f64() < p {
                    // Fission: the nucleus is flung away hot and 2–3 neutrons are born
                    fissions += 1;
                    let a = rng.f64() * std::f64::consts::TAU;
                    nu.vx += a.cos() * FRAGMENT_KICK;
                    nu.vy += a.sin() * FRAGMENT_KICK;
                    nu.spent = REFUEL_SECS.0 + rng.f64() * (REFUEL_SECS.1 - REFUEL_SECS.0);
                    nu.flash = 1.0;
                    flashes.push(Flash { x: nu.x, y: nu.y, age: 0.0 });
                    let (fx, fy) = (nu.x, nu.y);
                    let count = if rng.f64() < 0.5 { 2 } else { 3 };
                    for _ in 0..count {
                        let a = rng.f64() * std::f64::consts::TAU;
                        let (s, c) = a.sin_cos();
                        let v = NEUTRON_SPEED * (0.85 + rng.f64() * 0.3);
                        let (bx, by) = (fx + c * (NUCLEUS_R + 2.0), fy + s * (NUCLEUS_R + 2.0));
                        born.push(Neutron { x: bx, y: by, vx: c * v, vy: s * v, px: bx, py: by, life: NEUTRON_LIFE });
                    }
                    neutrons.swap_remove(k);
                    continue;
                }
            } else if rng.f64() < POISON_P {
                neutrons.swap_remove(k);
                continue;
            }

            // Scatter: bounce off the nucleus and lose some speed (moderation)
            let (dx, dy) = (x - nu.x, y - nu.y);
            let d = dx.hypot(dy).max(1e-6);
            let (nx, ny) = (dx / d, dy / d);
            let dot = nt.vx * nx + nt.vy * ny;
            let (mut vx, mut vy) = if dot < 0.0 {
                (nt.vx - 2.0 * dot * nx, nt.vy - 2.0 * dot * ny)
            } else {
                (nt.vx, nt.vy)
            };
            let keep = (speed * SCATTER_KEEP).max(MIN_NEUTRON_SPEED) / speed.max(1e-6);
            vx *= keep;
            vy *= keep;
            nu.vx -= nx * 0.15;
            nu.vy -= ny * 0.15;
            let nt = &mut neutrons[k];
            nt.vx = vx;
            nt.vy = vy;
            nt.x = nu.x + nx * (NUCLEUS_R + 1.5);
            nt.y = nu.y + ny * (NUCLEUS_R + 1.5);
            k += 1;
        }

        let room = MAX_NEUTRONS.saturating_sub(neutrons.len());
        neutrons.extend(born.drain(..).take(room));
        fissions
    }
}

/// Equal-mass bounce between nuclei `i` and `j`. Hot (recently split) nuclei
/// hit harder and pass some of their glow on, so heat spreads through the core.
fn collide(nuclei: &mut [Nucleus], i: usize, j: usize) {
    let (lo, hi) = (i.min(j), i.max(j));
    let (head, tail) = nuclei.split_at_mut(hi);
    let (a, b) = (&mut head[lo], &mut tail[0]);
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let d2 = dx * dx + dy * dy;
    let min = 2.0 * NUCLEUS_R;
    if d2 >= min * min || d2 < 1e-9 { return; }
    let d = d2.sqrt();
    let (nx, ny) = (dx / d, dy / d);
    let push = (min - d) * 0.5;
    a.x -= nx * push; a.y -= ny * push;
    b.x += nx * push; b.y += ny * push;
    let rel = (b.vx - a.vx) * nx + (b.vy - a.vy) * ny;
    if rel < 0.0 {
        let heat = a.flash.max(b.flash);
        let impulse = rel * (1.0 + 0.4 * heat);
        a.vx += impulse * nx; a.vy += impulse * ny;
        b.vx -= impulse * nx; b.vy -= impulse * ny;
        let glow = heat * 0.7;
        a.flash = a.flash.max(glow);
        b.flash = b.flash.max(glow);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Holds the rods at `rods` (skipping the startup withdrawal) for `secs`
    /// seconds and returns (mean fissions/s over the last half, peak temperature).
    fn run(rods: f64, secs: f64, seed: u64) -> (f64, f64, bool) {
        let mut r = Reactor::new(1920.0, 1080.0, 0.75, seed);
        r.rod_pos = rods;
        r.inject(960.0, 540.0, 20);
        let frames = (secs * 60.0) as usize;
        let (mut fissions, mut peak, mut melted) = (0usize, 0.0f64, false);
        for f in 0..frames {
            let n = r.step(1.0, rods);
            if f >= frames / 2 { fissions += n; }
            peak = peak.max(r.temp);
            melted |= r.meltdown;
        }
        (fissions as f64 / (secs / 2.0), peak, melted)
    }

    #[test]
    fn rods_withdrawn_melts_down_and_auto_scram_stops_it() {
        for seed in [7, 99] {
            let mut r = Reactor::new(1920.0, 1080.0, 0.75, seed);
            r.rod_pos = 0.0;
            r.inject(960.0, 540.0, 20);
            let mut frames = 0;
            while !r.meltdown && frames < 60 * 60 { r.step(1.0, 0.0); frames += 1; }
            assert!(r.meltdown && r.scrammed, "seed {seed}: no meltdown within 60s");
            for _ in 0..60 * 15 { r.step(1.0, 0.0); }
            let power = r.stats().power;
            assert!(power < 3.0, "seed {seed}: 15s after auto-SCRAM power is still {power:.1}/s");
        }
    }

    #[test]
    fn rods_half_in_sustain_a_steady_reaction() {
        for seed in [7, 99] {
            let (power, peak, melted) = run(0.5, 40.0, seed);
            assert!(!melted, "seed {seed}: melted down at 50% rods (peak {peak:.0}°C)");
            assert!(power > 8.0, "seed {seed}: reaction died at 50% rods ({power:.1}/s)");
        }
    }

    #[test]
    fn rods_fully_in_shut_the_reaction_down() {
        for seed in [7, 99] {
            let (power, _, melted) = run(1.0, 40.0, seed);
            assert!(!melted && power < 4.0, "seed {seed}: {power:.1}/s with rods fully in");
        }
    }

    #[test]
    fn xenon_builds_up_after_power_is_cut() {
        let mut r = Reactor::new(1920.0, 1080.0, 0.75, 7);
        r.rod_pos = 0.45;
        r.inject(960.0, 540.0, 20);
        for _ in 0..60 * 40 { r.step(1.0, 0.45); }
        let at_power = r.xenon;
        println!("at power: {:.1} fissions/s, {} neutrons", r.stats().power, r.neutrons.len());
        r.scram();
        let mut peak = 0.0f64;
        for _ in 0..60 * 20 { r.step(1.0, 1.0); peak = peak.max(r.xenon); }
        println!("xenon at power {at_power:.3}, peak after shutdown {peak:.3}");
        assert!(at_power > 0.02, "no xenon at power ({at_power:.3})");
        assert!(peak > at_power * 1.3, "no xenon pit: {at_power:.3} -> {peak:.3}");
    }

    #[test]
    fn pump_failure_runs_the_core_hotter() {
        let peak_temp = |cooling_factor: f64| {
            let mut r = Reactor::new(1920.0, 1080.0, 0.75, 7);
            r.rod_pos = 0.5;
            r.cooling_factor = cooling_factor;
            r.inject(960.0, 540.0, 20);
            let mut peak = 0.0f64;
            for _ in 0..60 * 30 { r.step(1.0, 0.5); peak = peak.max(r.temp); }
            peak
        };
        let (normal, failed) = (peak_temp(1.0), peak_temp(0.35));
        println!("peak temp: pump ok {normal:.0}°C, pump failed {failed:.0}°C");
        assert!(failed > normal + 150.0);
    }
}
