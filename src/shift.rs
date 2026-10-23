//! Operator shift: a compressed day (06:00 to 06:00 in three minutes) of grid
//! demand to follow. Keeping net output inside the demand band scores points,
//! with a multiplier that grows the longer you stay in it. Random incidents
//! hit the plant along the way, and a meltdown ends the shift.
//!
//! Plain Rust like the reactor, so it is unit tested natively.

use crate::reactor::{Reactor, Rng};

pub const SHIFT_SECS: f64 = 180.0;
/// No incidents while the operator brings a cold core up to power
const STARTUP_SECS: f64 = 20.0;
const BAND_FRAC: f64 = 0.15;
const BAND_MIN_MW: f64 = 35.0;
const POINTS_PER_SEC: f64 = 10.0;
/// Seconds in band per multiplier step
const STREAK_STEP_SECS: f64 = 10.0;
const MAX_MULTIPLIER: u32 = 5;
const INCIDENT_GAP_SECS: (f64, f64) = (22.0, 35.0);
/// No new incident this close to the end of the shift
const INCIDENT_CUTOFF_SECS: f64 = 15.0;
const SURGE_MW: f64 = 130.0;
/// Turbine trip: the grid takes only this share of normal demand
const TRIP_FRAC: f64 = 0.35;
const PUMP_FAIL_COOLING: f64 = 0.35;
const START_HOUR: f64 = 6.0;

/// Grid demand through the day as (hour, MW): night trough, morning ramp, evening peak
const DEMAND_CURVE: [(f64, f64); 9] = [
    (0.0, 170.0), (4.0, 140.0), (7.0, 180.0), (9.0, 300.0), (13.0, 340.0),
    (17.0, 380.0), (19.5, 500.0), (22.0, 300.0), (24.0, 170.0),
];

/// Demand in MW at a clock hour, eased between the curve's points
pub fn base_demand(hour: f64) -> f64 {
    let hour = hour.rem_euclid(24.0);
    for pair in DEMAND_CURVE.windows(2) {
        let ((h0, d0), (h1, d1)) = (pair[0], pair[1]);
        if hour <= h1 {
            let t = (hour - h0) / (h1 - h0);
            let eased = 0.5 - 0.5 * (t * std::f64::consts::PI).cos();
            return d0 + (d1 - d0) * eased;
        }
    }
    DEMAND_CURVE[0].1
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Incident { PumpFailure, StuckRod, DemandSurge, TurbineTrip }

impl Incident {
    const ALL: [Incident; 4] = [Incident::PumpFailure, Incident::StuckRod, Incident::DemandSurge, Incident::TurbineTrip];

    pub fn title(self) -> &'static str {
        match self {
            Incident::PumpFailure => "Coolant pump failure",
            Incident::StuckRod => "Control rod jammed",
            Incident::DemandSurge => "Heatwave demand surge",
            Incident::TurbineTrip => "Turbine trip",
        }
    }

    pub fn advice(self) -> &'static str {
        match self {
            Incident::PumpFailure => "Cooling is down to 35%. Cut power or the core will overheat.",
            Incident::StuckRod => "One rod won't move until maintenance frees it. Use the others.",
            Incident::DemandSurge => "Air conditioners everywhere. Raise output to meet the surge.",
            Incident::TurbineTrip => "The grid can't take your power. Bring output down fast, but beware xenon.",
        }
    }

    fn duration(self) -> f64 {
        match self {
            Incident::PumpFailure => 20.0,
            Incident::StuckRod => 25.0,
            Incident::DemandSurge => 18.0,
            Incident::TurbineTrip => 15.0,
        }
    }

    fn begin(self, reactor: &mut Reactor) {
        match self {
            Incident::PumpFailure => reactor.cooling_factor = PUMP_FAIL_COOLING,
            Incident::StuckRod => reactor.jam_random_rod(),
            Incident::DemandSurge | Incident::TurbineTrip => {}
        }
    }

    fn end(self, reactor: &mut Reactor) {
        match self {
            Incident::PumpFailure => reactor.cooling_factor = 1.0,
            Incident::StuckRod => reactor.stuck_rod = None,
            Incident::DemandSurge | Incident::TurbineTrip => {}
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShiftState { Running, Complete, Failed }

/// What the HUD shows about the shift; sent to the UI with the reactor stats
#[derive(Clone, Copy, PartialEq)]
pub struct ShiftStats {
    pub state: ShiftState,
    /// Clock hour, 0.0..24.0
    pub clock: f64,
    /// Fraction of the shift elapsed
    pub progress: f64,
    pub demand_mw: f64,
    pub band_lo: f64,
    pub band_hi: f64,
    pub in_band: bool,
    pub score: u32,
    pub multiplier: u32,
    pub incident: Option<Incident>,
    pub incident_left: f64,
    pub startup: bool,
    /// Share of the elapsed shift spent in band
    pub in_band_frac: f64,
    pub stars: u8,
}

pub struct Shift {
    t: f64,
    score: f64,
    streak: f64,
    in_band: bool,
    in_band_secs: f64,
    state: ShiftState,
    incident: Option<(Incident, f64)>,
    next_incident: f64,
    rng: Rng,
}

impl Shift {
    pub fn new(seed: u64) -> Self {
        Self {
            t: 0.0, score: 0.0, streak: 0.0, in_band: false, in_band_secs: 0.0,
            state: ShiftState::Running, incident: None,
            next_incident: STARTUP_SECS + 10.0, rng: Rng::new(seed),
        }
    }

    pub fn state(&self) -> ShiftState { self.state }

    pub fn clock(&self) -> f64 {
        (START_HOUR + self.t / SHIFT_SECS * 24.0).rem_euclid(24.0)
    }

    pub fn demand_mw(&self) -> f64 {
        let base = base_demand(self.clock());
        match self.incident {
            Some((Incident::DemandSurge, _)) => base + SURGE_MW,
            Some((Incident::TurbineTrip, _)) => base * TRIP_FRAC,
            _ => base,
        }
    }

    fn band(&self) -> (f64, f64) {
        let d = self.demand_mw();
        let half = (d * BAND_FRAC).max(BAND_MIN_MW);
        (d - half, d + half)
    }

    fn multiplier(&self) -> u32 {
        (1 + (self.streak / STREAK_STEP_SECS) as u32).min(MAX_MULTIPLIER)
    }

    /// Scores `secs` of running at `output_mw` against the current demand band.
    fn score(&mut self, secs: f64, output_mw: f64) {
        let (lo, hi) = self.band();
        self.in_band = (lo..=hi).contains(&output_mw);
        if self.in_band {
            self.streak += secs;
            self.in_band_secs += secs;
            self.score += POINTS_PER_SEC * self.multiplier() as f64 * secs;
        } else {
            self.streak = 0.0;
        }
    }

    /// Advances the shift by `secs` real seconds, applying incidents to the reactor.
    pub fn step(&mut self, secs: f64, reactor: &mut Reactor) {
        if self.state != ShiftState::Running { return; }
        self.t += secs;
        self.score(secs, reactor.stats().output_mw);

        match self.incident {
            Some((kind, left)) if left - secs <= 0.0 => {
                kind.end(reactor);
                self.incident = None;
                let (lo, hi) = INCIDENT_GAP_SECS;
                self.next_incident = self.t + lo + self.rng.f64() * (hi - lo);
            }
            Some((kind, left)) => self.incident = Some((kind, left - secs)),
            None if self.t >= self.next_incident && self.t < SHIFT_SECS - INCIDENT_CUTOFF_SECS => {
                let i = ((self.rng.f64() * 4.0) as usize).min(3);
                let kind = Incident::ALL[i];
                kind.begin(reactor);
                self.incident = Some((kind, kind.duration()));
            }
            None => {}
        }

        let finished = if reactor.meltdown {
            self.score *= 0.5;
            Some(ShiftState::Failed)
        } else if self.t >= SHIFT_SECS {
            Some(ShiftState::Complete)
        } else {
            None
        };
        if let Some(state) = finished {
            if let Some((kind, _)) = self.incident.take() { kind.end(reactor); }
            self.state = state;
        }
    }

    pub fn stats(&self) -> ShiftStats {
        let (band_lo, band_hi) = self.band();
        let in_band_frac = if self.t > 0.0 { self.in_band_secs / self.t } else { 0.0 };
        let stars = match self.state {
            ShiftState::Failed => 0,
            _ if in_band_frac >= 0.7 => 3,
            _ if in_band_frac >= 0.45 => 2,
            _ if in_band_frac >= 0.2 => 1,
            _ => 0,
        };
        ShiftStats {
            state: self.state,
            clock: self.clock(),
            progress: (self.t / SHIFT_SECS).min(1.0),
            demand_mw: self.demand_mw(),
            band_lo, band_hi,
            in_band: self.in_band,
            score: self.score.round() as u32,
            multiplier: self.multiplier(),
            incident: self.incident.map(|(k, _)| k),
            incident_left: self.incident.map_or(0.0, |(_, left)| left),
            startup: self.t < STARTUP_SECS,
            in_band_frac,
            stars,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demand_has_a_night_trough_and_evening_peak() {
        let (night, peak) = (base_demand(4.0), base_demand(19.5));
        assert!((night - 140.0).abs() < 1e-9 && (peak - 500.0).abs() < 1e-9);
        for h in 0..240 {
            let d = base_demand(h as f64 / 10.0);
            assert!((140.0..=500.0).contains(&d), "demand {d} at {h}");
        }
        assert!((base_demand(23.999) - base_demand(0.0)).abs() < 0.1, "curve doesn't wrap smoothly");
    }

    #[test]
    fn streak_grows_multiplier_and_missing_the_band_resets_it() {
        let mut s = Shift::new(1);
        let demand = s.demand_mw();
        for _ in 0..30 { s.score(1.0, demand); }
        assert_eq!(s.multiplier(), 4);
        let before = s.score;
        s.score(1.0, demand * 3.0);
        assert_eq!(s.multiplier(), 1);
        assert_eq!(s.score, before, "points scored while out of band");
    }

    #[test]
    fn incidents_apply_and_clear_and_the_shift_completes() {
        let mut reactor = Reactor::new(1920.0, 1080.0, 0.75, 3);
        let mut shift = Shift::new(5);
        let mut seen = Vec::new();
        let mut frames = 0;
        while shift.state() == ShiftState::Running && frames < 60 * 200 {
            // Hold the rods well in so this test never melts down
            reactor.step(1.0, 0.75);
            shift.step(1.0 / 60.0, &mut reactor);
            match shift.stats().incident {
                Some(Incident::PumpFailure) => assert!(reactor.cooling_factor < 1.0),
                Some(Incident::StuckRod) => assert!(reactor.stuck_rod.is_some()),
                _ => {}
            }
            if let Some(k) = shift.stats().incident { if !seen.contains(&k) { seen.push(k); } }
            frames += 1;
        }
        assert_eq!(shift.state(), ShiftState::Complete);
        assert!(seen.len() >= 2, "only saw incidents {seen:?}");
        assert_eq!(reactor.cooling_factor, 1.0, "pump left failed after the shift");
        assert!(reactor.stuck_rod.is_none(), "rod left jammed after the shift");
    }
}
