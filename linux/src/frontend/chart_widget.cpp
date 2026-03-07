/// @file chart_widget.cpp
#include "chart_widget.hpp"
#include <algorithm>
#include <cmath>

namespace firebox::frontend {

ChartWidget::ChartWidget() {
    set_content_width(400);
    set_content_height(200);
    add_css_class("card");
    set_draw_func(sigc::mem_fun(*this, &ChartWidget::on_draw));
}

void ChartWidget::add_data_point(double value) {
    data_.push_back(value);
    if (data_.size() > MAX_POINTS)
        data_.pop_front();
    queue_draw();
}

void ChartWidget::clear() {
    data_.clear();
    queue_draw();
}

void ChartWidget::on_draw(const Cairo::RefPtr<Cairo::Context>& cr,
                           int width, int height) {
    const double pad_l   = 36.0;
    const double pad_r   = 12.0;
    const double pad_t   = 16.0;
    const double pad_b   = 8.0;
    const double chart_w = width  - pad_l - pad_r;
    const double chart_h = height - pad_t - pad_b;

    // ── Theme colors via GTK4 ────────────────────────────────────
    GdkRGBA fg;
    gtk_widget_get_color(GTK_WIDGET(gobj()), &fg);
    bool is_dark = (fg.red + fg.green + fg.blue) > 1.5;

    GdkRGBA line_clr = is_dark
        ? GdkRGBA{0.40, 0.75, 1.00, 1.0}   // brighter in dark mode
        : GdkRGBA{0.13, 0.45, 0.80, 1.0};  // deeper in light mode

    if (data_.empty() || chart_w <= 0 || chart_h <= 0) {
        cr->set_source_rgba(fg.red, fg.green, fg.blue, 0.4);
        cr->select_font_face("sans", Cairo::ToyFontFace::Slant::NORMAL,
                             Cairo::ToyFontFace::Weight::NORMAL);
        cr->set_font_size(12.0);
        Cairo::TextExtents ext;
        cr->get_text_extents("No data", ext);
        cr->move_to(width / 2.0 - ext.width / 2.0,
                    height / 2.0 + ext.height / 2.0);
        cr->show_text("No data");
        return;
    }

    double mn = *std::min_element(data_.begin(), data_.end());
    double mx = *std::max_element(data_.begin(), data_.end());
    if (mx == mn) mx = mn + 1.0;

    double step_x = (data_.size() > 1)
        ? chart_w / static_cast<double>(data_.size() - 1)
        : chart_w;

    auto val_y = [&](double v) {
        return pad_t + chart_h * (1.0 - (v - mn) / (mx - mn));
    };

    // ── Grid lines ──────────────────────────────────────────────
    cr->set_source_rgba(fg.red, fg.green, fg.blue, 0.12);
    cr->set_line_width(0.5);
    for (int i = 0; i <= 4; ++i) {
        double y = pad_t + chart_h * (1.0 - i / 4.0);
        cr->move_to(pad_l, y);
        cr->line_to(pad_l + chart_w, y);
        cr->stroke();
    }

    // ── Fill under curve ────────────────────────────────────────
    cr->set_source_rgba(line_clr.red, line_clr.green, line_clr.blue, 0.10);
    cr->move_to(pad_l, pad_t + chart_h);
    cr->line_to(pad_l, val_y(data_[0]));
    for (size_t i = 1; i < data_.size(); ++i)
        cr->line_to(pad_l + i * step_x, val_y(data_[i]));
    cr->line_to(pad_l + (data_.size() - 1) * step_x, pad_t + chart_h);
    cr->close_path();
    cr->fill();

    // ── Line ────────────────────────────────────────────────────
    cr->set_source_rgba(line_clr.red, line_clr.green, line_clr.blue, 1.0);
    cr->set_line_width(2.0);
    cr->move_to(pad_l, val_y(data_[0]));
    for (size_t i = 1; i < data_.size(); ++i)
        cr->line_to(pad_l + i * step_x, val_y(data_[i]));
    cr->stroke();

    // ── Dots (skip when too dense) ──────────────────────────────
    if (data_.size() <= 60) {
        for (size_t i = 0; i < data_.size(); ++i) {
            cr->arc(pad_l + i * step_x, val_y(data_[i]), 3.0, 0, 2 * M_PI);
            cr->fill();
        }
    }

    // ── Y-axis labels ────────────────────────────────────────────
    cr->set_source_rgba(fg.red, fg.green, fg.blue, 0.55);
    cr->set_font_size(9.0);
    for (int i = 0; i <= 4; ++i) {
        double y   = pad_t + chart_h * (1.0 - i / 4.0);
        double val = mn + (mx - mn) * (i / 4.0);
        auto   txt = std::to_string(static_cast<int>(val));
        Cairo::TextExtents ext;
        cr->get_text_extents(txt, ext);
        cr->move_to(pad_l - ext.width - 4, y + ext.height / 2.0);
        cr->show_text(txt);
    }
}

} // namespace firebox::frontend
