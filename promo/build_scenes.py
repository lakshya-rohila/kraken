#!/usr/bin/env python3
"""Writes compositions/*.html for the Kraken reel.

Every scene shares one style sheet and one helper script (camera punch-ins,
flash frames, shake, per-word title slams), so the edit grammar stays the same
across the film. Times are written in beats: b(k) = k × 0.4405 s (136.2 BPM).
Re-running overwrites the scene files.
"""
import pathlib

BEAT = 0.4405
OUT = pathlib.Path(__file__).parent / "compositions"
OUT.mkdir(exist_ok=True)


def b(k):
    return round(k * BEAT, 3)


CSS = """
#root{position:absolute;inset:0;overflow:hidden;background:#06070c;font-family:"Archivo Black",sans-serif;color:#f2eeff}
.shot{position:absolute;inset:0;overflow:hidden}
.shk{position:absolute;inset:0}
.cam{position:absolute;inset:0;transform-origin:50% 50%}
.cam img{position:absolute;left:0;top:0;width:1920px;height:1080px;object-fit:cover}
.flash{position:absolute;inset:0;background:#fff;opacity:0;z-index:40;pointer-events:none}
.flash.o{background:#ff4a1c}
.ttl{position:absolute;z-index:20;display:flex;flex-wrap:wrap;gap:12px 26px;max-width:1720px}
.ttl.bl{left:96px;bottom:104px}
.ttl.tl{left:96px;top:96px}
.ttl.br{right:96px;bottom:104px;justify-content:flex-end}
.ttl.tr{right:96px;top:96px;justify-content:flex-end}
.w{display:inline-block;font-size:124px;line-height:.9;letter-spacing:-.02em;text-transform:uppercase;background:#06070c;padding:16px 26px 8px;
   text-shadow:-3px 0 rgba(255,74,28,.9),3px 0 rgba(65,224,240,.8)}
.w.a{color:#ff4a1c}
.field{position:absolute;inset:0;display:flex;align-items:center;justify-content:center}
.giant{font-size:250px;line-height:.86;letter-spacing:-.03em;text-transform:uppercase;text-align:center;
   text-shadow:-5px 0 rgba(255,74,28,.85),5px 0 rgba(65,224,240,.75)}
.mono{font-family:"JetBrains Mono",monospace}
"""

JS = """
const B = (k) => k * 0.4405;
const W = 1920, H = 1080;
// Camera framing that puts source point (fx, fy) at the centre at scale s, never showing an edge.
const cam = (fx, fy, s) => {
  const mx = (s - 1) * W / 2, my = (s - 1) * H / 2;
  const x = Math.max(-mx, Math.min(mx, (0.5 - fx) * W * s));
  const y = Math.max(-my, Math.min(my, (0.5 - fy) * H * s));
  return { scale: s, x, y };
};
// Where source point (fx, fy) lands on screen under a cam() framing.
const onScreen = (fx, fy, c) => ({ x: W / 2 + (fx * W - W / 2) * c.scale + c.x, y: H / 2 + (fy * H - H / 2) * c.scale + c.y });
const flash = (tl, sel, t, o = 0.9, d = 0.2) => { tl.set(sel, { opacity: o }, t); tl.to(sel, { opacity: 0, duration: d, ease: "power2.out" }, t + 0.034); };
const shake = (tl, sel, t, a = 24) => {
  [[1, -0.6], [-0.8, 0.9], [0.6, -0.4], [-0.35, 0.3], [0.15, -0.1], [0, 0]].forEach((p, i) => tl.set(sel, { x: p[0] * a, y: p[1] * a }, t + i * 0.034));
};
// Words of a title slam in one by one at the given times, then hold dead still.
const slam = (tl, sel, times) => {
  document.querySelectorAll(sel + " .w").forEach((el, i) => {
    tl.fromTo(el, { opacity: 0, scale: 1.45, filter: "blur(10px)" }, { opacity: 1, scale: 1, filter: "blur(0px)", duration: 0.18, ease: "expo.out" }, times[Math.min(i, times.length - 1)]);
  });
};
const leave = (tl, sel, t) => tl.to(sel + " .w", { opacity: 0, filter: "blur(8px)", duration: 0.14, ease: "power2.in", stagger: 0.03 }, t);
"""


def scene(cid, dur, body, script):
    html = f"""<template>
  <style>{CSS}</style>
  <div id="root" data-composition-id="{cid}" data-width="1920" data-height="1080">
{body}
  </div>
  <script>
{JS}
    const tl = gsap.timeline({{ paused: true }});
{script}
    window.__timelines["{cid}"] = tl;
  </script>
</template>
"""
    (OUT / f"{cid}.html").write_text(html)


def shot(id_, src, start, dur, z=1):
    return (f'    <div id="{id_}" class="shot clip" data-start="{start}" data-duration="{round(dur, 3)}" data-track-index="{z}" style="z-index:{z}">'
            f'<div class="shk" id="{id_}-shk"><div class="cam" id="{id_}-cam"><img id="{id_}-img" src="assets/shots/{src}.jpg" alt=""></div></div></div>')


def title(id_, pos, start, dur, words, accent=()):
    spans = "".join(f'<span class="w{" a" if w in accent else ""}">{w}</span>' for w in words)
    return f'    <div id="{id_}" class="ttl {pos} clip" data-start="{start}" data-duration="{round(dur, 3)}" data-track-index="10">{spans}</div>'


def flashdiv(id_, cls=""):
    return f'    <div id="{id_}" class="flash {cls}"></div>'


# ── 1. Hook: the eyes in the dark ────────────────────────────────────────
HOOK_DUR = 3.604  # the drop lands at film 3.604 s (beat 8)
o = 0.08  # the first beat sits 80 ms into the track
scene("hook", HOOK_DUR, "\n".join([
    shot("h-eyes", "splash-eyes", 0, o + b(4)),
    shot("h-head", "splash-head", round(o + b(4), 3), HOOK_DUR - (o + b(4))),
    '    <div id="h-bars" style="position:absolute;inset:0;z-index:30;pointer-events:none">'
    '<div style="position:absolute;left:0;right:0;top:0;height:132px;background:#000"></div>'
    '<div style="position:absolute;left:0;right:0;bottom:0;height:132px;background:#000"></div></div>',
    f'    <div id="h-t1" class="clip" data-start="0.15" data-duration="{round(o + b(4) - 0.2, 3)}" data-track-index="10" '
    'style="position:absolute;left:0;right:0;bottom:30px;z-index:31;text-align:center;font-family:Inter;font-weight:700;font-size:56px;letter-spacing:.32em;color:#cfc8e6">'
    '<span class="q">SOMETHING</span> <span class="q">IN</span> <span class="q">THE</span> <span class="q">DEEP</span></div>',
    f'    <div id="h-t2" class="clip" data-start="{round(o + b(4), 3)}" data-duration="{round(b(3.4), 3)}" data-track-index="10" '
    'style="position:absolute;left:0;right:0;bottom:30px;z-index:31;text-align:center;font-family:Inter;font-weight:700;font-size:56px;letter-spacing:.32em;color:#cfc8e6">'
    '<span class="q">IS</span> <span class="q" style="color:#ff4a1c">WATCHING</span> <span class="q">YOUR</span> <span class="q">CODE</span></div>',
    flashdiv("h-flash"),
]), f"""
    const o = 0.08;
    tl.fromTo("#h-eyes-cam", cam(0.5, 0.59, 2.2), {{ ...cam(0.5, 0.59, 2.75), duration: o + B(4), ease: "power1.inOut" }}, 0);
    tl.fromTo("#h-head-cam", cam(0.5, 0.42, 1.45), {{ ...cam(0.5, 0.42, 1.8), duration: B(3), ease: "power1.out" }}, o + B(4));
    // Zoom through the eye into the drop.
    tl.to("#h-head-cam", {{ ...cam(0.5, 0.36, 7.5), duration: B(1) + 0.004, ease: "power3.in" }}, o + B(7));
    // Flicker like a dying light.
    tl.set("#h-head-img", {{ opacity: 0.25 }}, o + B(6.5)); tl.set("#h-head-img", {{ opacity: 1 }}, o + B(6.5) + 0.067);
    tl.set("#h-eyes-img", {{ opacity: 0.3 }}, o + B(2.5)); tl.set("#h-eyes-img", {{ opacity: 1 }}, o + B(2.5) + 0.05);
    document.querySelectorAll("#h-t1 .q").forEach((el, i) => tl.fromTo(el, {{ opacity: 0, filter: "blur(6px)" }}, {{ opacity: 1, filter: "blur(0px)", duration: 0.2, ease: "power2.out" }}, 0.15 + i * B(0.5)));
    document.querySelectorAll("#h-t2 .q").forEach((el, i) => tl.fromTo(el, {{ opacity: 0, filter: "blur(6px)" }}, {{ opacity: 1, filter: "blur(0px)", duration: 0.2, ease: "power2.out" }}, o + B(4) + 0.02 + i * B(0.5)));
    tl.to("#h-t2", {{ opacity: 0, duration: 0.12 }}, o + B(7.2));
    tl.to("#h-bars", {{ opacity: 0, duration: B(1), ease: "power2.in" }}, o + B(7));
    tl.fromTo("#h-flash", {{ opacity: 0 }}, {{ opacity: 1, duration: 0.2, ease: "power2.in" }}, 3.404);
""")

# ── 2. Drop: the roar, then the claim on colour fields ───────────────────
words = [("THE GPU", "#06070c", "#f2eeff"), ("TERMINAL", "#6e55ff", "#f2eeff"), ("FOR THE", "#06070c", "#f2eeff"), ("AGENT ERA", "#ff4a1c", "#06070c")]
fields = []
for i, (w, bg, ink) in enumerate(words):
    fields.append(f'    <div id="d-f{i}" class="shot clip" data-start="{b(4 + i)}" data-duration="{b(1)}" data-track-index="2" style="z-index:2;background:{bg}">'
                  f'<div class="field" id="d-f{i}-in"><div class="giant" style="color:{ink};font-size:215px">{w}</div></div></div>')
scene("drop", b(8), "\n".join([shot("d-roar", "splash-roar", 0, b(4))] + fields + [flashdiv("d-flash"), flashdiv("d-oflash", "o")]), """
    tl.fromTo("#d-roar-cam", { scale: 1.5 }, { scale: 1.12, duration: 0.32, ease: "expo.out" }, 0);
    tl.to("#d-roar-cam", { scale: 1.17, duration: B(2) - 0.32, ease: "none" }, 0.32);
    shake(tl, "#d-roar-shk", 0, 34);
    flash(tl, "#d-flash", 0, 1, 0.28);
    tl.set("#d-roar-cam", { scale: 1.26 }, B(2));
    tl.to("#d-roar-cam", { scale: 1.18, duration: 0.3, ease: "expo.out" }, B(2));
    shake(tl, "#d-roar-shk", B(2), 18);
    flash(tl, "#d-oflash", B(2), 0.55, 0.16);
    for (let i = 0; i < 4; i++) {
      tl.fromTo(`#d-f${i}-in`, { scale: 1.35 }, { scale: 1, duration: 0.22, ease: "expo.out" }, B(4 + i));
      shake(tl, `#d-f${i}-in`, B(4 + i), i === 3 ? 30 : 14);
    }
    flash(tl, "#d-flash", B(7), 0.45, 0.1);
""")

# ── 3. Agents side by side ──────────────────────────────────────────────
scene("agents", b(8), "\n".join([
    shot("a-win", "hero", 0, b(2)),
    shot("a-cols", "hero", b(2), b(2)),
    shot("a-row", "hero", b(4), b(2)),
    shot("a-dash", "agents", b(6), b(2)),
    title("a-t1", "bl", b(0.4), b(3.6) - 0.02, ["RUN", "EVERY", "AGENT"], ["AGENT"]),
    title("a-t2", "tl", b(4), b(4), ["SIDE", "BY", "SIDE"]),
    flashdiv("a-flash"),
]), """
    tl.fromTo("#a-win-shk", { x: 1500 }, { x: 0, duration: 0.3, ease: "expo.out" }, 0);
    tl.fromTo("#a-win-cam", { scale: 1.12 }, { scale: 1.02, duration: B(2), ease: "power2.out" }, 0);
    tl.fromTo("#a-cols-cam", cam(0.76, 0.26, 2.05), { ...cam(0.76, 0.3, 1.9), duration: B(2), ease: "power1.out" }, B(2));
    shake(tl, "#a-cols-shk", B(2), 16);
    tl.fromTo("#a-row-cam", cam(0.5, 0.985, 3.6), { ...cam(0.5, 0.985, 3.2), duration: 0.3, ease: "expo.out" }, B(4));
    shake(tl, "#a-row-shk", B(4), 18);
    flash(tl, "#a-flash", B(4), 0.6, 0.12);
    tl.fromTo("#a-dash-cam", cam(0.5, 0.24, 2.4), { ...cam(0.5, 0.24, 2.05), duration: 0.32, ease: "expo.out" }, B(6));
    shake(tl, "#a-dash-shk", B(6), 14);
    slam(tl, "#a-t1", [B(0.4), B(1), B(2)]);
    leave(tl, "#a-t1", B(3.7));
    slam(tl, "#a-t2", [B(4), B(5), B(6)]);
""")

# ── 4. The "needs you" card, answered with one click ────────────────────
scene("card", b(8), "\n".join([
    shot("c-card", "card", 0, b(4)),
    shot("c-done", "card-answered", b(4), b(4)),
    title("c-t1", "bl", b(0.5), b(3.4) - 0.02, ["AGENT", "STUCK?"], ["STUCK?"]),
    title("c-t2", "tl", b(4.5), b(3.5), ["ANSWER", "IN", "ONE", "CLICK"], ["CLICK"]),
    '    <div id="c-cursor" class="clip" data-start="0" data-duration="' + str(b(4) + 0.2) + '" data-track-index="11" style="position:absolute;left:0;top:0;z-index:25;width:64px;height:88px">'
    '<svg id="c-cursor-svg" width="64" height="88" viewBox="0 0 16 22" style="overflow:visible"><path d="M1 1 L1 17 L5 13.4 L8 20.5 L10.6 19.4 L7.7 12.5 L13 12.5 Z" fill="#fff" stroke="#06070c" stroke-width="1.2" stroke-linejoin="round"/></svg></div>',
    flashdiv("c-flash", "o"),
]), """
    const cardCam = cam(0.87, 0.17, 2.45);
    tl.fromTo("#c-card-cam", { scale: 1, x: 0, y: 0 }, { ...cardCam, duration: 0.55, ease: "power2.inOut" }, 0.05);
    const yes = onScreen(0.779, 0.171, cardCam);
    tl.set("#c-cursor", { opacity: 0 }, 0);
    tl.fromTo("#c-cursor", { opacity: 1, x: 1980, y: 1120 }, { x: yes.x - 6, y: yes.y - 4, duration: B(1.6), ease: "power3.out" }, B(2));
    tl.to("#c-cursor-svg", { scale: 0.8, duration: 0.06, transformOrigin: "0% 0%" }, B(4) - 0.1);
    tl.to("#c-cursor-svg", { scale: 1, duration: 0.08 }, B(4) - 0.04);
    tl.fromTo("#c-done-cam", cam(0.75, 0.95, 2.8), { ...cam(0.75, 0.95, 2.4), duration: 0.32, ease: "expo.out" }, B(4));
    shake(tl, "#c-done-shk", B(4), 22);
    flash(tl, "#c-flash", B(4), 0.6, 0.16);
    slam(tl, "#c-t1", [B(0.5), B(1.5)]);
    leave(tl, "#c-t1", B(3.75));
    slam(tl, "#c-t2", [B(4.5), B(5), B(5.5), B(6)]);
""")

# ── 5. Paste screenshots to agents, images in the grid ──────────────────
scene("paste", b(8), "\n".join([
    shot("p-paste", "paste", 0, b(4)),
    shot("p-img", "icat", b(4), b(4)),
    title("p-t1", "bl", b(0.5), b(3.4) - 0.02, ["PASTE", "SCREENSHOTS"], ["SCREENSHOTS"]),
    title("p-t2", "br", b(4.5), b(3.5), ["INLINE", "IMAGES"], ["IMAGES"]).replace('class="ttl br clip"', 'class="ttl br clip" style="max-width:900px"'),
    flashdiv("p-flash"),
]), """
    tl.fromTo("#p-paste-shk", { x: -1500 }, { x: 0, duration: 0.3, ease: "expo.out" }, 0);
    tl.fromTo("#p-paste-cam", cam(0.17, 0.264, 2.5), { ...cam(0.17, 0.264, 2.6), duration: B(4), ease: "none" }, 0);
    tl.fromTo("#p-img-cam", cam(0.1, 0.21, 2.6), { ...cam(0.16, 0.26, 1.75), duration: B(3), ease: "power2.inOut" }, B(4));
    shake(tl, "#p-img-shk", B(4), 22);
    flash(tl, "#p-flash", B(4), 0.7, 0.16);
    slam(tl, "#p-t1", [B(0.5), B(1.5)]);
    leave(tl, "#p-t1", B(3.75));
    slam(tl, "#p-t2", [B(4.5), B(5.5)]);
""")

# ── 6. Tab overview: pull out from one tab to all of them ───────────────
scene("overview", b(8), "\n".join([
    shot("o-grid", "overview", 0, b(8)),
    title("o-t1", "bl", b(2), b(6), ["EVERY", "TAB.", "ONE", "GLANCE."], ["GLANCE."]),
]), """
    tl.fromTo("#o-grid-cam", cam(0.5, 0.66, 2.7), { ...cam(0.5, 0.52, 1.0), duration: B(2), ease: "power3.inOut" }, 0.02);
    tl.to("#o-grid-cam", { scale: 1.05, duration: B(6), ease: "none" }, B(2));
    shake(tl, "#o-grid-shk", B(2), 12);
    slam(tl, "#o-t1", [B(2), B(2.5), B(4), B(4.5)]);
""")

# ── 7. Themes, one per beat ─────────────────────────────────────────────
themes = [("hero", "Kraken Abyss"), ("theme-neon-reef", "Neon Reef"), ("theme-ember", "Ember"), ("theme-deep-sea", "Deep Sea"),
          ("theme-midnight-ink", "Midnight Ink"), ("theme-high-contrast", "High Contrast"), ("palette", "⌘⇧P  theme"), ("theme-neon-reef", "Neon Reef")]
parts = []
for i, (src, name) in enumerate(themes):
    parts.append(shot(f"th-{i}", src, b(i), b(1)))
    parts.append(f'    <div id="th-n{i}" class="clip mono" data-start="{b(i)}" data-duration="{b(1)}" data-track-index="11" '
                 'style="position:absolute;right:96px;bottom:96px;z-index:22;font-size:44px;font-weight:700;background:#06070c;color:#f2eeff;padding:14px 24px;border-left:8px solid #ff4a1c">'
                 f'{name}</div>')
parts.append(title("th-t", "tl", 0, b(8), ["6", "THEMES.", "LIVE."], ["LIVE."]))
scene("themes", b(8), "\n".join(parts), """
    for (let i = 0; i < 8; i++) {
      const c = i === 6 ? cam(0.5, 0.25, 2.1) : { scale: 1.12, x: 0, y: 40 };
      tl.fromTo(`#th-${i}-cam`, { ...c, scale: c.scale * 1.08 }, { ...c, duration: 0.24, ease: "expo.out" }, B(i));
      shake(tl, `#th-${i}-shk`, B(i), 10);
    }
    slam(tl, "#th-t", [0.02, B(1), B(2)]);
""")

# ── 8. Tools: git, file tree, fuzzy find, status bar ────────────────────
tools = [("t-git", "gitpop", (0.31, 0.9, 2.9), "tl", ["GIT,", "ONE", "CLICK"], ["GIT,"]),
         ("t-tree", "sidebar", (0.11, 0.32, 1.95), "br", ["FILE", "TREE"], ["TREE"]),
         ("t-find", "findfile", (0.5, 0.19, 2.3), "bl", ["FUZZY", "FIND"], ["FUZZY"]),
         ("t-bar", "card", (0.4, 0.985, 1.9), "tl", ["STATUS", "THAT", "TALKS"], ["TALKS"])]
parts, js = [], []
for i, (id_, src, c, pos, ws, acc) in enumerate(tools):
    parts.append(shot(id_, src, b(2 * i), b(2)))
    parts.append(title(f"{id_}-t", pos, b(2 * i), b(2), ws, acc))
    d = 1 if i % 2 == 0 else -1
    js.append(f'    tl.fromTo("#{id_}-shk", {{ x: {d * 900} }}, {{ x: 0, duration: 0.26, ease: "expo.out" }}, B({2 * i}));')
    js.append(f'    tl.fromTo("#{id_}-cam", cam({c[0]}, {c[1]}, {c[2] * 1.1:.3f}), {{ ...cam({c[0]}, {c[1]}, {c[2]}), duration: B(2), ease: "power2.out" }}, B({2 * i}));')
    js.append(f'    slam(tl, "#{id_}-t", [B({2 * i}) + 0.03, B({2 * i + 0.5}), B({2 * i + 1})]);')
scene("tools", b(8), "\n".join(parts), "\n".join(js))

# ── 9. Speed, in measured numbers ───────────────────────────────────────
stats = [("200", " MB/s", "of terminal output parsed every second", "#06070c", "#f2eeff"),
         ("0.4", "%", "CPU while idle", "#6e55ff", "#f2eeff"),
         ("75", " MB", "memory footprint", "#06070c", "#f2eeff"),
         ("RUST", " + METAL", "native code, drawn on the GPU", "#ff4a1c", "#06070c")]
parts = []
for i, (n, unit, label, bg, ink) in enumerate(stats):
    parts.append(f'    <div id="s-{i}" class="shot clip" data-start="{b(2 * i)}" data-duration="{b(2)}" data-track-index="2" style="z-index:2;background:{bg};color:{ink}">'
                 f'<div class="field" id="s-{i}-in" style="flex-direction:column;gap:28px">'
                 f'<div class="giant" style="font-size:240px"><span id="s-{i}-n">{n}</span><span style="font-size:150px">{unit}</span></div>'
                 f'<div style="font-family:Inter;font-weight:700;font-size:54px;letter-spacing:-.01em;opacity:.9">{label}</div></div></div>')
parts.append('    <div id="s-note" class="clip" data-start="0" data-duration="' + str(b(8)) + '" data-track-index="11" '
             'style="position:absolute;right:96px;bottom:72px;z-index:22;font-family:Inter;font-weight:700;font-size:28px;opacity:.75">measured on an Apple Silicon Mac</div>')
scene("speed", b(8), "\n".join(parts), """
    const count = (sel, from, to, dec, t) => {
      const el = document.querySelector(sel), o = { v: from };
      tl.fromTo(o, { v: from }, { v: to, duration: B(1.4), ease: "expo.out", onUpdate: () => { el.textContent = o.v.toFixed(dec); } }, t);
    };
    count("#s-0-n", 0, 200, 0, 0.02);
    count("#s-1-n", 9.9, 0.4, 1, B(2));
    count("#s-2-n", 0, 75, 0, B(4));
    for (let i = 0; i < 4; i++) {
      tl.fromTo(`#s-${i}-in`, { scale: 1.3 }, { scale: 1, duration: 0.24, ease: "expo.out" }, B(2 * i));
      shake(tl, `#s-${i}-in`, B(2 * i), i === 3 ? 26 : 14);
    }
""")

# ── 10. Call to action: open source, one command ────────────────────────
CMD1 = "curl -fsSL https://raw.githubusercontent.com/"
CMD2 = "lakshya-rohila/kraken/main/install.sh | bash"
out_lines = [
    ('<span style="color:#ff4a1c">◉ KRAKEN</span><span style="color:#8d86a8">  — the GPU terminal for the coding-agent era</span>', b(11)),
    ('<span style="color:#41e0f0">▌</span> Cloning <b>lakshya-rohila/kraken</b> (main)', b(12)),
    ('<span style="color:#41e0f0">▌</span> Building a release bundle', b(13)),
    ('<span style="color:#ff4a1c">◉ Kraken is installed</span> → /Applications/Kraken.app', b(14)),
]
parts = [
    f'    <div id="x-open" class="shot clip" data-start="0" data-duration="{b(2)}" data-track-index="2" style="z-index:2">'
    '<div class="field" id="x-open-in" style="flex-direction:column"><div class="giant">OPEN</div><div class="giant" style="color:#ff4a1c">SOURCE</div></div></div>',
    f'    <div id="x-gh" class="shot clip" data-start="{b(2)}" data-duration="{b(2)}" data-track-index="2" style="z-index:2;background:#6e55ff">'
    '<div class="field" id="x-gh-in" style="flex-direction:column;gap:30px"><div style="font-family:Inter;font-weight:700;font-size:56px">Free, on GitHub</div>'
    '<div class="mono" id="x-gh-url" data-var-text="repo" style="font-size:76px;font-weight:800;background:#06070c;padding:18px 34px">github.com/lakshya-rohila/kraken</div></div></div>',
    f'    <div id="x-term" class="shot clip" data-start="{b(4)}" data-duration="{b(12)}" data-track-index="2" style="z-index:2">'
    '<div class="shk" id="x-term-shk"><div class="cam" id="x-term-cam">'
    '<div style="position:absolute;left:96px;top:96px;font-size:124px;line-height:.9;text-transform:uppercase">ONE <span style="color:#ff4a1c">COMMAND.</span></div>'
    '<div id="x-win" style="position:absolute;left:96px;right:96px;top:300px;background:#0b0c14;border:3px solid #6e55ff;border-radius:18px;padding:56px 64px;font-family:\'JetBrains Mono\',monospace;font-size:46px;line-height:1.5;color:#e8e4f7">'
    '<div><span style="color:#ff6b8a">❯</span> <span id="x-c1"></span></div>'
    '<div style="padding-left:56px"><span id="x-c2"></span><span id="x-caret" style="display:inline-block;width:26px;height:52px;background:#41e0f0;vertical-align:-8px;margin-left:4px"></span></div>'
    '<div style="height:28px"></div>'
    + "".join(f'<div class="xo" id="x-o{i}" style="font-size:38px;opacity:0">{l}</div>' for i, (l, _) in enumerate(out_lines))
    + '</div></div></div></div>',
    flashdiv("x-flash"),
]
scene("cta", b(16), "\n".join(parts), f"""
    const CMD1 = {CMD1!r}, CMD2 = {CMD2!r};
    tl.fromTo("#x-open-in", {{ scale: 1.4 }}, {{ scale: 1, duration: 0.24, ease: "expo.out" }}, 0.02);
    shake(tl, "#x-open-in", 0.02, 24);
    tl.set("#x-open-in .giant:nth-child(2)", {{ opacity: 0 }}, 0); tl.set("#x-open-in .giant:nth-child(2)", {{ opacity: 1 }}, B(1));
    shake(tl, "#x-open-in", B(1), 18);
    tl.fromTo("#x-gh-in", {{ scale: 1.25 }}, {{ scale: 1, duration: 0.24, ease: "expo.out" }}, B(2));
    shake(tl, "#x-gh-in", B(2), 16);
    // Type the command at a steady 34 characters a second.
    const c1 = document.querySelector("#x-c1"), c2 = document.querySelector("#x-c2");
    const ty = {{ n: 0 }}, total = CMD1.length + CMD2.length;
    tl.fromTo(ty, {{ n: 0 }}, {{ n: total, duration: B(6), ease: "none", onUpdate: () => {{
      const n = Math.round(ty.n); c1.textContent = CMD1.slice(0, n); c2.textContent = n > CMD1.length ? CMD2.slice(0, n - CMD1.length) : "";
    }} }}, B(4.3));
    for (let k = 10.5; k < 16; k += 0.5) tl.set("#x-caret", {{ opacity: (k * 2) % 2 === 0 ? 1 : 0 }}, B(k));
    tl.fromTo("#x-term-cam", {{ scale: 1.0 }}, {{ scale: 1.04, duration: B(6.5), ease: "none" }}, B(4));
    // Enter.
    tl.set("#x-term-cam", {{ scale: 1.09 }}, B(10.5));
    tl.to("#x-term-cam", {{ scale: 1.05, duration: 0.3, ease: "expo.out" }}, B(10.5));
    shake(tl, "#x-term-shk", B(10.5), 20);
    flash(tl, "#x-flash", B(10.5), 0.75, 0.16);
    {"".join(f'tl.fromTo("#x-o{i}", {{ opacity: 0, x: -24 }}, {{ opacity: 1, x: 0, duration: 0.16, ease: "expo.out" }}, {t});' for i, (_, t) in enumerate(out_lines))}
    shake(tl, "#x-term-shk", B(14), 12);
""")

# ── 11. End card ────────────────────────────────────────────────────────
END = round(45 - 38.844, 3)
scene("endcard", END, "\n".join([
    '    <div id="e-lock" style="position:absolute;left:0;right:0;top:250px;display:flex;align-items:center;justify-content:center;gap:56px;z-index:3">'
    '<div id="e-icon-wrap"><img id="e-icon" src="assets/icon.png" alt="" style="display:block;width:330px;height:330px;border-radius:74px"></div>'
    '<div id="e-word" style="font-size:210px;letter-spacing:-.03em;line-height:1;text-shadow:-5px 0 rgba(255,74,28,.85),5px 0 rgba(65,224,240,.75)">KRAKEN</div></div>',
    '    <div id="e-tag" style="position:absolute;left:0;right:0;top:640px;text-align:center;font-family:Inter;font-weight:700;font-size:52px;color:#cfc8e6;z-index:3">'
    + " ".join(f'<span class="tw">{w}</span>' for w in "The GPU terminal for the coding-agent era".split()) + '</div>',
    '    <div id="e-cmd" class="mono" data-var-text="command" style="position:absolute;left:50%;top:790px;transform:translateX(-50%);white-space:nowrap;font-size:30px;font-weight:700;background:#0b0c14;border:2px solid #6e55ff;border-radius:12px;padding:20px 30px;z-index:3;color:#e8e4f7">'
    'curl -fsSL https://raw.githubusercontent.com/lakshya-rohila/kraken/main/install.sh | bash</div>',
    '    <div id="e-url" class="mono" style="position:absolute;left:0;right:0;top:905px;text-align:center;font-size:34px;font-weight:700;color:#ff4a1c;z-index:3">github.com/lakshya-rohila/kraken</div>',
    '    <div id="e-black" style="position:absolute;inset:0;background:#000;opacity:0;z-index:30"></div>',
    flashdiv("e-flash", "o"),
]), """
    tl.fromTo("#e-icon", { scale: 2.4, opacity: 0 }, { scale: 1, opacity: 1, duration: 0.36, ease: "expo.out" }, 0.0);
    shake(tl, "#e-lock", 0.0, 30);
    flash(tl, "#e-flash", 0.0, 0.7, 0.22);
    const shift = (document.querySelector("#e-word").offsetWidth + 56) / 2;
    tl.fromTo("#e-icon-wrap", { x: shift }, { x: 0, duration: 0.42, ease: "power3.out" }, B(2) - 0.1);
    tl.fromTo("#e-word", { clipPath: "inset(0 100% 0 0)" }, { clipPath: "inset(0 0% 0 0)", duration: 0.42, ease: "power3.out" }, B(2) - 0.1);
    document.querySelectorAll("#e-tag .tw").forEach((el, i) => tl.fromTo(el, { opacity: 0, color: "#3a3550" }, { opacity: 1, color: "#cfc8e6", duration: 0.12 }, B(3) + i * 0.08));
    tl.fromTo("#e-cmd", { opacity: 0, filter: "blur(8px)" }, { opacity: 1, filter: "blur(0px)", duration: 0.2, ease: "power2.out" }, B(4));
    tl.fromTo("#e-url", { opacity: 0 }, { opacity: 1, duration: 0.2 }, B(5));
    tl.to("#e-black", { opacity: 1, duration: 0.9, ease: "power2.in" }, 6.156 - 0.95);
""")

print("wrote", sorted(p.name for p in OUT.glob("*.html")))
