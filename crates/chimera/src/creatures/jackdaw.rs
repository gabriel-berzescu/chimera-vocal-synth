//! Western jackdaw (Coloeus monedula), about 230 g. An oscine, so the songbird's plan about twice
//! as long: two labial pairs in the syrinx, one per bronchus, a long trachea, the larynx, the
//! throat (the oropharyngeal–oesophageal cavity) and a short stout beak.
//!
//! What is measured and what is not:
//! * The call. The "tchak" contact call is 157 ± 22 ms long with a mean fundamental of
//!   709 ± 116 Hz (SD over 464 calls of nine males), falling gradually, "often with a brief
//!   inflection marking a transition between two segments"; a second form starts harmonic and
//!   turns noisy, with a strongly modulated end (Szipl, Baotic & Kotrschal 2025, "Variability and
//!   individuality in the contact calls of jackdaws", Animal Cognition 29:2). The paper analysed
//!   397 calls of the first form; the 67 that leaves of its 464, about one in seven, is a
//!   subtraction made here, not a figure of the paper's.
//!   The paper's main text gives the mean pitch and the shape in words: the start, inflection and
//!   end frequencies below are chosen so that the mean comes out at 709 Hz.
//! * The trachea's size, by a rule for birds in general and not by a jackdaw's own. Hinds & Calder
//!   (1971, "Tracheal dead space in the respiration of birds", Evolution 25(2):429–440), as quoted
//!   by Wedel (2007, arXiv:1302.3267, p. 44; the paper itself was not read): length
//!   16.77·M^0.394 cm and volume 3.724·M^1.090 (taken to be ml), M in kg. For 0.23 kg that is
//!   9.4 cm and 0.75 ml, and a cylinder of that length and volume is 0.32 cm across: 0.08 cm².
//!   The model's trachea is 9 cm by 0.08 cm². Which birds the rule was fitted to, and whether
//!   songbirds lie on it, is not known here.
//! * Everything else (bronchi, valve size, larynx, throat volume, beak, pressures, how the beak
//!   moves through a call, where the noise comes from) is unmeasured: sized by proportion from the
//!   songbird next door and tuned by looking at spectrograms.
//!
//! The noisy form's roughness is made three ways at once: the pitch wobbles, the two sides of the
//! syrinx drift apart, and the valves hiss (turbulence, the network's Reynolds-number source). None
//! of the three is a claim about how a jackdaw does it: nonlinear labial dynamics are as likely.

use super::build::{TubeArea, TubeOpts, fill_areas, sections, tube};
use super::types::{CallDef, CallKey, CreatureDef, CreatureGroup, CreatureSim, Curve, GateTimes, ParamDef};
use crate::airway::network::{AirwayNetwork, NoiseSpec, PortSpec, Tube};
use crate::math::{cbrt, js_max, js_min, pow};
use crate::physics::WALL_TRACHEA;
use crate::sources::syrinx::SyrinxLabia;

const BRONCHUS_LEN: f64 = 1.2; // cm
const BRONCHUS_AREA: f64 = 0.045; // cm²
const TRACHEA_LEN: f64 = 9.0; // cm
const TRACHEA_AREA: f64 = 0.08; // cm²
const LARYNX_AREA: f64 = 0.055; // cm²
const THROAT: f64 = 1.6; // cm³
const BEAK_LEN: f64 = 3.0; // cm
/// widest opening of one syringeal valve (cm²)
const AMAX: f64 = 0.05;
/// The trachea and the beak are built with sections this much longer than the shortest stable one,
/// so that a smaller bird (`size` down to 0.88) still has stable sections.
const HEADROOM: f64 = 1.2;
/// Turbulence gain at full rasp, at the 192 kHz the airway runs at for a 48 kHz device (`create`
/// scales it for other rates). The hiss is then about 13 dB under the rest of the sound at the
/// noisy call's end and 23 dB under a held note; with the dial at its default it is 38 dB under
/// the clear call.
const RASP_GAIN: f64 = 1.9e-3;

/// The rasp dial's default, and where every call starts from.
const RASP_CLEAR: f64 = 0.2;

/// The tchak's mean fundamental (Hz) and length (s): Szipl et al. 2025.
pub const TCHAK_HZ: f64 = 709.0;
const TCHAK_SECONDS: f64 = 0.157;

// parameter indices (order of `def().params`)
const PITCH: usize = 0;
const VOICES: usize = 1;
const INTERVAL: usize = 2;
const DRIVE: usize = 3;
const TENSION: usize = 4;
const PRESSURE: usize = 5;
const RASP: usize = 6;
const OEC: usize = 7;
const SIZE: usize = 8;
const GAPE: usize = 9;

pub struct Jackdaw {
    net: AirwayNetwork,
    tubes: Vec<(&'static str, Tube)>,
    trachea: Tube,
    beak: Tube,
    sac: usize,
    v_l: usize,
    v_r: usize,
    oec: usize,
    oec_len: f64,
    lab_l: SyrinxLabia,
    lab_r: SyrinxLabia,
    /// the branch just below each valve (turbulence goes there when the flow runs backwards)
    below_l: usize,
    below_r: usize,
    /// `RASP_GAIN` for this rate's sections
    rasp_gain: f64,
    trachea_areas: Vec<f64>,
    beak_areas: Vec<f64>,
}

fn create(fs: f64) -> Box<dyn CreatureSim> {
    let mut net = AirwayNetwork::new(fs);
    // A short tube takes as few sections as its length allows at this rate. The helper's floor of
    // three would cut a 1.2 cm bronchus into sections shorter than sound travels in a sample once
    // the device runs at 16 kHz (64 kHz here), and the airway would run away, as the voice did in
    // issue #33. One section at 32 kHz, two at 64 kHz, six at 192 kHz.
    let short = TubeOpts { min: Some(1.0), ..Default::default() };
    let bron_l = tube(&mut net, "bronchusL", fs, BRONCHUS_LEN, TubeArea::Uniform(BRONCHUS_AREA), short);
    let bron_r = tube(&mut net, "bronchusR", fs, BRONCHUS_LEN, TubeArea::Uniform(BRONCHUS_AREA), short);
    let trachea = tube(&mut net, "trachea", fs, TRACHEA_LEN, TubeArea::Uniform(TRACHEA_AREA), TubeOpts { wall: Some(WALL_TRACHEA), coarse: Some(HEADROOM), ..Default::default() });
    let sac = net.add_pressure_node("air sac");
    let sac_l = net.add_port(sac, bron_l.first_node(), PortSpec::new(0.09, 0.5));
    let sac_r = net.add_port(sac, bron_r.first_node(), PortSpec::new(0.09, 0.5));
    let v_l = net.add_port(bron_l.last_node(), trachea.first_node(), PortSpec::new(0.0, 0.1).kinetic(0.5, 1.0));
    let v_r = net.add_port(bron_r.last_node(), trachea.first_node(), PortSpec::new(0.0, 0.1).kinetic(0.5, 1.0));
    let oec = net.add_cavity("oec", THROAT, None);
    net.add_port(trachea.last_node(), oec, PortSpec::new(LARYNX_AREA, 0.3).kinetic(0.3, 0.8));
    let beak = tube(&mut net, "beak", fs, BEAK_LEN, TubeArea::Uniform(0.2), TubeOpts { coarse: Some(HEADROOM), min: Some(2.0), ..Default::default() });
    net.connect(oec, beak.first_node());
    net.add_radiation(beak.last_node());
    net.finalize();
    // The turbulence is a pressure put across one section of the trachea, just above the valves,
    // and the longer that section the more of it reaches the beak: measured, the same gain gave
    // 14 dB more hiss at a 16 kHz device (three times the section) than at 48 kHz. So the gain
    // goes down as the section goes up. That holds the hiss within about 3 dB from 16 to 96 kHz;
    // at 8 and 11 kHz it overcorrects, and the hiss is some 7 dB quiet.
    let section = |n: usize| TRACHEA_LEN / n as f64;
    let rasp_gain = RASP_GAIN * section(sections(192_000.0, TRACHEA_LEN, 3.0, 120.0, HEADROOM)) / section(trachea.n);
    Box::new(Jackdaw {
        net,
        tubes: vec![("trachea", trachea), ("beak", beak), ("bronchusL", bron_l), ("bronchusR", bron_r)],
        trachea,
        beak,
        sac,
        v_l,
        v_r,
        oec,
        oec_len: cbrt(THROAT),
        lab_l: SyrinxLabia::new(fs, 0.0),
        lab_r: SyrinxLabia::new(fs, 1.0),
        // the bronchus's last branch, or the port from the air sac when it has only one section
        below_l: if bron_l.n >= 2 { bron_l.branch(bron_l.n - 2) } else { sac_l },
        below_r: if bron_r.n >= 2 { bron_r.branch(bron_r.n - 2) } else { sac_r },
        rasp_gain,
        trachea_areas: vec![0.0; trachea.n],
        beak_areas: vec![0.0; beak.n],
    })
}

impl CreatureSim for Jackdaw {
    fn net(&self) -> Option<&AirwayNetwork> {
        Some(&self.net)
    }
    fn net_mut(&mut self) -> Option<&mut AirwayNetwork> {
        Some(&mut self.net)
    }
    fn tubes(&self) -> &[(&'static str, Tube)] {
        &self.tubes
    }
    fn gain(&self) -> f64 {
        2.4e-7
    }
    fn control(&mut self, p: &[f64], gate: f64, _dt: f64) {
        let (lab_l, lab_r) = (&mut self.lab_l, &mut self.lab_r);
        self.net.set_fixed_pressure(self.sac, p[PRESSURE] * 1e4 * gate); // kPa → dyn/cm²
        lab_l.f0 = p[PITCH];
        lab_r.f0 = p[PITCH] * pow(2.0, p[INTERVAL] / 12.0);
        lab_l.drive = p[DRIVE];
        lab_r.drive = p[DRIVE];
        lab_l.tension = p[TENSION];
        lab_r.tension = p[TENSION];
        // voices: 0 = left only, 0.5 = both, 1 = right only
        lab_l.gate = if p[VOICES] <= 0.5 { 1.0 } else { js_max(0.0, 2.0 - 2.0 * p[VOICES]) };
        lab_r.gate = if p[VOICES] >= 0.5 { 1.0 } else { js_max(0.0, 2.0 * p[VOICES]) };
        // a bigger or smaller bird: the trachea and the beak longer by `size` and wider by its
        // square, the throat by its cube (the bronchi, the valves and the larynx stay as they are)
        let size = js_min(1.12, js_max(0.88, p[SIZE]));
        let areas = fill_areas(self.trachea, |_| TRACHEA_AREA * size * size, &mut self.trachea_areas);
        self.net.set_tube_targets(self.trachea, areas, Some(TRACHEA_LEN * size), None);
        let vol = js_min(4.0, js_max(0.3, p[OEC])) * size * size * size;
        self.net.set_node_area_target(self.oec, vol / self.oec_len);
        // the beak opens from its base to its tip
        let gape = p[GAPE];
        let areas = fill_areas(self.beak, |u| (0.02 + gape * (0.1 + 0.5 * u)) * size * size, &mut self.beak_areas);
        self.net.set_tube_targets(self.beak, areas, Some(BEAK_LEN * size), None);
        // turbulence at each syringeal valve, heard in the trachea just above it
        let g = p[RASP] * self.rasp_gain;
        if g > 0.0 {
            let fwd = self.trachea.branch(0);
            let list = [
                NoiseSpec::new(self.v_l, fwd, self.below_l, g).band(9000.0, 700.0),
                NoiseSpec::new(self.v_r, fwd, self.below_r, g).band(9000.0, 700.0),
            ];
            self.net.set_noise_sources(&list);
        } else {
            self.net.set_noise_sources(&[]);
        }
    }
    #[inline]
    fn sample(&mut self) -> f64 {
        let a = AMAX * self.lab_l.next();
        self.net.set_port_area(self.v_l, a);
        let a = AMAX * self.lab_r.next();
        self.net.set_port_area(self.v_r, a);
        self.net.step()
    }
    fn flush_tiny(&mut self) {
        self.lab_l.flush_tiny();
        self.lab_r.flush_tiny();
    }
    fn source_state(&self, out: &mut Vec<(&'static str, f64)>) {
        out.clear();
        out.push(("left", self.lab_l.opening));
        out.push(("right", self.lab_r.opening));
    }
}

// --- calls --------------------------------------------------------------------------------------

/// The shape of one tchak. Real birds are told apart by theirs (Szipl et al. 2025: mean pitch,
/// when the pitch is lowest, how long the call is), so a flock is made by giving each bird its own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tchak {
    /// pitch scale (1: a mean of 709 Hz, with the other fields at their defaults)
    pub pitch: f64,
    /// length scale (1: 157 ms)
    pub stretch: f64,
    /// the fundamental where the call starts, where it turns and where it ends, over 709 Hz
    pub start: f64,
    pub turn: f64,
    pub end: f64,
    /// when the pitch turns, as a fraction of the call (kept within 0.35..0.62)
    pub turn_at: f64,
    /// the second form: it starts the same and then roughens (the pitch wobbles, the two sides of
    /// the syrinx drift apart and the valves hiss)
    pub noisy: bool,
}

impl Default for Tchak {
    /// The fundamental falls 800 → 715 Hz over the first 85 ms, turns, and falls faster to 580 Hz:
    /// a time-weighted mean of 709 Hz.
    fn default() -> Self {
        Tchak { pitch: 1.0, stretch: 1.0, start: 800.0 / 709.0, turn: 715.0 / 709.0, end: 580.0 / 709.0, turn_at: 0.085 / 0.152, noisy: false }
    }
}

/// The keys of one tchak starting at `t0`, appended to `keys`. The beak snaps open and closes
/// again. Both forms set the rasp at their start: a clear call after a noisy one must not inherit
/// its hiss. With the default shape these are the bestiary's own calls, to within a millisecond.
pub fn tchak_keys(keys: &mut Vec<CallKey>, t0: f64, c: &Tchak) {
    let k = CallKey::new;
    let hz = TCHAK_HZ * c.pitch;
    let from = keys.len();
    keys.push(k(t0, &[("gate", 0.0), ("pitch", c.start * hz), ("gape", 0.2), ("rasp", RASP_CLEAR), ("interval", 0.0), ("voices", 0.5), ("drive", 0.95), ("tension", 0.3), ("pressure", 1.9)]).curve(Curve::Step));
    keys.push(k(t0 + 0.004, &[("gate", 1.0)]));
    if c.noisy {
        let d = TCHAK_SECONDS * c.stretch;
        let at = |u: f64| t0 + u * d;
        keys.push(k(at(0.287), &[("gape", 0.95)]));
        keys.push(k(at(0.35), &[("pitch", 745.0 / 709.0 * (c.start / (800.0 / 709.0)) * hz), ("rasp", RASP_CLEAR), ("interval", 0.0)]).curve(Curve::Exp));
        keys.push(k(at(0.541), &[("pitch", c.turn * hz), ("rasp", 1.0), ("interval", 1.7)]).curve(Curve::Exp));
        keys.push(k(at(0.669), &[("pitch", 790.0 / 709.0 * hz)]).curve(Curve::Exp));
        keys.push(k(at(0.732), &[("gape", 0.7)]));
        keys.push(k(at(0.796), &[("pitch", 610.0 / 709.0 * hz)]).curve(Curve::Exp));
        keys.push(k(at(0.892), &[("pitch", 700.0 / 709.0 * hz)]).curve(Curve::Exp));
        keys.push(k(at(0.955), &[("gate", 1.0), ("pitch", 540.0 / 709.0 * hz)]).curve(Curve::Exp));
        keys.push(k(at(1.0), &[("gate", 0.0), ("gape", 0.15)]));
    } else {
        // the breath is let go 5 ms before the call ends: the gate and the tract ring on that long
        let d = (TCHAK_SECONDS - 0.005) * c.stretch;
        let at = |u: f64| t0 + u * d;
        let turn_at = js_min(0.62, js_max(0.35, c.turn_at));
        keys.push(k(at(0.296), &[("gape", 0.95)]));
        keys.push(k(at(turn_at), &[("pitch", c.turn * hz)]).curve(Curve::Exp));
        keys.push(k(at(turn_at + 0.0987), &[("pitch", (c.turn - 25.0 / 709.0) * hz)]).curve(Curve::Exp));
        keys.push(k(at(0.7566), &[("gape", 0.6)]));
        keys.push(k(at(0.9605), &[("gate", 1.0), ("pitch", c.end * hz)]).curve(Curve::Exp));
        keys.push(k(at(1.0), &[("gate", 0.0), ("gape", 0.15)]));
    }
    // in time order, whatever the shape moved (the last key ends the call)
    keys[from..].sort_by(|a, b| a.t.total_cmp(&b.t));
}

pub fn def() -> CreatureDef {
    let mut clear = Vec::new();
    tchak_keys(&mut clear, 0.0, &Tchak::default());
    let mut noisy = Vec::new();
    tchak_keys(&mut noisy, 0.0, &Tchak { noisy: true, ..Default::default() });
    // one bird calling five times, as they do: no two alike, the fourth gone noisy
    let mut chatter = Vec::new();
    for (t0, pitch, rough) in [(0.0, 1.0, false), (0.31, 1.05, false), (0.57, 0.96, false), (0.93, 1.08, true), (1.21, 0.93, false)] {
        tchak_keys(&mut chatter, t0, &Tchak { pitch, noisy: rough, ..Default::default() });
    }
    def_with_calls(vec![
        CallDef::new("tchak", "Tchak", "The contact call: 157 ms, falling through 709 Hz", Some(TCHAK_HZ), clear),
        CallDef::new("noisy", "Noisy tchak", "The other form: starts clear, ends rough", Some(TCHAK_HZ), noisy),
        CallDef::new("chatter", "Chatter", "One bird, five calls, no two alike", Some(TCHAK_HZ), chatter),
    ])
}

/// The jackdaw with another set of calls: one bird of a flock, with its own versions of the tchak
/// ([`tchak_keys`]). [`def`] is this with the three calls the bestiary shows.
pub fn def_with_calls(calls: Vec<CallDef>) -> CreatureDef {
    CreatureDef {
        id: "jackdaw",
        name: "Jackdaw",
        latin: "Coloeus monedula",
        group: CreatureGroup::Bird,
        blurb: "A songbird's two-sided syrinx at crow scale: a long trachea and a stout beak that snaps open on every call.",
        role: "Sharp tchak hits, call-and-answer chatter, a roost's rising roar",
        oversample: 4,
        gate_times: Some(GateTimes { attack: 0.004, release: 0.008 }),
        params: vec![
            ParamDef::new("pitch", "Pitch", 250.0, 1800.0, TCHAK_HZ).unit("Hz").log().smooth(0.004).pitch(),
            ParamDef::new("voices", "Left ↔ both ↔ right", 0.0, 1.0, 0.5).smooth(0.02),
            ParamDef::new("interval", "Right voice interval", 0.0, 4.0, 0.0).unit("st").smooth(0.01).digits(1),
            ParamDef::new("drive", "Drive (pure → harsh)", 0.0, 1.0, 0.9).smooth(0.02),
            ParamDef::new("tension", "Labial tension", 0.0, 1.0, 0.3).smooth(0.02),
            ParamDef::new("pressure", "Air-sac pressure", 0.2, 3.0, 1.6).unit("kPa").smooth(0.006).digits(2),
            ParamDef::new("rasp", "Rasp", 0.0, 1.0, RASP_CLEAR).smooth(0.006),
            ParamDef::new("oec", "Throat cavity size", 0.3, 4.0, THROAT).unit("cm³").log().smooth(0.03).digits(2),
            ParamDef::new("size", "Body size", 0.88, 1.12, 1.0).smooth(0.05).digits(2),
            ParamDef::new("gape", "Beak gape", 0.0, 1.0, 0.6).smooth(0.008),
        ],
        calls,
        events: vec![],
        create,
    }
    .resolved()
}
