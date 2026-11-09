# Kraken — launch reel spec

GOAL:    A developer watches 45 s, feels "this terminal is a monster", and runs the one-line install.
OUTPUT:  HyperFrames, 1920 x 1080, 30 fps
TIME:    45 s x 30 = 1350 frames. Music: X-TALI (slowed), ~136.2 BPM → beat = 0.4405 s = 13.2 frames.
         Beat k (film time) = 0.08 + 0.4405·k. The drop is beat 8 = 3.604 s. Every scene starts on a bar (4 beats).
         Track looped on 8-bar phrases (beats 16→32) to 45 s; 1.6 s fade-out. Private-share only.

LOOK:    Ground: Kraken abyss #06070C (the app's own background), never empty — real UI or type fills it.
         Ink #F2EEFF. Accent A blood-orange #FF4A1C (the kraken's eye) — one word per title, the payoff hits.
         Accent B Kraken violet #6E55FF (the focus border) — colour-field flips only. Cursor cyan #41E0F0 in the command only.
         Display: Archivo Black, uppercase, 120–230 px, tracking -2%. Mono: JetBrains Mono 44–56 px for the command.
         Medium: real Kraken captures (3200x1800 @2x) pushed 1.6–3.2x on the control that matters.
         Phonk-edit grammar: hard cuts on beats, 0 ms hits, 4–6 frame shake, 1–2 frame white/orange flash frames,
         RGB-split text shadow ±3 px, film grain 6% opacity stepping every frame.

OBJECTS: The splash kraken (real frames), the split window with three agents, the "needs you" card and its
         answered toast, pasted file paths, an inline image, the tab overview grid, six real themes, git popover,
         file tree, fuzzy finder, status bar, the install command and its real output, the app icon.

MOTION:  Arrivals expo.out 0.25–0.35 s; camera power2.inOut 0.4–0.6 s; zoom-through power3.in 0.44 s.
         Hits are tl.set at the beat; a move meant to land on a beat starts 0.1 s early.
         Titles land per word on beats and then hold dead still; exits are 0.15 s blur-out.

SHOTS:   (film time; b = beat index)
  1  0.00–3.60  HOOK      Splash eyes in the dark, slow push. "SOMETHING IN THE DEEP" / "IS WATCHING YOUR CODE".
                           b7→b8 zoom through the eye → white flash on the drop.
  2  3.60–7.13  DROP      Roar frame slams with shake; re-hit b10; then word-per-beat on colour fields:
                           THE GPU / TERMINAL / FOR THE / AGENT ERA.
  3  7.13–10.65 AGENTS    Window whips in; punch codex/gemini; punch status-bar agent row; agents panel.
                           "RUN EVERY AGENT" / "SIDE BY SIDE".
  4  10.65–14.18 CARD     Push to the real "Claude Code needs you" card; cursor clicks "1 Yes" on b28;
                           cut to the real "Answered Claude Code with 1 Yes" toast. "AGENT STUCK?" / "ONE CLICK".
  5  14.18–17.70 PASTE    Pasted screenshot path into the agent; inline image of the logo. "PASTE SCREENSHOTS" / "IMAGES IN THE GRID".
  6  17.70–21.22 OVERVIEW Pull out from the current tab to the live tab-overview grid. "EVERY TAB. ONE GLANCE."
  7  21.22–24.75 THEMES   One theme per beat (abyss, neon reef, ember, deep sea, midnight ink, high contrast, palette). "6 THEMES. LIVE."
  8  24.75–28.27 TOOLS    Git popover / file tree / fuzzy find / status bar, two beats each.
  9  28.27–31.80 SPEED    Measured numbers: 200 MB/s parse, 0.4% CPU idle, ~75 MB memory, Rust + Metal.
 10  31.80–38.84 CTA      "OPEN SOURCE" → github.com/lakshya-rohila/kraken → the curl one-liner types out → real installer output.
 11  38.84–45.00 END      Icon slams in, KRAKEN wordmark wipes, the command holds; fade with the music.

BANNED:  glow/bloom and floating particles added by us (the splash's own are the product); purple mesh gradients; glass cards;
         three cards in a row; the logo docked in a corner; titles that drift after landing; fade-and-slide-up entrances;
         invented UI — every screen is a real capture or the installer's real text.

QA:      0.0 (thumbnail: eyes, not black) · 3.62 (drop flash + roar) · 6.6 (AGENT ERA) · 9.0 (agent row legible) ·
         12.5 (cursor on 1 Yes) · 13.0 (toast) · 19.5 (overview) · 22.5 (theme cut) · 30 (number) · 37 (command fully typed) · 41 (end card).
