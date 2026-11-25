// The Roost Vote page: `bun tools/roost-page/build.ts` after
//   chimera render roost out/roost/roost
//   chimera render creature jackdaw out/creatures
// Writes out/roost/page/: roost-vote.html (the vote inlined from out/roost/roost.json), the piece
// and the two single calls as MP3 (ffmpeg), and the calls' spectrograms.
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { readWav } from "../lib/wav.ts";
import { renderSpectrogram } from "../lib/spectrogram.ts";

const root = new URL("../../", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");
const out = `${root}out/roost/page`;
mkdirSync(out, { recursive: true });

// The clear call as `cargo test -p chimera --test jackdaw -- --nocapture` measures it from the
// sound (the_tchak_is_as_long_and_as_high_as_a_real_one prints these).
const MEASURED = { ms: "163", hz: "709", thirds: "772 / 722 / 629" };

function ffmpeg(args: string[]) {
  const r = spawnSync("ffmpeg", ["-hide_banner", "-loglevel", "error", "-y", ...args], { stdio: "inherit" });
  if (r.status !== 0) throw new Error(`ffmpeg ${args.join(" ")} failed`);
}

// the piece
ffmpeg(["-i", `${root}out/roost/roost.wav`, "-codec:a", "libmp3lame", "-b:a", "192k", `${out}/roost.mp3`]);

// the single calls, cut from the creature render: a 1.2 s held note and 0.4 s of rest, then each
// call in turn (the first lasts its 0.152 s and the renderer's 0.5 s after it)
const creature = `${root}out/creatures/jackdaw.wav`;
for (const [name, start, title] of [["tchak", 1.55, "clear"], ["noisy", 2.2, "rough"]] as const) {
  const wav = `${out}/${name}.wav`;
  ffmpeg(["-ss", String(start), "-t", "0.32", "-i", creature, wav]);
  // louder on its own than in the flock's render, with a little room so players don't clip its start
  ffmpeg(["-i", wav, "-af", "adelay=120,apad=pad_dur=0.25,volume=2.2", "-codec:a", "libmp3lame", "-b:a", "160k", `${out}/${name}.mp3`]);
  const { sampleRate, channels } = readWav(wav);
  renderSpectrogram(`${out}/${name}.png`, channels[0], sampleRate, { title, maxFreq: 8000, windowMs: 8 });
}

const score = readFileSync(`${root}out/roost/roost.json`, "utf8");
const page = readFileSync(`${root}tools/roost-page/page.html`, "utf8")
  .replace("__SCORE__", () => score)
  .replace("__CLEAR_MS__", MEASURED.ms)
  .replace("__CLEAR_HZ__", MEASURED.hz)
  .replace("__CLEAR_THIRDS__", MEASURED.thirds);
if (/__[A-Z_]+__/.test(page.replace(score, ""))) throw new Error("a placeholder was left in the page");
writeFileSync(`${out}/roost-vote.html`, page);
console.log(`${out}/roost-vote.html  ${(page.length / 1024).toFixed(0)} KB`);
