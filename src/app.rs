/// app.rs — Main application struct. Owns all UI state, the engine handle,
/// and drives every frame: poll engine events → update state → draw.

use std::sync::mpsc;
use std::time::Instant;

use egui::{
    Color32, Context, FontFamily, FontId, Painter, Pos2, Rect, Rounding, Stroke, Vec2, CentralPanel,
};

use crate::engine::{messages::{EngineCommand, EngineEvent}, EngineHandle};
use crate::ui::{
    charts::{BarChart, BarSeries, FragHistogram, LineChart, PieChart, PieSlice},
    cluster_map::{ClusterMap, MapPhase, MAP_COLS, MAP_ROWS},
    stats_panel::{AnalysisPanel, DefragPanel, MetricCard, VolumeStatsPanel},
    theme::*,
};

// ── Application state ─────────────────────────────────────────────────────────

#[derive(PartialEq, Clone, Copy)]
pub enum Phase { Idle, Enumerating, Analyzing, AnalysisDone, Defragging, DefragDone, Error }

#[derive(PartialEq, Clone, Copy)]
pub enum ActiveTab { Overview, Charts, TopFragmented }

pub struct DefragApp {
    engine:          EngineHandle,

    // Controls
    selected_drive:  String,
    compact_mode:    bool,
    boot_fallback:   bool,

    // State machine
    phase:           Phase,
    status_msg:      String,
    error_msg:       String,

    // Progress
    enum_progress:   f32,
    scan_progress:   f32,
    defrag_progress: f32,
    elapsed_secs:    u64,
    start_time:      Option<Instant>,

    // Volume
    vol_panel:       VolumeStatsPanel,
    total_clusters:  i64,

    // Cluster map
    cluster_map:     ClusterMap,
    cluster_map_after: ClusterMap,

    // Analysis
    analysis_panel:  AnalysisPanel,
    top_fragmented:  Vec<(String, u32)>,   // (filename, fragment_count)

    // Defrag
    defrag_panel:    DefragPanel,

    // Metric cards
    card_files:      MetricCard,
    card_frag:       MetricCard,
    card_clusters:   MetricCard,
    card_gb:         MetricCard,

    // Charts
    pie_chart:       PieChart,
    bar_chart:       BarChart,
    hist_chart:      FragHistogram,
    speed_chart:     LineChart,

    // UI
    active_tab:      ActiveTab,
    app_time:        f64,
    last_frame:      Instant,
}

impl DefragApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let engine = EngineHandle::launch();

        Self {
            engine,
            selected_drive:   "C:".to_string(),
            compact_mode:     false,
            boot_fallback:    true,
            phase:            Phase::Idle,
            status_msg:       "Select a drive and click Analyze or Defragment.".into(),
            error_msg:        String::new(),
            enum_progress:    0.0,
            scan_progress:    0.0,
            defrag_progress:  0.0,
            elapsed_secs:     0,
            start_time:       None,
            vol_panel:        VolumeStatsPanel::default(),
            total_clusters:   1_000_000,
            cluster_map:      ClusterMap::new(),
            cluster_map_after:ClusterMap::new(),
            analysis_panel:   AnalysisPanel::default(),
            top_fragmented:   Vec::new(),
            defrag_panel:     DefragPanel::default(),
            card_files:       MetricCard::new("Total Files",     TEXT_SEC),
            card_frag:        MetricCard::new("Fragmented",       AMBER),
            card_clusters:    MetricCard::new("Clusters Moved",   GREEN),
            card_gb:          MetricCard::new("Data Moved",       ACCENT),
            pie_chart:        PieChart::new(vec![]),
            bar_chart:        BarChart::new("File Size Distribution", vec![]),
            hist_chart:       FragHistogram::new(),
            speed_chart:      LineChart::new("Move speed", Color32::from_rgb(40, 200, 120), "cl/s"),
            active_tab:       ActiveTab::Overview,
            app_time:         0.0,
            last_frame:       Instant::now(),
        }
    }

    // ── Poll engine events ────────────────────────────────────────────────────

    fn poll_events(&mut self) {
        loop {
            match self.engine.evt_rx.try_recv() {
                Ok(ev)       => self.handle_event(ev),
                Err(mpsc::TryRecvError::Empty)        => break,
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        }
    }

    fn handle_event(&mut self, ev: EngineEvent) {
        match ev {
            EngineEvent::VolumeReady(info) => {
                self.vol_panel.label        = info.label.clone();
                self.vol_panel.filesystem   = info.filesystem.clone();
                self.vol_panel.cluster_size = info.cluster_size;
                self.vol_panel.total_bytes.set(info.total_bytes as f64);
                self.vol_panel.free_bytes.set(info.free_bytes as f64);
                self.vol_panel.anim_t       = 0.0;
                self.total_clusters         = info.total_clusters;
                self.defrag_panel.cluster_size = info.cluster_size;
            }

            EngineEvent::BitmapReady(map) => {
                self.cluster_map.apply_bitmap(&map);
                // Build "after" map — compact version
                let mut after_map = map.clone();
                let used_cells: usize = map.iter().flat_map(|r| r.iter()).filter(|&&c| c==2).count();
                let total_cells = MAP_COLS * MAP_ROWS;
                for r in 0..MAP_ROWS {
                    for c in 0..MAP_COLS {
                        let idx = r * MAP_COLS + c;
                        after_map[r][c] = if idx < used_cells { 2 } else { 0 };
                    }
                }
                self.cluster_map_after.apply_bitmap(&after_map);
            }

            EngineEvent::EnumProgress { done, total } => {
                self.enum_progress = if total > 0 { done as f32 / total as f32 } else { 0.0 };
                self.status_msg = format!("Enumerating files… {}/{}", fmt_large(done as u64), fmt_large(total as u64));
            }

            EngineEvent::EnumComplete { total_files } => {
                self.card_files.value.set(total_files as f64);
                self.card_files.anim_t = 0.0;
                self.status_msg = format!("Enumeration complete — {} files found.", fmt_large(total_files as u64));
            }

            EngineEvent::AnalysisProgress { done, total, frag_so_far, cluster_events } => {
                self.phase = Phase::Analyzing;
                self.scan_progress = if total > 0 { done as f32 / total as f32 } else { 0.0 };
                self.cluster_map.phase = MapPhase::Scanning;
                self.cluster_map.set_scan_progress(self.scan_progress);
                if !cluster_events.is_empty() {
                    self.cluster_map.apply_events(&cluster_events, self.total_clusters);
                }
                self.status_msg = format!(
                    "Analyzing… {:.0}%  (fragments found: {})",
                    self.scan_progress * 100.0,
                    fmt_large(frag_so_far)
                );
            }

            EngineEvent::AnalysisComplete(rep) => {
                self.phase         = Phase::AnalysisDone;
                self.scan_progress = 1.0;
                self.cluster_map.replace_state(4, 3);
                self.cluster_map.phase    = MapPhase::Complete;
                self.cluster_map.scan_col = None;

                self.analysis_panel.total_files.set(rep.total_files as f64);
                self.analysis_panel.fragmented_files.set(rep.fragmented_files as f64);
                self.analysis_panel.total_frags.set(rep.total_fragments as f64);
                self.analysis_panel.avg_frags   = rep.average_fragments();
                self.analysis_panel.frag_pct    = rep.fragmentation_percent();
                self.analysis_panel.anim_t      = 0.0;
                if let Some(w) = &rep.worst_file {
                    self.analysis_panel.worst_file = w.path.file_name()
                        .and_then(|n| n.to_str()).unwrap_or("").to_string();
                    self.analysis_panel.worst_count.set(w.fragment_count as f64);
                }

                self.card_files.value.set(rep.total_files as f64);
                self.card_frag.value.set(rep.fragmented_files as f64);
                self.card_files.anim_t = 0.0;
                self.card_frag.anim_t  = 0.0;

                // Pie chart
                let free  = self.vol_panel.free_bytes.val();
                let total = self.vol_panel.total_bytes.val();
                let used_clean = (total - free) * (1.0 - rep.fragmentation_percent() / 100.0);
                let used_frag  = (total - free) * (rep.fragmentation_percent() / 100.0);
                self.pie_chart = PieChart::new(vec![
                    PieSlice { label: "Free".into(),      value: free,       color: Color32::from_rgb(15, 80, 180) },
                    PieSlice { label: "Used".into(),      value: used_clean, color: Color32::from_rgb(30, 120, 60) },
                    PieSlice { label: "Fragmented".into(),value: used_frag,  color: Color32::from_rgb(200, 50, 50) },
                ]);

                // Bar chart from size buckets
                self.bar_chart = BarChart::new("File Size Distribution", vec![
                    BarSeries { label: "<1 MB".into(),   value: rep.size_buckets[0] as f64, color: Color32::from_rgb(30, 100, 200) },
                    BarSeries { label: "1-10 MB".into(), value: rep.size_buckets[1] as f64, color: Color32::from_rgb(50, 160, 80)  },
                    BarSeries { label: "10-100 MB".into(),value:rep.size_buckets[2] as f64, color: Color32::from_rgb(210, 150, 0)  },
                    BarSeries { label: ">100 MB".into(), value: rep.size_buckets[3] as f64, color: Color32::from_rgb(200, 50, 50)  },
                ]);

                // Histogram
                self.hist_chart.data  = rep.frag_histogram.clone();
                self.hist_chart.anim_t = 0.0;

                // Top fragmented list
                self.top_fragmented = rep.fragmented.iter().take(20).map(|f| {
                    let name = f.path.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string();
                    (name, f.fragment_count)
                }).collect();

                self.status_msg = format!(
                    "Analysis complete — {} fragmented files ({:.1}%)",
                    fmt_large(rep.fragmented_files), rep.fragmentation_percent()
                );
                self.stop_timer();
            }

            EngineEvent::DefragProgress {
                file_index, total_files, current_file,
                clusters_moved, files_defragged, files_skipped, files_in_use,
                bytes_moved, cluster_events,
            } => {
                self.phase = Phase::Defragging;
                self.defrag_progress = if total_files > 0 { file_index as f32 / total_files as f32 } else { 0.0 };
                self.cluster_map.phase = MapPhase::Defragging;
                self.cluster_map.apply_events(&cluster_events, self.total_clusters);

                self.defrag_panel.defragged.set(files_defragged as f64);
                self.defrag_panel.skipped.set(files_skipped as f64);
                self.defrag_panel.in_use.set(files_in_use as f64);
                self.defrag_panel.clusters.set(clusters_moved as f64);
                self.defrag_panel.bytes_moved.set(bytes_moved as f64);
                self.defrag_panel.anim_t = 0.0;

                self.card_clusters.value.set(clusters_moved as f64);
                self.card_gb.value.set(bytes_moved as f64);

                self.status_msg = format!(
                    "Defragmenting… {}/{} — {}",
                    file_index, total_files, current_file
                );
            }

            EngineEvent::DefragComplete(stats) => {
                self.phase         = Phase::DefragDone;
                self.defrag_progress = 1.0;
                self.cluster_map.replace_state(4, 5);
                self.cluster_map.phase = MapPhase::Complete;

                self.defrag_panel.attempted.set(stats.files_attempted as f64);
                self.defrag_panel.defragged.set(stats.files_defragged as f64);
                self.defrag_panel.skipped.set(stats.files_skipped as f64);
                self.defrag_panel.in_use.set(stats.files_in_use as f64);
                self.defrag_panel.whitelisted.set(stats.files_whitelisted as f64);
                self.defrag_panel.clusters.set(stats.clusters_moved as f64);
                self.defrag_panel.bytes_moved.set(stats.bytes_moved as f64);
                self.defrag_panel.boot_queued.set(stats.boot_queued as f64);

                self.card_clusters.value.set(stats.clusters_moved as f64);
                self.card_gb.value.set(stats.bytes_moved as f64);
                self.card_clusters.anim_t = 0.0;
                self.card_gb.anim_t       = 0.0;

                // Speed chart from history
                for v in &stats.speed_history { self.speed_chart.push(*v); }

                self.status_msg = format!(
                    "✓ Defragmentation complete — {} clusters moved  ({:.2} GB)",
                    fmt_large(stats.clusters_moved), stats.gb_moved()
                );
                self.stop_timer();
            }

            EngineEvent::Error(msg) => {
                self.phase     = Phase::Error;
                self.error_msg = msg.clone();
                self.status_msg = format!("Error: {}", msg);
                self.stop_timer();
            }

            EngineEvent::Stopped => {
                self.phase = Phase::Idle;
                self.status_msg = "Stopped.".into();
                self.stop_timer();
            }

            _ => {}
        }
    }

    // ── Timer helpers ────────────────────────────────────────────────────────

    fn start_timer(&mut self) { self.start_time = Some(Instant::now()); }
    fn stop_timer(&mut self)  { self.start_time = None; }
    fn tick_timer(&mut self) {
        if let Some(t) = self.start_time {
            self.elapsed_secs = t.elapsed().as_secs();
        }
    }
}

// ── eframe::App ───────────────────────────────────────────────────────────────

impl eframe::App for DefragApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // Delta time
        let now = Instant::now();
        let dt  = now.duration_since(self.last_frame).as_secs_f32().min(0.05);
        self.last_frame = now;
        self.app_time  += dt as f64;
        self.tick_timer();

        // Poll engine
        self.poll_events();

        // Animate everything
        let dt = dt;
        self.vol_panel.tick(dt);
        self.analysis_panel.tick(dt);
        self.defrag_panel.tick(dt);
        self.cluster_map.tick(dt);
        self.cluster_map_after.tick(dt);
        self.card_files.tick(dt);
        self.card_frag.tick(dt);
        self.card_clusters.tick(dt);
        self.card_gb.tick(dt);
        self.pie_chart.tick(dt);
        self.bar_chart.tick(dt);
        self.hist_chart.tick(dt);
        self.speed_chart.tick(dt);

        // Style
        let mut visuals = egui::Visuals::dark();
        visuals.window_fill     = BG_APP;
        visuals.panel_fill      = BG_APP;
        visuals.widgets.noninteractive.bg_fill = BG_PANEL;
        ctx.set_visuals(visuals);

        // Always repaint (animations)
        ctx.request_repaint();

        CentralPanel::default()
            .frame(egui::Frame::none().fill(BG_APP))
            .show(ctx, |ui| {
                // Use the full available rect
                let full = ui.available_rect_before_wrap();
                let painter = ui.painter().clone();
                self.draw_full(&painter, full, ctx);
                // Consume the space
                ui.allocate_rect(full, egui::Sense::hover());
            });
    }
}

impl DefragApp {
    fn draw_full(&mut self, painter: &Painter, rect: Rect, ctx: &Context) {
        let time = self.app_time;

        // ── Title bar ──────────────────────────────────────────────────────
        let bar_h = 52.0;
        let bar_rect = Rect::from_min_size(rect.min, Vec2::new(rect.width(), bar_h));
        self.draw_titlebar(painter, bar_rect, ctx);

        // ── Metric cards row ───────────────────────────────────────────────
        let cards_y   = rect.min.y + bar_h + 6.0;
        let card_h    = 70.0;
        let card_w    = (rect.width() - 30.0) / 4.0;
        for (i, card) in [&mut self.card_files, &mut self.card_frag,
                          &mut self.card_clusters, &mut self.card_gb].iter_mut().enumerate()
        {
            let cr = Rect::from_min_size(
                Pos2::new(rect.min.x + 6.0 + i as f32 * (card_w + 6.0), cards_y),
                Vec2::new(card_w, card_h),
            );
            card.draw(painter, cr);
        }

        // ── Tab bar ────────────────────────────────────────────────────────
        let tabs_y = cards_y + card_h + 4.0;
        let tab_h  = 26.0;
        self.draw_tabs(painter, Rect::from_min_size(
            Pos2::new(rect.min.x + 6.0, tabs_y),
            Vec2::new(rect.width() - 12.0, tab_h),
        ), ctx);

        // ── Main content ───────────────────────────────────────────────────
        let content_y  = tabs_y + tab_h + 4.0;
        let content_h  = rect.max.y - content_y - 48.0; // leave room for status bar
        let content_rect = Rect::from_min_size(
            Pos2::new(rect.min.x + 6.0, content_y),
            Vec2::new(rect.width() - 12.0, content_h),
        );

        match self.active_tab {
            ActiveTab::Overview       => self.draw_overview(painter, content_rect, time),
            ActiveTab::Charts         => self.draw_charts(painter, content_rect, time),
            ActiveTab::TopFragmented  => self.draw_top_frag(painter, content_rect),
        }

        // ── Status bar ─────────────────────────────────────────────────────
        let status_y = rect.max.y - 46.0;
        self.draw_status_bar(painter, Rect::from_min_size(
            Pos2::new(rect.min.x + 6.0, status_y),
            Vec2::new(rect.width() - 12.0, 40.0),
        ), time);
    }

    // ── Title bar ─────────────────────────────────────────────────────────────

    fn draw_titlebar(&mut self, painter: &Painter, rect: Rect, ctx: &Context) {
        // Glass bar background
        painter.rect_filled(rect, Rounding::ZERO, BG_PANEL);
        painter.rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(rect.width(), 1.0)),
            Rounding::ZERO, BORDER_BRT,
        );
        painter.rect_filled(
            Rect::from_min_size(Pos2::new(rect.min.x, rect.max.y - 1.0), Vec2::new(rect.width(), 1.0)),
            Rounding::ZERO, BORDER,
        );

        // Logo / title
        painter.text(
            Pos2::new(rect.min.x + 14.0, rect.center().y - 7.0), egui::Align2::LEFT_CENTER,
            "RustDefrag", FontId::new(16.0, FontFamily::Monospace),
            ACCENT,
        );
        painter.text(
            Pos2::new(rect.min.x + 14.0, rect.center().y + 9.0), egui::Align2::LEFT_CENTER,
            "NTFS Defragmentation Utility  —  github.com/arafat877/rust-defrag",
            FontId::new(8.5, FontFamily::Proportional), TEXT_DIM,
        );

        // Use egui widgets for interactive controls in the title bar area
        let controls_x = rect.max.x - 480.0;
        let ui_rect    = Rect::from_min_max(
            Pos2::new(controls_x, rect.min.y),
            rect.max,
        );
        let mut child_ui = egui::Ui::new(
            ctx.clone(),
            egui::LayerId::new(egui::Order::Foreground, egui::Id::new("titlebar")),
            egui::Id::new("titlebar_content"),
            egui::UiBuilder::new().max_rect(ui_rect),
        );
        child_ui.horizontal_centered(|ui| {
            ui.add_space(6.0);

            // Drive selector
            egui::ComboBox::from_id_source("drive_select")
                .selected_text(&self.selected_drive)
                .width(52.0)
                .show_ui(ui, |ui| {
                    for d in ["C:", "D:", "E:", "F:", "G:"] {
                        ui.selectable_value(&mut self.selected_drive, d.to_string(), d);
                    }
                });

            ui.add_space(4.0);
            ui.checkbox(&mut self.compact_mode, "Compact");
            ui.add_space(4.0);
            ui.checkbox(&mut self.boot_fallback, "Boot-retry");
            ui.add_space(8.0);

            let can_act = !matches!(self.phase, Phase::Enumerating | Phase::Analyzing | Phase::Defragging);
            let can_stop = matches!(self.phase, Phase::Enumerating | Phase::Analyzing | Phase::Defragging);

            if ui.add_enabled(can_act, egui::Button::new("Analyze")).clicked() {
                self.phase     = Phase::Enumerating;
                self.scan_progress  = 0.0;
                self.enum_progress  = 0.0;
                self.status_msg     = "Starting analysis…".into();
                self.start_timer();
                self.cluster_map    = ClusterMap::new();
                self.engine.send(EngineCommand::StartAnalysis { drive: self.selected_drive.clone() });
            }
            ui.add_space(4.0);
            if ui.add_enabled(can_act, egui::Button::new("Defragment")).clicked() {
                self.phase     = Phase::Enumerating;
                self.defrag_progress = 0.0;
                self.scan_progress   = 0.0;
                self.status_msg      = "Starting defragmentation…".into();
                self.start_timer();
                self.cluster_map     = ClusterMap::new();
                self.engine.send(EngineCommand::StartDefrag {
                    drive:        self.selected_drive.clone(),
                    compact_mode: self.compact_mode,
                    boot_fallback:self.boot_fallback,
                });
            }
            ui.add_space(4.0);
            if ui.add_enabled(can_stop, egui::Button::new("Stop")).clicked() {
                self.engine.stop();
                self.status_msg = "Stopping…".into();
            }
        });
    }

    // ── Tab bar ───────────────────────────────────────────────────────────────

    fn draw_tabs(&mut self, painter: &Painter, rect: Rect, ctx: &Context) {
        painter.rect_filled(rect, Rounding::same(5.0), BG_PANEL);
        painter.rect_stroke(rect, Rounding::same(5.0), Stroke::new(0.5, BORDER));

        let tabs = [("Overview", ActiveTab::Overview), ("Charts", ActiveTab::Charts), ("Top Fragmented", ActiveTab::TopFragmented)];
        let tab_w = 120.0;
        let mut child_ui = egui::Ui::new(
            ctx.clone(),
            egui::LayerId::new(egui::Order::Foreground, egui::Id::new("tabs")),
            egui::Id::new("tab_content"),
            egui::UiBuilder::new().max_rect(rect),
        );
        child_ui.horizontal(|ui| {
            ui.add_space(6.0);
            for (label, tab) in &tabs {
                let active = self.active_tab == *tab;
                let color  = if active { ACCENT } else { TEXT_DIM };
                let bg     = if active { with_alpha(ACCENT, 30) } else { Color32::TRANSPARENT };
                if ui.add(
                    egui::Button::new(egui::RichText::new(*label).color(color).size(10.5))
                        .fill(bg).min_size(Vec2::new(tab_w, rect.height() - 4.0))
                ).clicked() {
                    self.active_tab = *tab;
                }
                ui.add_space(2.0);
            }
        });
    }

    // ── Overview tab ──────────────────────────────────────────────────────────

    fn draw_overview(&self, painter: &Painter, rect: Rect, time: f64) {
        let pad = 4.0;

        // Left column: volume info + analysis stats
        let left_w = 180.0;
        let left_rect = Rect::from_min_size(rect.min, Vec2::new(left_w, rect.height()));
        let left_box_h = (rect.height() - 2.0 * pad) / 3.0;
        let vol_rect = Rect::from_min_size(left_rect.min, Vec2::new(left_w, left_box_h));
        self.vol_panel.draw(painter, vol_rect);

        let analysis_rect = Rect::from_min_size(
            Pos2::new(left_rect.min.x, vol_rect.max.y + pad),
            Vec2::new(left_w, left_box_h),
        );
        self.analysis_panel.draw(painter, analysis_rect);

        let defrag_rect = Rect::from_min_size(
            Pos2::new(left_rect.min.x, analysis_rect.max.y + pad),
            Vec2::new(left_w, left_box_h),
        );
        self.defrag_panel.draw(painter, defrag_rect);

        // Right: cluster maps
        let map_x    = rect.min.x + left_w + pad;
        let map_w    = rect.width() - left_w - pad;
        let map_h    = (rect.height() - pad) / 2.0;

        let map1_rect = Rect::from_min_size(Pos2::new(map_x, rect.min.y), Vec2::new(map_w, map_h));
        let map2_rect = Rect::from_min_size(Pos2::new(map_x, map1_rect.max.y + pad), Vec2::new(map_w, map_h));

        // Map labels
        draw_bold_label(
            painter,
            Pos2::new(map1_rect.min.x + 8.0, map1_rect.min.y + 12.0),
            "CURRENT DISK USAGE",
            with_alpha(TEXT_LABEL, 160),
        );
        draw_bold_label(
            painter,
            Pos2::new(map2_rect.min.x + 8.0, map2_rect.min.y + 12.0),
            "ESTIMATED AFTER DEFRAGMENTATION",
            with_alpha(TEXT_LABEL, 160),
        );

        let map1_inner = Rect::from_min_size(Pos2::new(map1_rect.min.x, map1_rect.min.y + 18.0), Vec2::new(map_w, map_h - 24.0));
        let map2_inner = Rect::from_min_size(Pos2::new(map2_rect.min.x, map2_rect.min.y + 18.0), Vec2::new(map_w, map_h - 24.0));

        self.cluster_map.draw(painter, map1_inner, time);
        self.cluster_map_after.draw(painter, map2_inner, time);

        // Legend aligned on the top row with the CURRENT DISK USAGE title.
        draw_legend_right(painter, Pos2::new(map1_rect.max.x - 8.0, map1_rect.min.y + 12.0));
    }

    // ── Charts tab ────────────────────────────────────────────────────────────

    fn draw_charts(&self, painter: &Painter, rect: Rect, time: f64) {
        use crate::ui::cluster_map::draw_glass_panel;

        let half_w = (rect.width() - 8.0) / 2.0;
        let half_h = (rect.height() - 8.0) / 2.0;
        let pad    = 8.0;

        // ── Pie ──
        let pie_r = Rect::from_min_size(rect.min, Vec2::new(half_w, half_h));
        draw_glass_panel(painter, pie_r, 8.0);
        painter.text(Pos2::new(pie_r.min.x + 10.0, pie_r.min.y + 12.0),
            egui::Align2::LEFT_CENTER, "DISK SPACE DISTRIBUTION",
            FontId::new(8.5, FontFamily::Monospace), with_alpha(TEXT_LABEL, 160));
        let pie_inner = pie_r.shrink(20.0);
        let pie_draw = Rect::from_min_size(Pos2::new(pie_inner.min.x, pie_inner.min.y + 8.0),
            Vec2::new(pie_inner.height(), pie_inner.height()));
        self.pie_chart.draw(painter, pie_draw, time);
        let legend_r = Rect::from_min_size(Pos2::new(pie_draw.max.x + 8.0, pie_draw.min.y), Vec2::new(80.0, pie_draw.height()));
        self.pie_chart.draw_legend(painter, legend_r);

        // ── Bar ──
        let bar_r = Rect::from_min_size(Pos2::new(rect.min.x + half_w + pad, rect.min.y), Vec2::new(half_w, half_h));
        draw_glass_panel(painter, bar_r, 8.0);
        painter.text(Pos2::new(bar_r.min.x + 10.0, bar_r.min.y + 12.0),
            egui::Align2::LEFT_CENTER, "FILE SIZE DISTRIBUTION",
            FontId::new(8.5, FontFamily::Monospace), with_alpha(TEXT_LABEL, 160));
        self.bar_chart.draw(painter, bar_r.shrink(20.0).translate(Vec2::new(0.0, 8.0)), time);

        // ── Fragment histogram ──
        let hist_r = Rect::from_min_size(Pos2::new(rect.min.x, rect.min.y + half_h + pad), Vec2::new(half_w, half_h));
        draw_glass_panel(painter, hist_r, 8.0);
        painter.text(Pos2::new(hist_r.min.x + 10.0, hist_r.min.y + 12.0),
            egui::Align2::LEFT_CENTER, "FRAGMENT COUNT HISTOGRAM (X = frag count)",
            FontId::new(8.5, FontFamily::Monospace), with_alpha(TEXT_LABEL, 160));
        self.hist_chart.draw(painter, hist_r.shrink(22.0).translate(Vec2::new(0.0, 8.0)), time);

        // ── Speed line chart ──
        let speed_r = Rect::from_min_size(Pos2::new(rect.min.x + half_w + pad, rect.min.y + half_h + pad), Vec2::new(half_w, half_h));
        draw_glass_panel(painter, speed_r, 8.0);
        painter.text(Pos2::new(speed_r.min.x + 10.0, speed_r.min.y + 12.0),
            egui::Align2::LEFT_CENTER, "DEFRAG SPEED  (clusters/sec)",
            FontId::new(8.5, FontFamily::Monospace), with_alpha(TEXT_LABEL, 160));
        if self.speed_chart.points.len() < 2 {
            painter.text(speed_r.center(), egui::Align2::CENTER_CENTER,
                "Data available after defragmentation",
                FontId::new(9.5, FontFamily::Proportional), TEXT_DIM);
        } else {
            self.speed_chart.draw(painter, speed_r.shrink(28.0).translate(Vec2::new(0.0, 8.0)), time);
        }
    }

    // ── Top fragmented tab ────────────────────────────────────────────────────

    fn draw_top_frag(&self, painter: &Painter, rect: Rect) {
        use crate::ui::cluster_map::draw_glass_panel;
        draw_glass_panel(painter, rect, 8.0);

        if self.top_fragmented.is_empty() {
            painter.text(rect.center(), egui::Align2::CENTER_CENTER,
                "Run analysis to see the top fragmented files",
                FontId::new(11.0, FontFamily::Proportional), TEXT_DIM);
            return;
        }

        // Header
        painter.text(Pos2::new(rect.min.x + 10.0, rect.min.y + 14.0),
            egui::Align2::LEFT_CENTER, "TOP FRAGMENTED FILES",
            FontId::new(8.5, FontFamily::Monospace), with_alpha(TEXT_LABEL, 180));

        let max_frags = self.top_fragmented.first().map(|(_, c)| *c).unwrap_or(1).max(1) as f32;
        let row_h = ((rect.height() - 30.0) / self.top_fragmented.len().max(1) as f32).min(26.0);
        let start_y = rect.min.y + 28.0;

        for (i, (name, count)) in self.top_fragmented.iter().enumerate() {
            let y     = start_y + i as f32 * row_h;
            let row_r = Rect::from_min_size(Pos2::new(rect.min.x + 6.0, y), Vec2::new(rect.width() - 12.0, row_h - 1.0));

            // Alternating background
            let bg = if i % 2 == 0 { with_alpha(ACCENT, 8) } else { Color32::TRANSPARENT };
            painter.rect_filled(row_r, Rounding::same(3.0), bg);

            // Bar
            let bar_w_max = (rect.width() - 180.0).max(100.0);
            let bar_w = (*count as f32 / max_frags) * bar_w_max;
            let bar_x = rect.min.x + 150.0;
            let bar_r = Rect::from_min_size(Pos2::new(bar_x, y + 4.0), Vec2::new(bar_w, row_h - 8.0));
            let bar_color = lerp_color(GREEN, RED, *count as f32 / max_frags);
            painter.rect_filled(bar_r, Rounding::same(2.0), with_alpha(bar_color, 80));
            painter.rect_stroke(bar_r, Rounding::same(2.0), Stroke::new(0.5, with_alpha(bar_color, 120)));

            // Rank
            painter.text(Pos2::new(rect.min.x + 16.0, y + row_h * 0.5),
                egui::Align2::LEFT_CENTER, &format!("#{}", i + 1),
                FontId::new(9.0, FontFamily::Monospace), TEXT_DIM);

            // Filename
            let short_name = if name.len() > 22 { &name[name.len()-22..] } else { name.as_str() };
            painter.text(Pos2::new(rect.min.x + 40.0, y + row_h * 0.5),
                egui::Align2::LEFT_CENTER, short_name,
                FontId::new(9.5, FontFamily::Proportional), TEXT_SEC);

            // Fragment count
            painter.text(Pos2::new(bar_x + bar_w + 6.0, y + row_h * 0.5),
                egui::Align2::LEFT_CENTER, &format!("{} frags", count),
                FontId::new(9.0, FontFamily::Monospace), bar_color);
        }
    }

    // ── Status bar ────────────────────────────────────────────────────────────

    fn draw_status_bar(&self, painter: &Painter, rect: Rect, time: f64) {
        use crate::ui::cluster_map::draw_glass_panel;
        draw_glass_panel(painter, rect, 6.0);

        // Phase dot
        let dot_color = match self.phase {
            Phase::Idle | Phase::Error                    => TEXT_DIM,
            Phase::Enumerating | Phase::Analyzing         => with_alpha(ACCENT, (pulse(time, 2.0) * 220.0) as u8),
            Phase::Defragging                             => with_alpha(GREEN,  (pulse(time, 1.5) * 220.0) as u8),
            Phase::AnalysisDone | Phase::DefragDone       => GREEN,
        };
        painter.circle_filled(Pos2::new(rect.min.x + 14.0, rect.center().y), 4.0, dot_color);

        // Status text
        painter.text(
            Pos2::new(rect.min.x + 26.0, rect.center().y), egui::Align2::LEFT_CENTER,
            &self.status_msg, FontId::new(10.0, FontFamily::Proportional),
            TEXT_SEC,
        );

        // Right side: elapsed + progress percent
        let pct = match self.phase {
            Phase::Analyzing => self.scan_progress,
            Phase::Defragging => self.defrag_progress,
            Phase::AnalysisDone | Phase::DefragDone => 1.0,
            _ => 0.0,
        };
        let elapsed_str = fmt_duration(self.elapsed_secs);
        painter.text(
            Pos2::new(rect.max.x - 80.0, rect.center().y), egui::Align2::RIGHT_CENTER,
            &elapsed_str, FontId::new(10.0, FontFamily::Monospace), TEXT_DIM,
        );
        painter.text(
            Pos2::new(rect.max.x - 12.0, rect.center().y), egui::Align2::RIGHT_CENTER,
            &format!("{:.0}%", pct * 100.0), FontId::new(10.0, FontFamily::Monospace),
            if pct >= 1.0 { GREEN } else { ACCENT },
        );

        // Progress track
        let track_rect = Rect::from_min_size(
            Pos2::new(rect.min.x + 6.0, rect.max.y - 5.0),
            Vec2::new(rect.width() - 12.0, 3.0),
        );
        painter.rect_filled(track_rect, Rounding::same(1.5), with_alpha(TEXT_LABEL, 30));
        let fill_w = track_rect.width() * pct;
        if fill_w > 0.5 {
            let fill_rect = Rect::from_min_size(track_rect.min, Vec2::new(fill_w, 3.0));
            let fill_color = match self.phase {
                Phase::Analyzing  => ACCENT,
                Phase::Defragging => GREEN,
                Phase::DefragDone => GREEN,
                _                 => ACCENT,
            };
            painter.rect_filled(fill_rect, Rounding::same(1.5), fill_color);
            if matches!(self.phase, Phase::Analyzing | Phase::Defragging) && pct < 1.0 {
                let sh   = shimmer(time);
                let sh_w = (fill_w * 0.15).min(fill_w);
                let sh_x = fill_rect.min.x + sh * (fill_w - sh_w).max(0.0);
                let sh_r = Rect::from_min_size(Pos2::new(sh_x, track_rect.min.y), Vec2::new(sh_w, 3.0));
                painter.rect_filled(sh_r, Rounding::same(1.5), with_alpha(Color32::WHITE, 60));
            }
        }
    }
}

// ── Legend helper ─────────────────────────────────────────────────────────────

fn draw_legend_right(painter: &Painter, right_pos: Pos2) {
    let items: &[(Color32, &str)] = &[
        (COLOR_FREE,   "Free"),
        (COLOR_SYSTEM, "System"),
        (COLOR_USED,   "Contiguous"),
        (COLOR_FRAG,   "Fragmented"),
        (COLOR_MOVING, "Moving"),
        (COLOR_DONE,   "Done"),
    ];
    let sw = 10.0;
    let sh = 7.0;
    let font = FontId::new(8.0, FontFamily::Monospace);
    let max_len = items.iter().map(|(_, label)| label.len()).max().unwrap_or(1) as f32;
    let char_w = painter
        .layout_no_wrap("M".to_string(), font.clone(), TEXT_DIM)
        .size()
        .x
        .max(4.0);
    let item_w = sw + 3.0 + (max_len + 2.0) * char_w;
    let total_w = item_w * items.len() as f32;
    let start_x = right_pos.x - total_w;
    for (i, (col, label)) in items.iter().enumerate() {
        let x = start_x + i as f32 * item_w;
        let swatch = Rect::from_min_size(Pos2::new(x, right_pos.y - sh * 0.5), Vec2::new(sw, sh));
        painter.rect_filled(swatch, Rounding::same(1.5), *col);
        painter.text(
            Pos2::new(x + sw + 3.0, right_pos.y),
            egui::Align2::LEFT_CENTER,
            label,
            font.clone(),
            TEXT_DIM,
        );
    }
}

fn draw_bold_label(painter: &Painter, pos: Pos2, text: &str, color: Color32) {
    let font = FontId::new(8.5, FontFamily::Monospace);
    painter.text(pos, egui::Align2::LEFT_CENTER, text, font.clone(), color);
    painter.text(pos + Vec2::new(0.7, 0.0), egui::Align2::LEFT_CENTER, text, font, color);
}


// ── Color re-exports for cluster_map ─────────────────────────────────────────

use crate::ui::theme::{COLOR_FREE, COLOR_SYSTEM, COLOR_USED, COLOR_FRAG, COLOR_MOVING, COLOR_DONE};

fn fmt_large(v: u64) -> String {
    if v >= 1_000_000 { format!("{:.1}M", v as f64 / 1_000_000.0) }
    else if v >= 1_000 { format!("{:.1}K", v as f64 / 1_000.0) }
    else { v.to_string() }
}
