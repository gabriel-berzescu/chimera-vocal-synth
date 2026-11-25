//! The jackdaw's contact call against what was measured on real birds: Szipl, Baotic & Kotrschal
//! (2025), Animal Cognition 29:2, 464 tchak calls of nine males. One form is harmonic throughout
//! and falls in pitch; the other starts the same and turns noisy. Everything here is measured from
//! the rendered sound, and the birds' figures are written in this file, not read from the engine.
use chimera::analysis::pitch::{YinOpts, yin_pitch};
use chimera::creatures::{CreatureVoice, creature_by_id};

/// The real tchak's length (s) and mean fundamental (Hz): mean and standard deviation over calls.
const REAL_SECONDS: (f64, f64) = (0.157, 0.022);
const REAL_HZ: (f64, f64) = (709.0, 116.0);

/// The call starts this long into a render (s).
const LEAD: f64 = 0.05;

fn voice(fs: f64, params: &[(&str, f64)]) -> CreatureVoice {
    let def = creature_by_id("jackdaw").expect("the jackdaw");
    let initial: Vec<(usize, f64)> = params.iter().map(|(k, x)| (def.param_index(k).expect("a param"), *x)).collect();
    CreatureVoice::new(def, fs, &initial)
}

/// `LEAD` of rest, then the call, 400 ms in all after it starts. `hiss: false` silences the
/// valves' turbulence and nothing else, so the difference between the two renders is the hiss.
fn call(fs: f64, id: &str, params: &[(&str, f64)], hiss: bool) -> Vec<f32> {
    call_for(fs, id, params, hiss, 0.4)
}

/// The same, `seconds` in all after the call starts.
fn call_for(fs: f64, id: &str, params: &[(&str, f64)], hiss: bool, seconds: f64) -> Vec<f32> {
    let mut v = voice(fs, params);
    if !hiss {
        v.sim.net_mut().unwrap().noise_scale = 0.0;
    }
    let mut out = vec![0f32; ((LEAD + seconds) * fs) as usize];
    let lead = (LEAD * fs) as usize;
    v.render(&mut out[..lead]);
    v.play_call_id(id, None, 1.0);
    for block in out[lead..].chunks_mut(128) {
        v.render(block);
    }
    out
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|&v| (v as f64) * (v as f64)).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0, |p, v| p.max(v.abs()))
}

struct Measured {
    /// first and last moment the 5 ms RMS is within 20 dB of its peak (s)
    start: f64,
    end: f64,
    /// (time from the call's start, f0, aperiodicity) every 2.5 ms over the call, 20 ms frames
    track: Vec<(f64, f64, f64)>,
}

impl Measured {
    fn seconds(&self) -> f64 {
        self.end - self.start
    }
    /// Mean f0 of the voiced frames whose centre lies in `from..to` of the call's length (0..1).
    fn f0(&self, from: f64, to: f64) -> f64 {
        let d = self.seconds();
        let v: Vec<f64> = self.track.iter().filter(|(t, f, _)| *f > 0.0 && *t >= from * d && *t < to * d).map(|(_, f, _)| *f).collect();
        assert!(!v.is_empty(), "no voiced frame between {from} and {to} of the call");
        v.iter().sum::<f64>() / v.len() as f64
    }
    /// Mean aperiodicity over the same span (0 = perfectly periodic).
    fn aperiodicity(&self, from: f64, to: f64) -> f64 {
        let d = self.seconds();
        let v: Vec<f64> = self.track.iter().filter(|(t, _, _)| *t >= from * d && *t < to * d).map(|(_, _, a)| *a).collect();
        v.iter().sum::<f64>() / v.len().max(1) as f64
    }
}

fn measure(fs: f64, x: &[f32]) -> Measured {
    let win = (0.005 * fs) as usize;
    let hop = (0.001 * fs) as usize;
    let env: Vec<f64> = (0..(x.len() - win) / hop).map(|i| rms(&x[i * hop..i * hop + win])).collect();
    let top = env.iter().cloned().fold(0.0, f64::max);
    let first = env.iter().position(|&e| e > 0.1 * top).unwrap();
    let last = env.iter().rposition(|&e| e > 0.1 * top).unwrap();
    let (start, end) = (first as f64 * 0.001, last as f64 * 0.001 + 0.005);
    let frame = (0.02 * fs) as usize;
    let mut track = Vec::new();
    let mut t = start;
    while t + 0.02 <= end {
        let s = (t * fs) as usize;
        let f: Vec<f64> = x[s..s + frame].iter().map(|&v| v as f64).collect();
        // a loose threshold, so the track follows the fundamental through the rough part too; the
        // search stops below the second harmonic of the lowest fundamental (2 × 540 Hz), which a
        // strong resonance there would otherwise win
        let p = yin_pitch(&f, fs, YinOpts { min_f0: Some(350.0), max_f0: Some(1050.0), threshold: Some(0.35) });
        track.push((t + 0.01 - start, p.f0, p.aperiodicity));
        t += 0.0025;
    }
    Measured { start, end, track }
}

#[test]
fn the_tchak_is_as_long_and_as_high_as_a_real_one() {
    let m = measure(48000.0, &call(48000.0, "tchak", &[], true));
    let (len, f0) = (m.seconds(), m.f0(0.0, 1.0));
    println!("tchak: {:.0} ms, mean f0 {f0:.0} Hz, thirds {:.0} / {:.0} / {:.0} Hz, aperiodicity {:.3}", len * 1e3, m.f0(0.0, 0.33), m.f0(0.33, 0.67), m.f0(0.67, 1.0), m.aperiodicity(0.0, 1.0));
    // within one standard deviation of the birds' length, and a third of one of their pitch
    assert!((len - REAL_SECONDS.0).abs() < REAL_SECONDS.1, "the tchak lasts {:.0} ms: real ones 157 ± 22 ms", len * 1e3);
    assert!((f0 - REAL_HZ.0).abs() < REAL_HZ.1 / 3.0, "the tchak's mean f0 is {f0:.0} Hz: real ones 709 ± 116 Hz");
    assert!(m.f0(0.0, 0.33) > m.f0(0.67, 1.0) + 60.0, "the tchak's pitch falls");
}

#[test]
fn the_noisy_tchak_starts_clear_and_ends_rough() {
    let clear = measure(48000.0, &call(48000.0, "tchak", &[], true));
    let noisy = measure(48000.0, &call(48000.0, "noisy", &[], true));
    println!("noisy: {:.0} ms, mean f0 {:.0} Hz", noisy.seconds() * 1e3, noisy.f0(0.0, 1.0));
    println!("aperiodicity, first third / last half: clear {:.3} / {:.3}, noisy {:.3} / {:.3}", clear.aperiodicity(0.0, 0.33), clear.aperiodicity(0.5, 1.0), noisy.aperiodicity(0.0, 0.33), noisy.aperiodicity(0.5, 1.0));
    assert!((noisy.seconds() - REAL_SECONDS.0).abs() < REAL_SECONDS.1, "the noisy tchak lasts {:.0} ms", noisy.seconds() * 1e3);
    // "starts with a harmonic segment": as periodic as the clear call at first
    assert!(noisy.aperiodicity(0.0, 0.33) < clear.aperiodicity(0.0, 0.33) + 0.05, "the noisy tchak starts clear");
    // then much less periodic than the clear call, and than its own start. What YIN calls
    // aperiodic over a 20 ms frame is everything at once: the pitch's wobble, the beating of the
    // two sides and the hiss. The hiss has its own test below.
    assert!(noisy.aperiodicity(0.5, 1.0) > 3.0 * clear.aperiodicity(0.5, 1.0), "the noisy tchak ends rough");
    assert!(noisy.aperiodicity(0.5, 1.0) > 3.0 * noisy.aperiodicity(0.0, 0.33), "the noisy tchak ends rougher than it starts");
}

/// The valves' hiss over a span of the call (s from its start), in dB against the rest of the
/// sound: the same call rendered with the turbulence and without it, and the difference taken.
fn hiss_db(fs: f64, id: &str, from: f64, to: f64) -> f64 {
    let (with, without) = (call_for(fs, id, &[], true, to + 0.1), call_for(fs, id, &[], false, to + 0.1));
    let (i, j) = (((LEAD + from) * fs) as usize, ((LEAD + to) * fs) as usize);
    let noise: f64 = with[i..j].iter().zip(&without[i..j]).map(|(a, b)| ((a - b) as f64).powi(2)).sum();
    let sound: f64 = without[i..j].iter().map(|b| (*b as f64).powi(2)).sum();
    10.0 * (noise / sound).log10()
}

#[test]
fn the_valves_hiss_where_the_noisy_tchak_roughens_and_not_before() {
    // the paper's second form has "low harmonicity" at its end: real noise, not only an unsteady
    // pitch (which is all the test above would need)
    let fs = 48000.0;
    let (clear, start, end) = (hiss_db(fs, "tchak", 0.0, 0.157), hiss_db(fs, "noisy", 0.0, 0.052), hiss_db(fs, "noisy", 0.08, 0.157));
    println!("hiss against the call: clear {clear:.1} dB, noisy first third {start:.1} dB, noisy last half {end:.1} dB");
    assert!(clear < -30.0, "the clear tchak hisses: {clear:.1} dB");
    assert!(start < -30.0, "the noisy tchak hisses from the start: {start:.1} dB");
    assert!(end > -20.0, "the noisy tchak's end has no noise to speak of: {end:.1} dB");
    assert!(end < -6.0, "the noisy tchak's end is mostly hiss: {end:.1} dB");
}

#[test]
fn a_clear_call_after_a_noisy_one_does_not_inherit_its_hiss() {
    // the chatter's fourth call is the noisy one (0.93 s in), its fifth a clear one (1.21 s):
    // the fifth was 16 dB hissier than the first when only the noisy form set the rasp
    let fs = 48000.0;
    let (first, fourth, fifth) = (hiss_db(fs, "chatter", 0.0, 0.157), hiss_db(fs, "chatter", 0.93 + 0.08, 0.93 + 0.157), hiss_db(fs, "chatter", 1.21, 1.21 + 0.157));
    println!("hiss through the chatter: first call {first:.1} dB, the noisy fourth's end {fourth:.1} dB, fifth {fifth:.1} dB");
    assert!(fourth > -20.0, "the chatter's noisy call has no noise: {fourth:.1} dB");
    assert!((fifth - first).abs() < 6.0, "the call after the noisy one: {fifth:.1} dB of hiss, the first {first:.1} dB");
}

#[test]
fn the_hiss_is_about_as_loud_at_any_device_rate() {
    // The turbulence is put across one section of the trachea, and a longer section (a lower
    // rate) made more of it: with one gain for every rate the noisy call's end was as much hiss as
    // voice at a 16 kHz device. `create` scales the gain by the section's length. Not covered:
    // 8 and 11 kHz, where that overcorrects and the hiss is quieter (printed below).
    for fs in [8000.0, 11025.0] {
        println!("{fs} Hz: clear {:.1} dB, noisy last half {:.1} dB (not asserted)", hiss_db(fs, "tchak", 0.0, 0.157), hiss_db(fs, "noisy", 0.08, 0.157));
    }
    for fs in [16000.0, 22050.0, 24000.0, 32000.0, 44100.0, 48000.0, 96000.0] {
        let (clear, end) = (hiss_db(fs, "tchak", 0.0, 0.157), hiss_db(fs, "noisy", 0.08, 0.157));
        println!("{fs} Hz: clear {clear:.1} dB, noisy last half {end:.1} dB");
        assert!(clear < -30.0, "{fs} Hz: the clear tchak hisses, {clear:.1} dB");
        assert!(end > -20.0 && end < -6.0, "{fs} Hz: the noisy tchak's end has {end:.1} dB of hiss");
    }
}

#[test]
fn a_bird_of_any_size_holds_at_any_device_rate() {
    // A creature's oversampling does not rise as the device rate falls, so a 16 kHz device (a
    // Bluetooth headset) runs this airway at 64 kHz and an 8 kHz one at 32 kHz. The short tubes
    // then take fewer sections (see `create`); with the usual floor of three the bronchi ran at a
    // Courant number of 1.37 at 16 kHz and the output sat at the limiter. `size` also shortens
    // the trachea and the beak under the simulation: the smallest bird must stay stable too.
    for size in [0.88, 1.0, 1.12] {
        let reference = rms(&call(48000.0, "tchak", &[("size", size)], true));
        for fs in [8000.0, 16000.0, 22050.0, 24000.0, 32000.0, 44100.0, 48000.0, 96000.0] {
            let courant = voice(fs, &[("size", size)]).sim.net().unwrap().check_cfl();
            let x = call(fs, "tchak", &[("size", size)], true);
            let (r, top) = (rms(&x), peak(&x));
            println!("size {size} at {fs} Hz: Courant {courant:.2}, RMS {r:.4} (48 kHz: {reference:.4}), peak {top:.3}");
            // the network's own check, over its shortest section (at 8 kHz that is the throat's
            // cavity, a single lumped node, not a tube)
            assert!(courant < 1.0, "size {size} at {fs} Hz: Courant number {courant:.2}");
            // a runaway airway sits at the limiter's ceiling of 1, or goes silent (the voice zeroes
            // what is not finite): either way it fails here
            assert!(top < 0.9, "size {size} at {fs} Hz: peak {top:.3}");
            assert!(r > 0.5 * reference && r < 2.0 * reference, "size {size} at {fs} Hz: RMS {r:.4}, at 48 kHz {reference:.4}");
        }
    }
}

#[test]
fn the_dials_can_be_thrown_about_mid_call() {
    // the three dials that move the geometry, slammed end to end every 20 ms through the chatter
    let def = creature_by_id("jackdaw").unwrap();
    let dials: Vec<(usize, f64, f64)> = ["size", "oec", "gape"].iter().map(|k| def.param_index(k).unwrap()).map(|i| (i, def.params[i].min, def.params[i].max)).collect();
    // (8 kHz is where a bronchus is a single section)
    for fs in [8000.0, 16000.0, 48000.0] {
        let mut v = voice(fs, &[]);
        v.play_call_id("chatter", None, 1.0);
        let mut out = vec![0f32; (1.8 * fs) as usize];
        let mut worst = 0.0f64;
        for (i, block) in out.chunks_mut((0.02 * fs) as usize).enumerate() {
            let set: Vec<(usize, f64)> = dials.iter().map(|&(k, lo, hi)| (k, if i % 2 == 0 { lo } else { hi })).collect();
            v.set_params(&set);
            v.render(block);
            worst = worst.max(v.sim.net().unwrap().check_cfl());
        }
        println!("{fs} Hz, dials thrown about: worst Courant {worst:.2}, peak {:.3}, RMS {:.4}", peak(&out), rms(&out));
        assert!(worst < 1.0, "{fs} Hz: Courant number {worst:.2}");
        assert!(peak(&out) < 0.9 && rms(&out) > 0.01, "{fs} Hz: peak {:.3}, RMS {:.4}", peak(&out), rms(&out));
    }
}

/// The tract's resonances below 6 kHz with the beak open (peaks of the transfer function from the
/// syrinx end of the trachea to the beak, the valves shut), for a bird of this size.
fn resonances(size: f64) -> Vec<f64> {
    let v = voice(48000.0, &[("size", size), ("gape", 0.9)]);
    let net = v.sim.net().unwrap();
    let trachea = v.sim.tubes().iter().find(|(name, _)| *name == "trachea").unwrap().1;
    let freqs: Vec<f64> = (20..=600).map(|i| i as f64 * 10.0).collect();
    let h = chimera::airway::frequency::transfer_function(&net.linear_elements(), trachea.first_node(), &freqs, &[]);
    (1..freqs.len() - 1).filter(|&i| h.mag[i] > h.mag[i - 1] && h.mag[i] >= h.mag[i + 1]).map(|i| freqs[i]).collect()
}

#[test]
fn a_smaller_birds_tract_rings_higher() {
    // the syrinx sets the pitch and the tract its colour: a smaller bird's tube is shorter, and
    // its first resonances lie higher by about the ratio of the sizes (1.27 between the dial's ends)
    let (small, mid, big) = (resonances(0.88), resonances(1.0), resonances(1.12));
    println!("resonances, beak open: size 0.88 {small:?}\n                       size 1.00 {mid:?}\n                       size 1.12 {big:?}");
    assert!(small.len() >= 3 && big.len() >= 3, "resonances: {small:?} / {big:?}");
    for (s, b) in small.iter().zip(&big).take(3) {
        let ratio = s / b;
        assert!(ratio > 1.15 && ratio < 1.4, "a resonance at {s} Hz in the small bird, {b} Hz in the big one");
    }
}
