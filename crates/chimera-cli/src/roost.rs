//! `chimera render roost`: a jackdaw roost on a winter morning, deciding when to leave.
//!
//! What it enacts: Dibnah et al. (2022), "Vocally mediated consensus decisions govern mass
//! departures from jackdaw roosts", Current Biology 32. Jackdaws roost together in their hundreds
//! and leave in the morning, often all at once. The paper's finding: the calling in the roost
//! builds, the roost goes when the calling reaches a level, a steeper rise means an earlier
//! departure, and then every bird is in the air within about five seconds. Playing roost calls to
//! a roost made it leave earlier. When the calling never builds, the birds leave in dribs and drabs.
//!
//! What is made up: everything quantitative. Each bird here has a readiness to leave, between 0
//! and 1. It rises slowly with the light, it rises when the bird hears calling, and it fades when
//! there is nothing to hear. A bird calls more the readier it is. So calls make readiness and
//! readiness makes calls, and past a point the roost runs away with itself. The roost leaves when
//! the calling it hears passes a threshold. The constants are chosen for the shape of the curve
//! (a long slow build, then a steep one) and to fit an hour before dawn into under a minute. None
//! of them is measured, and the paper does not propose this mechanism: it reports the relation.
//!
//! Each bird is its own simulated airway ([`chimera::creatures::jackdaw`]) with its own size,
//! its own throat and its own version of the call. Nothing in the sound is a sample, and nothing
//! is a wing: jackdaws leaving a wood are loud with wings too, and a voice model has none.

use crate::{Ctx, FS, arg};
use chimera::creatures::jackdaw::{self, TCHAK_HZ, Tchak};
use chimera::creatures::types::{CallDef, CreatureDef};
use chimera::creatures::{CreatureVoice, creature_by_id};
use chimera::math::{PI, atan2, cos, exp, hypot, js_max, js_min, js_round, ln, sin, sqrt};
use std::time::Instant;

// ---- the vote ----------------------------------------------------------------------------------
/// simulation step (s)
const DT: f64 = 0.02;
/// calls a second from a bird that is fully ready
const RATE_MAX: f64 = 1.2;
/// readiness lost per second with nothing to hear
const DECAY: f64 = 0.25;
/// readiness gained per second, per call a second per bird heard
const CONTAGION: f64 = 0.5;
/// how long a call stays in the ear (s): the calling a bird "hears" is the roost's call rate
/// smoothed over this long
const HEAR_TAU: f64 = 2.5;
/// the calling (calls a second per bird) at which the roost goes
const THRESHOLD: f64 = 0.45;
/// readiness gained per second from the light alone: at the start, and how fast that grows
const LIGHT_START: f64 = 0.014;
const LIGHT_PER_SECOND: f64 = 0.0008;
/// calls a second from a bird that is awake and not ready at all (contact calls, which a roost
/// makes whatever it is deciding), and how long the roost takes to wake (s)
const WAKING_RATE: f64 = 0.008;
const WAKING: f64 = 2.5;
/// a bird's shortest gap between its own calls (s)
const REFRACTORY: f64 = 0.38;
/// every bird is in the air within this long of the decision (s): "less than five seconds"
const TAKEOFF_SPAN: f64 = 4.3;
/// flight speed (m/s) and the time to reach it (s)
const FLIGHT_SPEED: f64 = 13.0;
const FLIGHT_SPINUP: f64 = 1.2;
/// the flock's heading, as an angle from straight ahead (rad): away and to the right
const HEADING: f64 = 0.95;
/// calls a second from a bird that has just taken off, and how fast that fades (s)
const FLIGHT_RATE: f64 = 1.1;
const FLIGHT_RATE_TAU: f64 = 6.0;
/// the piece follows the flock for this long after the decision (s)
const FOLLOW: f64 = 20.0;
/// the roost must agree within this long, or the run is reported as a failed vote (s)
const PATIENCE: f64 = 150.0;

// ---- the place ---------------------------------------------------------------------------------
/// the jackdaws start this long into the piece (s)
const LEAD_IN: f64 = 3.0;
const SOUND_SPEED: f64 = 343.0;
/// a bird this far away plays at its rendered level (m)
const REF_DISTANCE: f64 = 9.0;
/// the wood's reverberation: decay time (s) and how bright it stays (Hz)
const RT60: f64 = 1.3;
const REVERB_DAMP_HZ: f64 = 3500.0;
const REVERB_LEVEL: f64 = 0.3;

/// splitmix64: the whole piece from one seed.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03)
    }
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn uniform(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.uniform()
    }
    fn normal(&mut self) -> f64 {
        let u = js_max(1e-12, self.uniform());
        let v = self.uniform();
        sqrt(-2.0 * ln(u)) * cos(2.0 * PI * v)
    }
}

fn clamp(x: f64, lo: f64, hi: f64) -> f64 {
    js_min(hi, js_max(lo, x))
}

/// One jackdaw: where it sits, what it is like, and what it has decided.
struct Bird {
    /// angle from straight ahead (rad, right positive) and distance (m) of its perch
    az: f64,
    dist: f64,
    size: f64,
    throat: f64,
    /// its mean pitch over the species' 709 Hz
    pitch: f64,
    /// how much it calls for a given readiness, and how much calling moves it
    chatty: f64,
    sway: f64,
    /// the chance that a call of its is of the noisy form
    noisy: f64,
    /// its versions of the call: the clear ones first, then `n_noisy` noisy ones
    shapes: Vec<Tchak>,
    n_noisy: usize,
    /// samples its voice idles before its first call (so that no two birds hiss alike)
    warm: usize,
    // the vote
    ready: f64,
    quiet_until: f64,
    takeoff: Option<f64>,
}

impl Bird {
    fn new(rng: &mut Rng) -> Self {
        let az = rng.range(-1.15, 1.15);
        // spread evenly over the ground of a wedge of wood, 9 to 48 m away
        let dist = sqrt(rng.range(9.0 * 9.0, 48.0 * 48.0));
        let size = clamp(1.0 + 0.04 * rng.normal(), 0.9, 1.1);
        // Szipl et al. 2025 give the spread over all calls (116 Hz of 709, 22 ms of 157) but not
        // how much of it lies between birds: about two thirds of the variance here, by choice
        let pitch = clamp(exp(0.13 * rng.normal()), 0.74, 1.35);
        let stretch = exp(0.1 * rng.normal());
        // the bird's own shape of call, its signature
        let own = Tchak {
            pitch: 1.0,
            stretch,
            start: 800.0 / 709.0 + 0.05 * rng.normal(),
            turn: 715.0 / 709.0 + 0.035 * rng.normal(),
            end: 580.0 / 709.0 + 0.05 * rng.normal(),
            turn_at: 0.56 + 0.06 * rng.normal(),
            noisy: false,
        };
        // the noisy form came from five of the nine birds, and was about one call in seven overall
        let noisy = if rng.uniform() < 0.55 { rng.range(0.1, 0.4) } else { 0.0 };
        let mut shapes = Vec::new();
        let version = |rng: &mut Rng, noisy: bool| Tchak {
            stretch: clamp(own.stretch * exp(0.09 * rng.normal()), 0.62, 1.5),
            start: own.start + 0.015 * rng.normal(),
            end: own.end + 0.015 * rng.normal(),
            noisy,
            ..own
        };
        for _ in 0..4 {
            shapes.push(version(rng, false));
        }
        let n_noisy = if noisy > 0.0 { 2 } else { 0 };
        for _ in 0..n_noisy {
            shapes.push(version(rng, true));
        }
        Bird {
            az,
            dist,
            size,
            throat: clamp(1.6 * exp(0.15 * rng.normal()), 0.8, 3.2),
            pitch,
            chatty: exp(0.35 * rng.normal()),
            sway: rng.range(0.7, 1.3),
            noisy,
            shapes,
            n_noisy,
            warm: (rng.uniform() * 4000.0) as usize,
            ready: rng.range(0.0, 0.08),
            quiet_until: 0.0,
            takeoff: None,
        }
    }

    /// Where the bird is at `t`: distance (m), angle (rad) and how fast it is moving away (m/s).
    fn place(&self, t: f64) -> (f64, f64, f64) {
        let Some(t0) = self.takeoff.filter(|&t0| t > t0) else {
            return (self.dist, self.az, 0.0);
        };
        let e = t - t0;
        let v = FLIGHT_SPEED * (1.0 - exp(-e / FLIGHT_SPINUP));
        let flown = FLIGHT_SPEED * (e - FLIGHT_SPINUP * (1.0 - exp(-e / FLIGHT_SPINUP)));
        let (dx, dy) = (sin(HEADING), cos(HEADING));
        let (x, y) = (self.dist * sin(self.az) + flown * dx, self.dist * cos(self.az) + flown * dy);
        let d = hypot(x, y);
        (d, atan2(x, y), v * (x * dx + y * dy) / d)
    }
}

/// One call, as the vote decided it.
struct Call {
    /// when it is given (s, from the jackdaws' start)
    t: f64,
    bird: usize,
    /// which of the bird's versions
    shape: usize,
    /// pitch over the species' mean, and breath
    pitch: f64,
    amp: f64,
    dist: f64,
    az: f64,
    /// speed away from the listener (m/s)
    away: f64,
}

struct Vote {
    birds: Vec<Bird>,
    calls: Vec<Call>,
    /// (t, calling heard: calls a second per bird) four times a second
    heard: Vec<(f64, f64)>,
    /// when the roost decided
    decided: f64,
}

fn vote(seed: u64, n: usize) -> Result<Vote, String> {
    let mut rng = Rng::new(seed);
    let mut birds: Vec<Bird> = (0..n).map(|_| Bird::new(&mut rng)).collect();
    let mut calls = Vec::new();
    let mut heard = Vec::new();
    let mut h = 0.0f64;
    let mut decided: Option<f64> = None;
    let mut step = 0usize;
    loop {
        let t = step as f64 * DT;
        if let Some(d) = decided {
            if t > d + FOLLOW {
                break;
            }
        } else if t > PATIENCE {
            return Err(format!("the roost never agreed: after {PATIENCE} s the calling was at {h:.2} calls a second per bird, {THRESHOLD} needed (seed {seed}, {n} birds)"));
        }
        let light = LIGHT_START + LIGHT_PER_SECOND * t;
        let mut given = 0usize;
        for (i, b) in birds.iter_mut().enumerate() {
            let flying = b.takeoff.is_some_and(|t0| t >= t0);
            let rate = if flying {
                b.chatty * FLIGHT_RATE * exp(-(t - b.takeoff.unwrap()) / FLIGHT_RATE_TAU)
            } else {
                if decided.is_some() {
                    b.ready = 1.0; // the decision is made: it is only waiting for its moment
                } else {
                    b.ready = clamp(b.ready + DT * (light + CONTAGION * b.sway * h - DECAY * b.ready), 0.0, 1.0);
                }
                b.chatty * (WAKING_RATE * js_min(1.0, t / WAKING) + RATE_MAX * b.ready * b.ready)
            };
            if t >= b.quiet_until && rng.uniform() < rate * DT {
                let at = t + rng.uniform() * DT;
                let (dist, az, away) = b.place(at);
                let n_clear = b.shapes.len() - b.n_noisy;
                let shape = if b.n_noisy > 0 && rng.uniform() < b.noisy { n_clear + (rng.next() as usize) % b.n_noisy } else { (rng.next() as usize) % n_clear };
                // a bird in the air calls a little higher and harder (a choice, not a measurement)
                let lift = if flying { 1.04 } else { 1.0 };
                calls.push(Call { t: at, bird: i, shape, pitch: b.pitch * lift * exp(0.08 * rng.normal()), amp: clamp(0.92 + 0.1 * rng.normal(), 0.7, 1.15) * lift, dist, az, away });
                b.quiet_until = at + REFRACTORY + 0.25 * rng.uniform();
                given += 1;
            }
        }
        h += DT * (-h / HEAR_TAU) + given as f64 / (n as f64 * HEAR_TAU);
        if decided.is_none() && h >= THRESHOLD {
            decided = Some(t);
            for b in birds.iter_mut() {
                b.takeoff = Some(t + TAKEOFF_SPAN * rng.uniform());
            }
        }
        if step % (0.25 / DT) as usize == 0 {
            heard.push((t, h));
        }
        step += 1;
    }
    Ok(Vote { birds, calls, heard, decided: decided.unwrap() })
}

// ---- the sound ---------------------------------------------------------------------------------

/// Every call of one bird, each in its own buffer (in the order of `mine`).
fn sing(bird: &Bird, mine: &[&Call]) -> Vec<Vec<f32>> {
    let calls = bird
        .shapes
        .iter()
        .map(|s| {
            let mut keys = Vec::new();
            jackdaw::tchak_keys(&mut keys, 0.0, s);
            CallDef::new("tchak", "Tchak", "", Some(TCHAK_HZ), keys)
        })
        .collect();
    // a creature definition lives as long as the program: one per bird, kept until exit
    let def: &'static CreatureDef = Box::leak(Box::new(jackdaw::def_with_calls(calls)));
    let initial = [(def.param_index("size").unwrap(), bird.size), (def.param_index("oec").unwrap(), bird.throat)];
    let mut v = CreatureVoice::new(def, FS, &initial);
    let mut idle = vec![0f32; bird.warm];
    v.render(&mut idle);
    mine.iter()
        .map(|c| {
            v.play_call(c.shape, Some(TCHAK_HZ * c.pitch), c.amp);
            let seconds = def.calls[c.shape].keys.last().unwrap().t + 0.14;
            let mut buf = vec![0f32; (seconds * FS) as usize];
            for block in buf.chunks_mut(128) {
                v.render(block);
            }
            fade_out(&mut buf, 0.01);
            buf
        })
        .collect()
}

fn fade_out(x: &mut [f32], seconds: f64) {
    let n = ((seconds * FS) as usize).min(x.len());
    let from = x.len() - n;
    for i in 0..n {
        x[from + i] *= 1.0 - (i as f32 + 1.0) / n as f32;
    }
}

/// The dry stereo field and the send to the wood's reverberation.
struct Field {
    l: Vec<f32>,
    r: Vec<f32>,
    send: Vec<f32>,
}

impl Field {
    fn new(seconds: f64) -> Self {
        let n = (seconds * FS) as usize;
        Field { l: vec![0.0; n], r: vec![0.0; n], send: vec![0.0; n] }
    }

    /// Put a sound made at `t` (s) by a source `dist` m away, at angle `az`, moving away at
    /// `away` m/s: the Doppler shift, the air and the leaves in between, the time the sound takes
    /// to arrive, and where it is heard from.
    fn place(&mut self, x: &[f32], t: f64, dist: f64, az: f64, away: f64) {
        // Doppler: a receding source is heard lower and longer by c / (c + v)
        let rate = SOUND_SPEED / (SOUND_SPEED + away);
        let n = (x.len() as f64 / rate) as usize;
        // distance: quieter by 1/d, and duller (one pole, lower the farther)
        let gain = REF_DISTANCE / js_max(REF_DISTANCE * 0.6, dist);
        let a = 1.0 - exp(-2.0 * PI * (12000.0 / (1.0 + dist / 25.0)) / FS);
        // direction: level between the ears, and the time between them
        let angle = (clamp(az / 1.3, -1.0, 1.0) + 1.0) * PI / 4.0;
        let (gl, gr) = (cos(angle) * gain, sin(angle) * gain);
        let itd = js_round(0.00066 * sin(az) * FS) as i64;
        let start = js_round((t + dist / SOUND_SPEED) * FS) as i64;
        let (start_l, start_r) = (start + itd.max(0), start + (-itd).max(0));
        let wet = (0.25 + 0.75 * js_min(1.0, dist / 60.0)) * gain;
        let mut lp = 0.0f64;
        for i in 0..n {
            let p = i as f64 * rate;
            let k = p as usize;
            if k + 1 >= x.len() {
                break;
            }
            let u = p - k as f64;
            let s = x[k] as f64 * (1.0 - u) + x[k + 1] as f64 * u;
            lp += a * (s - lp);
            let (il, ir) = (start_l + i as i64, start_r + i as i64);
            if il >= 0 && (il as usize) < self.l.len() {
                self.l[il as usize] += (lp * gl) as f32;
            }
            if ir >= 0 && (ir as usize) < self.r.len() {
                self.r[ir as usize] += (lp * gr) as f32;
            }
            let im = start + i as i64;
            if im >= 0 && (im as usize) < self.send.len() {
                self.send[im as usize] += (lp * wet) as f32;
            }
        }
    }
}

/// A small feedback delay network: eight delay lines through a Householder mixing matrix, each
/// losing level and brightness on the way round. Trunks scatter sound for a long, dull tail.
struct Wood {
    lines: Vec<Vec<f64>>,
    pos: [usize; 8],
    lp: [f64; 8],
    gain: [f64; 8],
    damp: f64,
}

const WOOD_DELAYS: [usize; 8] = [1447, 1723, 1999, 2311, 2671, 3011, 3407, 3833];

impl Wood {
    fn new() -> Self {
        let mut gain = [0.0; 8];
        for (g, d) in gain.iter_mut().zip(WOOD_DELAYS) {
            *g = exp(ln(0.001) * d as f64 / (RT60 * FS)); // −60 dB after RT60
        }
        Wood { lines: WOOD_DELAYS.iter().map(|&d| vec![0.0; d]).collect(), pos: [0; 8], lp: [0.0; 8], gain, damp: 1.0 - exp(-2.0 * PI * REVERB_DAMP_HZ / FS) }
    }

    fn step(&mut self, x: f64) -> (f64, f64) {
        let mut o = [0.0f64; 8];
        for i in 0..8 {
            self.lp[i] += self.damp * (self.lines[i][self.pos[i]] - self.lp[i]);
            o[i] = self.lp[i] * self.gain[i];
        }
        let mean2 = o.iter().sum::<f64>() * (2.0 / 8.0);
        for i in 0..8 {
            let input = if i % 2 == 0 { x } else { -x };
            self.lines[i][self.pos[i]] = input + o[i] - mean2;
            self.pos[i] = (self.pos[i] + 1) % WOOD_DELAYS[i];
        }
        (o[0] - o[2] + o[4] - o[6], o[1] - o[3] + o[5] - o[7])
    }
}

/// A phrase from the bestiary's songbird, far off (its `flute` call; `pitch` in Hz).
fn songbird_phrase(pitch: f64) -> Vec<f32> {
    let def = creature_by_id("songbird").expect("the songbird");
    let mut v = CreatureVoice::new(def, FS, &[]);
    v.play_call_id("flute", Some(pitch), 0.9);
    let mut buf = vec![0f32; (1.7 * FS) as usize];
    for block in buf.chunks_mut(128) {
        v.render(block);
    }
    fade_out(&mut buf, 0.02);
    buf
}

fn write_stereo(path: &str, l: &[f32], r: &[f32]) -> std::io::Result<()> {
    let data_bytes = (l.len() * 4) as u32;
    let mut b = Vec::with_capacity(44 + data_bytes as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&2u16.to_le_bytes()); // stereo
    b.extend_from_slice(&(FS as u32).to_le_bytes());
    b.extend_from_slice(&(FS as u32 * 4).to_le_bytes());
    b.extend_from_slice(&4u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_bytes.to_le_bytes());
    for (x, y) in l.iter().zip(r) {
        for v in [x, y] {
            let v = (*v as f64).clamp(-1.0, 1.0);
            b.extend_from_slice(&(js_round(v * 32767.0) as i16).to_le_bytes());
        }
    }
    if let Some(dir) = std::path::Path::new(path).parent()
        && !dir.as_os_str().is_empty()
    {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, b)
}

/// `render roost [out] [seed] [birds] [--dry] [--jobs n]`: `<out>.wav` (stereo) and `<out>.json`
/// (the vote: every call, the calling over time, the moment of the decision). `--dry` runs the
/// vote without the sound and prints it.
pub fn run(_ctx: &Ctx, a: &[String]) -> Result<(), String> {
    let mut dry = false;
    let mut jobs = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let mut rest = Vec::new();
    let mut it = a.iter();
    while let Some(x) = it.next() {
        if x == "--dry" {
            dry = true;
        } else if x == "--jobs" {
            jobs = it.next().and_then(|n| n.parse().ok()).filter(|&n| n > 0).ok_or("--jobs needs a number")?;
        } else {
            rest.push(x.clone());
        }
    }
    let a = &rest[..];
    let out = arg(a, 0, "out/roost/roost");
    let out = out.strip_suffix(".wav").unwrap_or(out);
    let seed: u64 = arg(a, 1, "1").parse().map_err(|_| "bad seed")?;
    let n: usize = arg(a, 2, "90").parse().map_err(|_| "bad bird count")?;
    if n < 2 {
        return Err("a roost needs at least two birds".into());
    }

    let v = vote(seed, n)?;
    let noisy = v.calls.iter().filter(|c| c.shape >= v.birds[c.bird].shapes.len() - v.birds[c.bird].n_noisy).count();
    let before = v.calls.iter().filter(|c| c.t < v.decided).count();
    println!("{n} birds, seed {seed}: {} calls ({noisy} noisy), {before} before the roost decided at {:.1} s", v.calls.len(), v.decided);
    // the calling over time: calls in each 2 s, and what the roost heard at the end of them
    let bins = (v.decided + FOLLOW) as usize / 2 + 1;
    let mut count = vec![0usize; bins];
    for c in &v.calls {
        count[(c.t / 2.0) as usize] += 1;
    }
    for (i, k) in count.iter().enumerate() {
        let t = (i + 1) as f64 * 2.0;
        let h = v.heard.iter().rev().find(|(ht, _)| *ht <= t).map_or(0.0, |x| x.1);
        let mark = if (v.decided / 2.0) as usize == i { "  <- decided" } else { "" };
        println!("  {:>5.0} s  {k:>4} calls  heard {h:.3}  {}{mark}", t - 2.0, "#".repeat(k / 4));
    }
    if dry {
        return Ok(());
    }

    // every bird sings its own calls, the birds shared out over the cores
    let started = Instant::now();
    let mut per_bird: Vec<Vec<&Call>> = vec![Vec::new(); n];
    for c in &v.calls {
        per_bird[c.bird].push(c);
    }
    let per_bird = &per_bird;
    let birds = &v.birds;
    let mut sung: Vec<(usize, Vec<Vec<f32>>)> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..jobs)
            .map(|j| s.spawn(move || (j..n).step_by(jobs).map(|i| (i, sing(&birds[i], &per_bird[i]))).collect::<Vec<_>>()))
            .collect();
        hs.into_iter().flat_map(|h| h.join().expect("a bird's render")).collect()
    });
    // mixed in the birds' order, whatever the thread count: the same seed is the same sound
    sung.sort_by_key(|(i, _)| *i);
    let seconds = LEAD_IN + v.decided + FOLLOW + 14.0;
    let mut field = Field::new(seconds);
    let mut sung_seconds = 0.0;
    for (i, bufs) in &sung {
        for (c, x) in per_bird[*i].iter().zip(bufs) {
            field.place(x, LEAD_IN + c.t, c.dist, c.az, c.away);
            sung_seconds += x.len() as f64 / FS;
        }
    }
    // the songbird: once in the dark before the jackdaws wake, twice in the quiet they leave
    let gone = LEAD_IN + v.decided + FOLLOW;
    // (when, pitch, distance, angle)
    let songbird = [(0.8, 2450.0, 52.0, -0.95), (gone + 1.5, 2600.0, 52.0, -0.95), (gone + 6.5, 2300.0, 52.0, -0.95)];
    for (t, pitch, dist, az) in songbird {
        field.place(&songbird_phrase(pitch), t, dist, az, 0.0);
    }
    // the wood
    let mut wood = Wood::new();
    let predelay = (0.025 * FS) as usize;
    for i in 0..field.l.len() {
        let x = if i >= predelay { field.send[i - predelay] as f64 } else { 0.0 };
        let (wl, wr) = wood.step(x);
        field.l[i] += (REVERB_LEVEL * wl) as f32;
        field.r[i] += (REVERB_LEVEL * wr) as f32;
    }
    // the loudest moment at −1 dB
    let top = field.l.iter().chain(&field.r).fold(0.0f32, |p, x| p.max(x.abs()));
    let k = 0.89 / top.max(1e-9);
    for x in field.l.iter_mut().chain(field.r.iter_mut()) {
        *x *= k;
    }
    fade_out(&mut field.l, 1.5);
    fade_out(&mut field.r, 1.5);
    let wav = format!("{out}.wav");
    write_stereo(&wav, &field.l, &field.r).map_err(|e| format!("{wav}: {e}"))?;
    println!("{wav}: {seconds:.1} s; {sung_seconds:.0} s of calls sung in {:.1} s on {jobs} threads; peak before levelling {top:.2}", started.elapsed().as_secs_f64());

    let r3 = |x: f64| js_round(x * 1000.0) / 1000.0;
    let score = serde_json::json!({
        "seed": seed,
        "seconds": r3(seconds),
        "leadIn": LEAD_IN,
        "decided": r3(v.decided),
        "threshold": THRESHOLD,
        "takeoffSpan": TAKEOFF_SPAN,
        "follow": FOLLOW,
        "birds": v.birds.iter().map(|b| serde_json::json!({
            "az": r3(b.az), "dist": r3(b.dist), "size": r3(b.size), "pitch": r3(b.pitch), "noisy": r3(b.noisy), "takeoff": b.takeoff.map(r3),
        })).collect::<Vec<_>>(),
        // [t, bird, noisy, distance]
        "calls": v.calls.iter().map(|c| {
            let b = &v.birds[c.bird];
            serde_json::json!([r3(c.t), c.bird, (c.shape >= b.shapes.len() - b.n_noisy) as u8, r3(c.dist)])
        }).collect::<Vec<_>>(),
        // [t, calls a second per bird heard]
        "heard": v.heard.iter().map(|(t, h)| serde_json::json!([r3(*t), r3(*h)])).collect::<Vec<_>>(),
        // the songbird's phrases: [t in the piece, distance, angle]
        "songbird": songbird.iter().map(|(t, _, dist, az)| serde_json::json!([r3(*t), dist, az])).collect::<Vec<_>>(),
    });
    let json = format!("{out}.json");
    std::fs::write(&json, score.to_string()).map_err(|e| format!("{json}: {e}"))?;
    println!("{json}: the vote");
    Ok(())
}
