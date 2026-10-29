"""Turn the raw ElevenLabs SFX (tools/sfx/raw/*.mp3) into game-ready clips (assets/sfx/*.ogg):
cut leading silence, cap the length, fade out, loudness-normalise, mono Vorbis.

Loudness: every clip is brought to the same loudest-moment level (max momentary loudness, 400 ms
window, TARGET_LUFS) with a -1 dBFS limiter, so the per-sound volumes in crates/pw_game/src/sfx.rs alone
set the mix. Peak normalising (the old way) left 20+ dB between a short click and a long chime.

Hand-edited clips in tools/sfx/hand/<name>.ogg are copied unchanged (the user's own edit wins).

    python -I tools/sfx/process.py
"""
import pathlib, re, shutil, subprocess

ROOT = pathlib.Path(__file__).resolve().parents[2]
RAW = ROOT / "tools/sfx/raw"
HAND = ROOT / "tools/sfx/hand"
OUT = ROOT / "assets/sfx"
TARGET_LUFS = -21.0

# Longest useful part of each clip in seconds (the generator pads most clips to 1-6 s).
MAX = {
    "ui_click": 0.25, "ui_open": 0.75, "swing": 0.5, "hit": 0.6, "creature_cry": 0.9, "pickaxe": 0.7, "axe": 0.7,
    "node_break": 1.2, "pickup": 0.6, "throw": 0.8, "shake": 0.6, "capture": 2.5, "breakfree": 0.9,
    "build_place": 0.9, "build_done": 1.8, "craft_done": 1.0, "levelup": 2.0, "step": 0.35, "glide": 1.2,
    "splash": 1.0, "summon": 1.4, "eat": 1.0,
}


def run(args):
    return subprocess.run(args, capture_output=True, text=True, check=True)


def max_momentary(src, af):
    """Loudest 400 ms of the clip (LUFS); silence padding so short clips fill a window."""
    err = run(["ffmpeg", "-v", "info", "-i", str(src), "-af", f"{af},apad=pad_dur=0.5,ebur128", "-f", "null", "-"]).stderr
    return max(float(x) for x in re.findall(r"M:\s*(-?[\d.]+)", err))


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    for src in sorted(RAW.glob("*.mp3")):
        name = src.stem
        dst = OUT / f"{name}.ogg"
        hand = HAND / f"{name}.ogg"
        if hand.exists():
            shutil.copyfile(hand, dst)
            print(f"{name:12} hand-edited, copied unchanged")
            continue
        cap = MAX.get(name, 1.0)
        fade = min(0.15, cap * 0.3)
        af = ",".join([
            "silenceremove=start_periods=1:start_threshold=-45dB:start_silence=0.01",
            f"atrim=0:{cap}",
            f"afade=t=out:st={cap - fade:.3f}:d={fade:.3f}",
        ])
        gain = TARGET_LUFS - max_momentary(src, af)
        run(["ffmpeg", "-y", "-v", "error", "-i", str(src), "-af", f"{af},volume={gain:.2f}dB,alimiter=limit=0.89:level=false", "-ac", "1", "-ar", "44100", "-c:a", "libvorbis", "-q:a", "5", str(dst)])
        print(f"{name:12} gain {gain:+6.1f} dB -> {max_momentary(dst, 'anull'):6.1f} LUFS  {dst.stat().st_size} B")


if __name__ == "__main__":
    main()
