//! Drawing: the frame (tab bar, panes, status bar, overlays), toasts,
//! drag previews and animation pacing.

use super::*;

impl TermWindow {
    pub(super) fn redraw(&mut self, ctx: &mut Ctx) {
        let build_start = Instant::now();
        self.capture_hook();
        self.later_hook(ctx);
        self.select_autoscroll();
        let theme = self.preview.clone().unwrap_or_else(|| ctx.cfg.theme.clone());
        let t = &theme;
        let s = self.scale;
        let w = self.renderer.config.width as f32;
        let mut frame = std::mem::take(&mut self.frame);
        frame.clear_all();
        let speed = ctx.cfg.animation_speed;
        let dt = self.last_frame.elapsed().as_secs_f32().min(0.1);
        self.last_frame = Instant::now();
        frame.clear = rgb(t.background);
        frame.clear_alpha = ctx.cfg.opacity;
        let ui_px = 12.5 * s;
        let label = |x: f32, y: f32, h: f32, spans: Vec<Span>, clip: R| TextLine {
            x,
            y: y + (h - ui_px * 1.4) / 2.0,
            font: FontKind::Ui,
            size: ui_px,
            line_height: ui_px * 1.4,
            spans,
            clip: Some([clip.x as i32, clip.y as i32, clip.right() as i32, clip.bottom() as i32]),
            cell_width: None,
        };
        let span = |text: String, c: Rgb| Span { text, color: rgb(c), bold: false, italic: false };

        // Title bar.
        let bar_h = self.titlebar_h();
        let chrome_a = (255.0 * (0.55 + 0.45 * ctx.cfg.opacity.clamp(0.0, 1.0))) as u8;
        let sf = t.surface;
        frame.under.push(Rect { x: 0.0, y: 0.0, w, h: bar_h, color: [sf.0, sf.1, sf.2, chrome_a] });
        let title = match self.tabs.get(self.active).and_then(|t| t.name.clone()) {
            Some(name) => name,
            None => self.focus_id().map(|id| self.pane_title(id)).unwrap_or_default(),
        };
        let title = if title.is_empty() { "Kraken".to_string() } else { title };
        let approx_w = title.chars().count() as f32 * ui_px * 0.55;
        let tcol = if self.focused { t.foreground.mix(t.muted, 0.35) } else { t.muted };
        frame.lines.push(label(
            ((w - approx_w) / 2.0).max(80.0 * s),
            0.0,
            bar_h,
            vec![span(title, tcol)],
            R { x: 0.0, y: 0.0, w, h: bar_h },
        ));

        // Tab strip.
        self.tab_hits.clear();
        self.plus_hit = None;
        let tab_h = self.tabbar_h();
        if tab_h > 0.0 {
            let y0 = bar_h;
            frame.under.push(Rect { x: 0.0, y: y0, w, h: tab_h, color: [sf.0, sf.1, sf.2, chrome_a] });
            let pad = 10.0 * s;
            let plus_w = 30.0 * s;
            let n = self.tabs.len() as f32;
            let tw = ((w - pad * 2.0 - plus_w) / n).min(MAX_TAB_PT * s).floor();
            let mut target = None;
            for i in 0..self.tabs.len() {
                let active = i == self.active;
                let r = R { x: pad + i as f32 * tw, y: y0 + 4.0 * s, w: tw - 4.0 * s, h: tab_h - 8.0 * s };
                // New tabs pop open (with a little overshoot) and fade in.
                let lin = anim::progress(self.tabs[i].born, 260.0, speed);
                let pop = ease_out_back(lin);
                let appear = ease_out_cubic(lin);
                let tab = &self.tabs[i];
                let tab_color = self.tab_color(i, t);
                if active {
                    let pw = r.w * pop;
                    let ph = r.h * (0.7 + 0.3 * pop);
                    let fill = tab_color.map_or(t.elevated, |c| t.elevated.mix(c, 0.22));
                    frame.under.push(Rect::new(r.x + (r.w - pw) / 2.0, r.y + (r.h - ph) / 2.0, pw, ph, rgb(fill)));
                    target = Some((r.x, r.w));
                } else if let Some(c) = tab_color {
                    frame.under.push(Rect::new(r.x, r.y, r.w, r.h, rgb(t.surface.mix(c, 0.12))));
                    frame.under.push(Rect::new(r.x, r.bottom() - 2.0 * s, r.w, 2.0 * s, rgb(c.mix(t.surface, 0.35))));
                }
                let ids = tab.root.panes();
                let activity = ids.iter().any(|id| self.panes.get(id).is_some_and(|p| p.activity));
                let bell = ids.iter().any(|id| self.panes.get(id).is_some_and(|p| p.bell));
                let num_color = tab_color.unwrap_or(if active { t.accent } else { t.muted });
                let mut spans = vec![span(format!("{}  ", i + 1), num_color)];
                // Status dot first so a long title can't push it out of view.
                if bell {
                    spans.push(span("● ".into(), t.ssh));
                } else if activity {
                    spans.push(span("● ".into(), t.normal[2]));
                }
                if tab.zoom {
                    spans.push(span("⤢ ".into(), t.accent));
                }
                if ids.len() > 1 {
                    spans.push(span(format!("[{}] ", ids.len()), t.muted));
                }
                match &tab.name {
                    Some(name) => {
                        let mut sp = span(name.clone(), if active { t.foreground } else { t.foreground.mix(t.muted, 0.35) });
                        sp.bold = true;
                        spans.push(sp);
                    }
                    None => spans.push(span(self.pane_title(tab.focus), if active { t.foreground } else { t.muted })),
                }
                let close_w = 18.0 * s;
                let text_clip = R { w: r.w - if active { close_w + 6.0 * s } else { 6.0 * s }, ..r };
                for sp in &mut spans {
                    let c = t.surface.mix(Rgb(sp.color[0], sp.color[1], sp.color[2]), appear);
                    sp.color = rgb(c);
                }
                frame.lines.push(label(r.x + 10.0 * s, r.y + (1.0 - appear) * 6.0 * s, r.h, spans, text_clip));
                let close = active.then(|| {
                    let c = R { x: r.right() - close_w - 4.0 * s, y: r.y, w: close_w, h: r.h };
                    frame.lines.push(label(c.x + 4.0 * s, c.y, c.h, vec![span("×".into(), t.muted)], c));
                    c
                });
                self.tab_hits.push(TabHit { index: i, rect: r, close });
            }
            // The accent underline glides to the active tab.
            if let Some((tx, twid)) = target {
                let k = if speed > 0.0 { 1.0 - (-dt * 16.0 * speed).exp() } else { 1.0 };
                let (x, wd) = match self.indicator {
                    Some((x, wd)) => (x + (tx - x) * k, wd + (twid - wd) * k),
                    None => (tx, twid),
                };
                self.indicator_moving = (x - tx).abs() > 0.5 || (wd - twid).abs() > 0.5;
                let (x, wd) = if self.indicator_moving { (x, wd) } else { (tx, twid) };
                self.indicator = Some((x, wd));
                let line_color = self.tab_color(self.active, t).unwrap_or(t.accent);
                frame.under.push(Rect::new(x, y0 + tab_h - 6.0 * s, wd, 2.0 * s, rgb(line_color)));
            }
            let p = R { x: pad + n * tw + 2.0 * s, y: y0 + 4.0 * s, w: plus_w - 6.0 * s, h: tab_h - 8.0 * s };
            frame.lines.push(label(p.x + 7.0 * s, p.y, p.h, vec![span("+".into(), t.muted)], p));
            self.plus_hit = Some(p);
        }
        if tab_h == 0.0 {
            self.indicator = None;
            self.indicator_moving = false;
        }
        let chrome_bottom = bar_h + tab_h;
        frame.under.push(Rect::new(0.0, chrome_bottom - s, w, s, rgb(t.border)));

        // Panes.
        let focus = self.focus_id();
        let mut new_cursor: Option<Rect> = None;
        let multi = self.visible.len() > 1;
        if focus != self.focus_drawn {
            self.focus_prev = self.focus_drawn;
            self.focus_drawn = focus;
            self.focus_at = Instant::now();
        }
        let focus_t = ease_out_cubic(anim::progress(self.focus_at, 160.0, speed));
        let dim_max = ctx.cfg.dim_inactive.clamp(0.0, 0.85);
        let blink = self.blink_state(ctx.cfg);
        for &(id, r) in &self.visible {
            let Some(p) = self.panes.get(&id) else { continue };
            let first_line = frame.lines.len();
            let mut marks = Vec::new();
            {
                let mut term = p.session.term.lock();
                let mut layout = p.layout;
                if Some(id) == focus {
                    layout.cursor_visible = blink.is_none_or(|(on, _)| on);
                }
                let c = view::build(&term, t, &layout, &mut frame, &mut marks);
                if Some(id) == focus {
                    new_cursor = c;
                }
                if let Some(st) = self.search.as_mut().filter(|st| st.pane == id) {
                    if st.dirty {
                        st.recompute(&mut term);
                    }
                    st.highlights(&term, &p.layout, t, &mut frame.under);
                }
            }
            // Inline images: each placeholder row tells us where its image's top is.
            if !marks.is_empty()
                && let Ok(store) = p.session.images.lock()
            {
                let mut seen = std::collections::HashSet::new();
                for m in marks {
                    let Some(pl) = store.placement(m.slot) else { continue };
                    let top = m.y - m.row as f32 * p.layout.cell_h;
                    if !seen.insert((m.slot, top as i32, m.x as i32)) {
                        continue;
                    }
                    frame.images.push(kraken_render::ImageDraw {
                        key: pl.image.key,
                        width: pl.image.width,
                        height: pl.image.height,
                        rgba: pl.image.rgba.clone(),
                        x: m.x,
                        y: top,
                        w: pl.draw_w,
                        h: pl.draw_h,
                        clip: [r.x as i32, r.y as i32, r.right() as i32, r.bottom() as i32],
                    });
                }
            }
            // New panes grow out from the divider; a new tab's pane fades in.
            let lin = anim::progress(p.born, 300.0, speed);
            if lin < 1.0 {
                let e = ease_out_cubic(lin);
                let b = t.background;
                let a = t.accent;
                match p.grow {
                    Some(Dir::Horizontal) => {
                        let x = r.x + r.w * e;
                        frame.over.push(Rect { x, y: r.y, w: r.right() - x, h: r.h, color: [b.0, b.1, b.2, 255] });
                        frame.over.push(Rect {
                            x: x - 2.0 * s,
                            y: r.y,
                            w: 2.0 * s,
                            h: r.h,
                            color: [a.0, a.1, a.2, (255.0 * (1.0 - e)) as u8],
                        });
                    }
                    Some(Dir::Vertical) => {
                        let y = r.y + r.h * e;
                        frame.over.push(Rect { x: r.x, y, w: r.w, h: r.bottom() - y, color: [b.0, b.1, b.2, 255] });
                        frame.over.push(Rect {
                            x: r.x,
                            y: y - 2.0 * s,
                            w: r.w,
                            h: 2.0 * s,
                            color: [a.0, a.1, a.2, (255.0 * (1.0 - e)) as u8],
                        });
                    }
                    None => {
                        frame.over.push(Rect { x: r.x, y: r.y, w: r.w, h: r.h, color: [b.0, b.1, b.2, (255.0 * (1.0 - e)) as u8] });
                    }
                }
            }
            let clip = [r.x as i32, r.y as i32, r.right() as i32, r.bottom() as i32];
            for line in &mut frame.lines[first_line..] {
                line.clip = Some(clip);
            }
            // Panes you're not typing in sit back; focus changes crossfade.
            if multi && dim_max > 0.0 {
                let dim = if Some(id) == focus {
                    if Some(id) == self.focus_prev { 0.0 } else { dim_max * (1.0 - focus_t) }
                } else if Some(id) == self.focus_prev {
                    dim_max * focus_t
                } else {
                    dim_max
                };
                if dim > 0.003 {
                    let b = t.background;
                    frame.over.push(Rect { x: r.x, y: r.y, w: r.w, h: r.h, color: [b.0, b.1, b.2, (dim * 255.0) as u8] });
                }
            }
            // The focused pane gets a soft glow along its inside edge.
            if multi && ctx.cfg.focus_glow && Some(id) == focus {
                let c = if p.st.ssh.is_some() { t.ssh } else { t.accent };
                for (inset, width, alpha) in [(0.0f32, 1.5f32, 210.0f32), (1.5, 2.5, 70.0), (4.0, 4.0, 26.0)] {
                    let (i, bw) = ((inset * s).round(), (width * s).max(1.0).round());
                    let al = (alpha * focus_t) as u8;
                    let col = [c.0, c.1, c.2, al];
                    frame.over.push(Rect { x: r.x + i, y: r.y + i, w: r.w - 2.0 * i, h: bw, color: col });
                    frame.over.push(Rect { x: r.x + i, y: r.bottom() - i - bw, w: r.w - 2.0 * i, h: bw, color: col });
                    frame.over.push(Rect { x: r.x + i, y: r.y + i + bw, w: bw, h: r.h - 2.0 * (i + bw), color: col });
                    frame.over.push(Rect { x: r.right() - i - bw, y: r.y + i + bw, w: bw, h: r.h - 2.0 * (i + bw), color: col });
                }
            }
        }
        // Panes connected to another machine get an orange outline.
        for &(id, r) in &self.visible {
            if self.panes.get(&id).is_some_and(|p| p.st.ssh.is_some()) {
                let b = (2.0 * s).round();
                let c = rgb(t.ssh);
                frame.over.push(Rect::new(r.x, r.y, r.w, b, c));
                frame.over.push(Rect::new(r.x, r.bottom() - b, r.w, b, c));
                frame.over.push(Rect::new(r.x, r.y, b, r.h, c));
                frame.over.push(Rect::new(r.right() - b, r.y, b, r.h, c));
            }
        }

        let status_px = 11.5 * s;
        let geom = BarGeom {
            x: 0.0,
            y: self.renderer.config.height as f32 - self.status_h(),
            w,
            h: self.status_h(),
            scale: s,
            char_w: self.cell.0 * status_px / (self.font_pt * s),
            font_px: status_px,
        };
        let mut view = self.status_view();
        static ANIM_T0: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
        view.anim = ANIM_T0.get_or_init(Instant::now).elapsed().as_secs_f32();
        let first_under = frame.under.len();
        self.status_hits = statusbar::draw(&mut frame, t, &geom, &view, self.status_hover);
        if let Some(bar) = frame.under.get_mut(first_under) {
            bar.color[3] = chrome_a;
        }
        if self.overview.is_some() {
            let speed = ctx.cfg.animation_speed;
            let (opened, closing, selected) =
                self.overview.as_ref().map(|o| (o.opened, o.closing, o.selected)).unwrap_or((Instant::now(), None, 0));
            let open = match closing {
                Some(at) => 1.0 - ease_out_cubic(anim::progress(at, 140.0, speed)),
                None => ease_out_cubic(anim::progress(opened, 190.0, speed)),
            };
            if closing.is_some() && open <= 0.0 {
                self.overview = None;
                self.overview_hits.clear();
            } else {
                let cards = self.overview_cards(t);
                let og = kraken_ui::overview::OverviewGeom {
                    area: R { x: 0.0, y: chrome_bottom, w, h: geom.y - chrome_bottom },
                    scale: s,
                    ui_px: 13.0 * s,
                    aspect: w / (geom.y - chrome_bottom).max(1.0),
                };
                self.overview_hits = kraken_ui::overview::draw(&mut frame, t, &og, &cards, selected, open);
            }
        }
        if let Some(pop) = &self.git_pop {
            let info = self.git.get(&pop.root).and_then(|e| e.info.as_ref());
            let pg = kraken_ui::gitpop::PopGeom {
                anchor_x: pop.anchor_x,
                bottom: geom.y,
                window_w: w,
                char_w: geom.char_w,
                font_px: geom.font_px,
                scale: s,
            };
            self.git_pop_hits = kraken_ui::gitpop::draw(&mut frame, t, &pg, info, pop.changes.as_deref(), self.git_pop_hover);
        } else {
            self.git_pop_hits.clear();
        }
        self.draw_pane_drag(&mut frame, t);
        if let (Some(sb), Some(g)) = (&self.sidebar, self.sidebar_geom()) {
            sidebar::draw(&mut frame, t, &g, sb, chrome_a);
        }
        // ⌘ held over a link: underline it.
        if let Some((pid, row, c0, c1, _)) = &self.link_hover
            && let Some(p) = self.panes.get(pid)
        {
            let l = &p.layout;
            let y = l.y + (*row as f32 + 1.0) * l.cell_h - 3.0 * s;
            let c = t.accent.mix(Rgb(255, 255, 255), 0.2);
            frame.over.push(Rect::new(l.x + *c0 as f32 * l.cell_w, y, (*c1 + 1 - *c0) as f32 * l.cell_w, 2.0 * s, rgb(c)));
        }
        // Files dragged in from Finder: show where they'll be pasted.
        if self.file_hover
            && let Some(&(_, r)) = self
                .pane_at(self.cursor_px.0, self.cursor_px.1)
                .or(self.focus_id())
                .and_then(|id| self.visible.iter().find(|(p, _)| *p == id))
        {
            let a = t.accent;
            frame.overlay_rects.push(Rect { x: r.x, y: r.y, w: r.w, h: r.h, color: [a.0, a.1, a.2, 34] });
            let b = 2.0 * s;
            for e in [R { h: b, ..r }, R { y: r.bottom() - b, h: b, ..r }, R { w: b, ..r }, R { x: r.right() - b, w: b, ..r }] {
                frame.overlay_rects.push(Rect { x: e.x, y: e.y, w: e.w, h: e.h, color: [a.0, a.1, a.2, 230] });
            }
        }

        // Search box on the searched pane.
        if let Some(st) = &self.search
            && let Some(&(_, r)) = self.visible.iter().find(|(id, _)| *id == st.pane)
        {
            let px = 12.5 * s;
            let g = search::BarGeom { pane: r, scale: s, char_w: self.cell.0 * px / (self.font_pt * s), font_px: px };
            search::draw(&mut frame, t, &g, st, 1.0);
        }
        self.draw_toast(&mut frame, t, geom.y);
        self.draw_preedit(&mut frame, t, new_cursor);
        self.draw_rename(&mut frame, t);
        // Tell macOS where the cursor is, so the candidate window opens beside it.
        let moved = new_cursor.map(|c| (c.x as i32, c.y as i32)) != self.cursor_rect.map(|c| (c.x as i32, c.y as i32));
        if moved && let Some(c) = new_cursor {
            self.window.set_ime_cursor_area(
                winit::dpi::PhysicalPosition::new(c.x as f64, c.y as f64),
                winit::dpi::PhysicalSize::new(c.w as f64, c.h as f64),
            );
        }
        self.cursor_rect = new_cursor;

        if let Some(start) = self.splash {
            let area = R { x: 0.0, y: chrome_bottom, w, h: geom.y - chrome_bottom };
            let sg = SplashGeom { area, cell_w: self.cell.0, cell_h: self.cell.1, font_px: self.font_pt * s };
            let secs = if speed > 0.0 { anim::scaled_secs(start, speed) } else { splash::TOTAL };
            if !splash::draw(&mut frame, t, &sg, secs) {
                self.splash = None;
            }
        }
        let speed_now = speed;
        self.cards.retain(|c| c.leaving.is_none_or(|at| anim::progress(at, cards::SLIDE_OUT_MS, speed_now) < 1.0));
        if self.cards.is_empty() {
            self.card_hits.clear();
        } else {
            let px = 12.5 * s;
            let g = cards::CardGeom {
                area: R { x: 0.0, y: chrome_bottom, w, h: geom.y - chrome_bottom },
                scale: s,
                char_w: self.cell.0 * px / (self.font_pt * s),
                font_px: px,
            };
            self.card_hits = cards::draw(&mut frame, t, &g, &self.cards, speed);
        }
        if let Some((opened, sel)) = self.agents_panel {
            let px = 13.0 * s;
            let g = agents::PanelGeom {
                window: R { x: 0.0, y: 0.0, w, h: self.renderer.config.height as f32 },
                scale: s,
                char_w: self.cell.0 * px / (self.font_pt * s),
                font_px: px,
            };
            let rows = self.agent_rows();
            agents::draw(
                &mut frame,
                t,
                &g,
                &rows,
                sel.min(rows.len().saturating_sub(1)),
                ease_out_cubic(anim::progress(opened, 170.0, speed)),
            );
        }
        if let Some((start, closing)) = self.sheet {
            let lin = anim::progress(start, if closing { 150.0 } else { 200.0 }, speed);
            let p = if closing { 1.0 - ease_out_cubic(lin) } else { ease_out_cubic(lin) };
            if closing && lin >= 1.0 {
                self.sheet = None;
            } else {
                let px = 13.0 * s;
                let sg = SheetGeom {
                    window: R { x: 0.0, y: 0.0, w, h: self.renderer.config.height as f32 },
                    scale: s,
                    char_w: self.cell.0 * px / (self.font_pt * s),
                    font_px: px,
                };
                sheet::draw(&mut frame, t, &sg, p);
            }
        }
        if self.palette.is_some() {
            let (start, closing) = self.palette_anim;
            let lin = anim::progress(start, if closing { 120.0 } else { 170.0 }, speed);
            let p = if closing { 1.0 - ease_out_cubic(lin) } else { ease_out_cubic(lin) };
            if closing && lin >= 1.0 {
                self.palette = None;
            } else if let Some(st) = &self.palette {
                let px = 13.0 * s;
                let pg = PaletteGeom {
                    window: R { x: 0.0, y: 0.0, w, h: self.renderer.config.height as f32 },
                    scale: s,
                    char_w: self.cell.0 * px / (self.font_pt * s),
                    font_px: px,
                };
                palette::draw(&mut frame, t, &pg, st, p);
            }
        }

        let divider_color = t.border.mix(t.muted, 0.25);
        for d in &self.dividers {
            let r = d.rect;
            frame.over.push(Rect::new(r.x, r.y, r.w, r.h, rgb(divider_color)));
        }

        let render_start = Instant::now();
        if !self.renderer.render_frame(ctx.shared, &frame) {
            self.window.request_redraw();
        }
        self.presented = true;
        self.refresh_a11y(false);
        if let Some(perf) = self.perf.as_mut() {
            perf.frame(render_start - build_start, self.renderer.last_prepare, render_start.elapsed(), self.renderer.last_shaped);
        }
        self.frame = frame;
    }

    /// `KRAKEN_CAPTURE=/path.bmp` saves a frame ~2.5 s after launch.
    /// `KRAKEN_CAPTURE_MS=300,900,1600` saves one per time, as path-0.bmp, path-1.bmp, …
    pub(super) fn capture_hook(&mut self) {
        let Ok(path) = std::env::var("KRAKEN_CAPTURE") else { return };
        static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let start = *START.get_or_init(Instant::now);
        let times: Vec<u64> = std::env::var("KRAKEN_CAPTURE_MS")
            .ok()
            .map(|v| v.split(',').filter_map(|x| x.trim().parse().ok()).collect())
            .unwrap_or_else(|| vec![2500]);
        let n = NEXT.load(std::sync::atomic::Ordering::Relaxed);
        let Some(&due) = times.get(n) else { return };
        if start.elapsed() >= Duration::from_millis(due) {
            NEXT.store(n + 1, std::sync::atomic::Ordering::Relaxed);
            let out = if times.len() > 1 { path.replace(".bmp", &format!("-{n}.bmp")) } else { path };
            self.renderer.capture = Some(out.into());
        }
        self.window.request_redraw();
    }

    // ── Animation pacing ─────────────────────────────────────────────────

    /// `Some(now)` while an animation is running; `None` when idle, so an idle
    /// window costs no CPU.
    pub fn next_frame(&self, cfg: &Config) -> Option<Instant> {
        if self.occluded {
            return None; // hidden: no animation frames at all
        }
        let speed = cfg.animation_speed;
        let busy = self.splash.is_some()
            || self.sheet.is_some_and(|(st, closing)| closing || anim::progress(st, 200.0, speed) < 1.0)
            || self.indicator_moving
            || (self.visible.len() > 1 && anim::progress(self.focus_at, 160.0, speed) < 1.0)
            || self.overview.as_ref().is_some_and(|o| o.closing.is_some() || anim::progress(o.opened, 190.0, speed) < 1.0)
            || self.cards.iter().any(|c| c.leaving.is_some() || anim::progress(c.born, cards::SLIDE_IN_MS, speed) < 1.0)
            || self.agents_panel.is_some_and(|(at, _)| anim::progress(at, 170.0, speed) < 1.0)
            || (self.palette.is_some() && (self.palette_anim.1 || anim::progress(self.palette_anim.0, 170.0, speed) < 1.0))
            || self.toast.as_ref().is_some_and(|(_, at, _)| {
                let e = at.elapsed();
                e < TOAST_FADE || (e > TOAST_SHOW && e < TOAST_SHOW + TOAST_FADE)
            })
            || self.tabs.iter().any(|t| anim::progress(t.born, 260.0, speed) < 1.0)
            || self.visible.iter().any(|(id, _)| self.panes.get(id).is_some_and(|p| anim::progress(p.born, 300.0, speed) < 1.0));
        if busy {
            return Some(Instant::now());
        }
        // Dragging a selection past a pane's edge keeps scrolling the scrollback.
        if self.select_overshoot().is_some() {
            return Some(self.autoscroll_at + AUTOSCROLL_STEP);
        }
        // The legend bar's spinner, comet and pulses: 30 frames a second is
        // plenty for them, and only while something is actually happening.
        if self.focused && self.status_view().is_animating() {
            return Some(Instant::now() + Duration::from_millis(33));
        }
        // While the overview is open its thumbnails stay live (8 times a second).
        if self.overview.is_some() {
            return Some(Instant::now() + Duration::from_millis(125));
        }
        // Wake up when a toast starts fading out, or the cursor blinks.
        let toast = self.toast.as_ref().and_then(|(_, at, _)| (at.elapsed() < TOAST_SHOW).then(|| *at + TOAST_SHOW));
        let blink = self.blink_state(cfg).map(|(_, next)| next);
        match (toast, blink) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// The focused cursor's blink: `(visible now, next toggle)`, or `None`
    /// when it shouldn't blink (unfocused window, a steady cursor, or idle
    /// long enough that blinking stops to save power).
    pub(super) fn blink_state(&self, cfg: &Config) -> Option<(bool, Instant)> {
        const STOP_AFTER: Duration = Duration::from_secs(30);
        if !self.focused || self.palette.is_some() || self.renaming.is_some() {
            return None;
        }
        let p = self.focused_pane()?;
        if !p.session.cursor_blinks(cfg) {
            return None;
        }
        let elapsed = self.blink_epoch.elapsed();
        if elapsed >= STOP_AFTER {
            return None;
        }
        let half = Duration::from_millis(cfg.cursor.blink_interval_ms.clamp(100, 5000));
        let n = (elapsed.as_millis() / half.as_millis()) as u32;
        Some((n.is_multiple_of(2), (self.blink_epoch + half * (n + 1)).min(self.blink_epoch + STOP_AFTER)))
    }

    // ── Status ───────────────────────────────────────────────────────────

    pub(super) fn status_view(&self) -> StatusView {
        let p = self.focused_pane();
        let st = p.map(|p| &p.st);
        let home = std::env::var("HOME").unwrap_or_default();
        let cwd = st.and_then(|s| s.cwd.as_ref()).map(|c| {
            let c = c.to_string_lossy();
            if !home.is_empty() && c.starts_with(&home) { format!("~{}", &c[home.len()..]) } else { c.into_owned() }
        });
        let git = st.and_then(|s| s.git_root.as_ref()).and_then(|r| self.git.get(r)).and_then(|e| e.info.clone());
        StatusView {
            program: st.and_then(|s| s.program.clone()),
            ssh: st.and_then(|s| s.ssh.clone()),
            cwd,
            git: if st.is_some_and(|s| s.ssh.is_some()) { None } else { git },
            running_for: st.and_then(|s| s.running_since).map(|t| t.elapsed()),
            last: st.and_then(|s| s.last),
            clock: statusbar::clock(),
            agents: {
                let all: Vec<&Agent> = self.panes.values().filter_map(|p| p.agent.as_ref()).collect();
                let working = all.iter().filter(|a| a.state == AgentState::Working).count();
                (!all.is_empty()).then_some((working, all.len() - working))
            },
            agent_dots: {
                // In tab order, so the dots line up with your layout.
                let mut dots = Vec::new();
                for tab in &self.tabs {
                    for id in tab.root.panes() {
                        if let Some(a) = self.panes.get(&id).and_then(|p| p.agent.as_ref()) {
                            // "Claude Code" → "claude": short, and never ambiguous like initials.
                            let label = a.name.split_whitespace().next().unwrap_or(a.name).to_lowercase();
                            dots.push((label, a.state == AgentState::Working, a.attention));
                        }
                    }
                }
                dots
            },
            usage: st.and_then(|s| s.usage),
            finished_ago: st.and_then(|s| s.last_at).map(|t| t.elapsed()).filter(|d| d.as_secs() < 10),
            animate: st.is_some_and(|s| s.running_since.is_some()) && p.is_some_and(|p| p.agent.is_none()),
            anim: 0.0,
        }
    }

    /// A tab's display label: its name, or the focused pane's title.
    pub(super) fn tab_label(&self, i: usize) -> String {
        match self.tabs.get(i) {
            Some(t) => t.name.clone().unwrap_or_else(|| self.pane_title(t.focus)),
            None => String::new(),
        }
    }

    /// The colour shown for tab `i`: yours, else orange while connected over SSH.
    pub(super) fn tab_color(&self, i: usize, t: &Theme) -> Option<Rgb> {
        let tab = self.tabs.get(i)?;
        match tab.color {
            Some(99) => Some(t.ssh),
            Some(c) => Some(t.normal[c.min(7)]),
            None => tab.root.panes().iter().any(|id| self.panes.get(id).is_some_and(|p| p.st.ssh.is_some())).then_some(t.ssh),
        }
    }

    /// The tab-name editor, as a pill in the middle of the title bar.
    pub(super) fn draw_rename(&self, frame: &mut Frame, t: &Theme) {
        let Some((_, text)) = &self.renaming else { return };
        let s = self.scale;
        let px = 12.5 * s;
        let cw = self.cell.0 * px / (self.font_pt * s);
        let w_win = self.renderer.config.width as f32;
        let label = "✎ Tab name  ";
        let body = if text.is_empty() { "automatic (folder)".to_string() } else { text.clone() };
        let cells = (label.chars().count() + body.chars().count().max(18) + 18) as f32;
        let w = (cells * cw).min(w_win - 160.0 * s);
        let h = self.titlebar_h() - 6.0 * s;
        let x = (w_win - w) / 2.0;
        let y = 3.0 * s;
        let (e, a) = (t.elevated, t.accent);
        frame.overlay_rects.push(Rect { x: x - s, y: y - s, w: w + 2.0 * s, h: h + 2.0 * s, color: [a.0, a.1, a.2, 255] });
        frame.overlay_rects.push(Rect { x, y, w, h, color: [e.0, e.1, e.2, 255] });
        let mut spans = vec![Span { text: label.into(), color: rgb(t.accent), bold: true, italic: false }];
        if text.is_empty() {
            spans.push(Span { text: body, color: rgb(t.muted), bold: false, italic: false });
        } else {
            spans.push(Span { text: body, color: rgb(t.foreground), bold: false, italic: false });
        }
        spans.push(Span { text: "▏".into(), color: rgb(t.cursor), bold: false, italic: false });
        spans.push(Span { text: "   ↩ save · esc cancel".into(), color: rgb(t.muted), bold: false, italic: false });
        frame.overlay_lines.push(TextLine {
            x: x + 10.0 * s,
            y: y + (h - px * 1.3) / 2.0,
            font: FontKind::Mono,
            size: px,
            line_height: px * 1.3,
            spans,
            clip: Some([x as i32, y as i32, (x + w) as i32, (y + h) as i32]),
            cell_width: Some(cw),
        });
    }

    /// Draw the text you're composing with an input method, underlined, at the cursor.
    pub(super) fn draw_preedit(&self, frame: &mut Frame, t: &Theme, cursor: Option<Rect>) {
        if self.preedit_text.is_empty() {
            return;
        }
        let Some(c) = cursor else { return };
        let s = self.scale;
        let cells: usize = self.preedit_text.chars().map(|ch| if is_wide(ch) { 2 } else { 1 }).sum();
        let w = cells as f32 * self.cell.0;
        let e = t.elevated;
        frame.overlay_rects.push(Rect { x: c.x, y: c.y, w, h: c.h, color: [e.0, e.1, e.2, 255] });
        let a = t.accent;
        frame.overlay_rects.push(Rect { x: c.x, y: c.y + c.h - 2.0 * s, w, h: 2.0 * s, color: [a.0, a.1, a.2, 255] });
        frame.overlay_lines.push(TextLine {
            x: c.x,
            y: c.y,
            font: FontKind::Mono,
            size: self.font_pt * s,
            line_height: c.h,
            spans: vec![Span { text: self.preedit_text.clone(), color: rgb(t.foreground), bold: false, italic: false }],
            clip: None,
            cell_width: Some(self.cell.0),
        });
    }

    pub fn show_toast(&mut self, text: &str, error: bool) {
        self.toast = Some((text.to_string(), Instant::now(), error));
        self.window.request_redraw();
    }

    pub(super) fn draw_toast(&mut self, frame: &mut Frame, t: &Theme, bottom: f32) {
        let Some((text, at, error)) = &self.toast else { return };
        let e = at.elapsed();
        if e >= TOAST_SHOW + TOAST_FADE {
            self.toast = None;
            return;
        }
        let p = if e < TOAST_FADE {
            ease_out_cubic(e.as_secs_f32() / TOAST_FADE.as_secs_f32())
        } else if e > TOAST_SHOW {
            1.0 - ease_out_cubic((e - TOAST_SHOW).as_secs_f32() / TOAST_FADE.as_secs_f32())
        } else {
            1.0
        };
        let s = self.scale;
        let px = 12.5 * s;
        let cw = self.cell.0 * px / (self.font_pt * s);
        let w_win = self.renderer.config.width as f32;
        let tw = (text.chars().count() as f32 + 4.0) * cw;
        let h = px * 2.3;
        let x = (w_win - tw) / 2.0;
        let y = bottom - h - 14.0 * s + (1.0 - p) * 10.0 * s;
        let a = (255.0 * p) as u8;
        let el = t.elevated;
        let mark = if *error { t.normal[1] } else { t.accent };
        frame.overlay_rects.push(Rect { x, y, w: tw, h, color: [el.0, el.1, el.2, a] });
        frame.overlay_rects.push(Rect { x, y, w: 3.0 * s, h, color: [mark.0, mark.1, mark.2, a] });
        let c = el.mix(if *error { t.normal[1].mix(t.foreground, 0.4) } else { t.foreground }, p);
        frame.overlay_lines.push(TextLine {
            x: x + 2.0 * cw,
            y: y + (h - px * 1.3) / 2.0,
            font: FontKind::Mono,
            size: px,
            line_height: px * 1.3,
            spans: vec![Span { text: text.clone(), color: rgb(c), bold: false, italic: false }],
            clip: None,
            cell_width: Some(cw),
        });
    }

    // ── Moving panes ─────────────────────────────────────────────────────

    /// Grip on the hovered pane, plus drop-zone previews while dragging.
    pub(super) fn draw_pane_drag(&self, frame: &mut Frame, t: &Theme) {
        let s = self.scale;
        let dragging = match &self.drag {
            Some(Drag::Pane { id, active: true, .. }) => Some(*id),
            _ => None,
        };
        if dragging.is_none()
            && let Some(g) = self.hover.and_then(|h| self.grip_rect(h))
        {
            let c = t.muted.mix(t.background, 0.35);
            // Two short bars: a classic "grab here" handle.
            for i in 0..2 {
                let y = g.y + g.h * (0.3 + 0.3 * i as f32);
                frame.over.push(Rect::new(g.x + g.w * 0.2, y, g.w * 0.6, 2.0 * s, [c.0, c.1, c.2]));
            }
        }
        let Some(src) = dragging else { return };
        let a = t.accent;
        let tint = |r: R, alpha: u8, frame: &mut Frame| {
            frame.overlay_rects.push(Rect { x: r.x, y: r.y, w: r.w, h: r.h, color: [a.0, a.1, a.2, alpha] });
            let b = 2.0 * s;
            for e in [R { h: b, ..r }, R { y: r.bottom() - b, h: b, ..r }, R { w: b, ..r }, R { x: r.right() - b, w: b, ..r }] {
                frame.overlay_rects.push(Rect { x: e.x, y: e.y, w: e.w, h: e.h, color: [a.0, a.1, a.2, 230] });
            }
        };
        // Lift the dragged pane: dim it.
        if let Some(&(_, r)) = self.visible.iter().find(|(id, _)| *id == src) {
            let b = t.background;
            frame.overlay_rects.push(Rect { x: r.x, y: r.y, w: r.w, h: r.h, color: [b.0, b.1, b.2, 150] });
        }
        let label = match self.drop_target(src) {
            Some(DropTarget::Edge(id, dir)) => {
                if let Some(&(_, r)) = self.visible.iter().find(|(p, _)| *p == id) {
                    let half = match dir {
                        Direction::Left => R { w: r.w / 2.0, ..r },
                        Direction::Right => R { x: r.x + r.w / 2.0, w: r.w / 2.0, ..r },
                        Direction::Up => R { h: r.h / 2.0, ..r },
                        Direction::Down => R { y: r.y + r.h / 2.0, h: r.h / 2.0, ..r },
                    };
                    tint(half, 46, frame);
                }
                "split here"
            }
            Some(DropTarget::Swap(id)) => {
                if let Some(&(_, r)) = self.visible.iter().find(|(p, _)| *p == id) {
                    tint(r, 30, frame);
                }
                "⇄ swap"
            }
            Some(DropTarget::Tab(i)) => {
                if let Some(h) = self.tab_hits.iter().find(|h| h.index == i) {
                    tint(h.rect, 60, frame);
                }
                "move to tab"
            }
            Some(DropTarget::NewTab) => {
                if let Some(p) = self.plus_hit {
                    tint(p, 80, frame);
                }
                "new tab"
            }
            None => "",
        };
        // A little tag that follows the mouse.
        let px = 12.0 * s;
        let cw = self.cell.0 * px / (self.font_pt * s);
        let text = if label.is_empty() { self.pane_title(src) } else { format!("{}  ·  {label}", self.pane_title(src)) };
        let w = (text.chars().count() as f32 + 3.0) * cw;
        let h = px * 2.1;
        let (x, y) = (self.cursor_px.0 + 14.0 * s, self.cursor_px.1 + 14.0 * s);
        let e = t.elevated;
        frame.overlay_rects.push(Rect { x, y, w, h, color: [e.0, e.1, e.2, 240] });
        frame.overlay_rects.push(Rect { x, y, w: 3.0 * s, h, color: [a.0, a.1, a.2, 255] });
        frame.overlay_lines.push(TextLine {
            x: x + 1.5 * cw,
            y: y + (h - px * 1.3) / 2.0,
            font: FontKind::Mono,
            size: px,
            line_height: px * 1.3,
            spans: vec![Span { text, color: rgb(t.foreground), bold: false, italic: false }],
            clip: None,
            cell_width: Some(cw),
        });
    }

    // ── Window style & sessions ──────────────────────────────────────────
}
