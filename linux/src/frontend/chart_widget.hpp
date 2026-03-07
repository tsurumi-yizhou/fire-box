#pragma once
/// @file chart_widget.hpp
/// Self-drawn line chart using GtkDrawingArea.

#include <gtkmm.h>
#include <deque>

namespace firebox::frontend {

class ChartWidget : public Gtk::DrawingArea {
public:
    ChartWidget();

    /// Add a data point to the chart. Old points scroll off.
    void add_data_point(double value);

    /// Clear all data.
    void clear();

private:
    void on_draw(const Cairo::RefPtr<Cairo::Context>& cr,
                 int width, int height);

    std::deque<double> data_;
    static constexpr size_t MAX_POINTS = 120; // ~2 minutes at 1s interval
};

} // namespace firebox::frontend
