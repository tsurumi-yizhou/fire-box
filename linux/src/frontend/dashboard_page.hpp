#pragma once
/// @file dashboard_page.hpp
/// Dashboard page: service control, KPIs, and real-time chart.

#include "chart_widget.hpp"
#include <gtkmm.h>
#include <sdbus-c++/sdbus-c++.h>

namespace firebox::frontend {

class DashboardPage : public Gtk::Box {
public:
    explicit DashboardPage(sdbus::IProxy* proxy);

private:
    void setup_ui();
    void refresh_metrics();
    void on_start_stop_clicked();
    bool on_auto_refresh();

    sdbus::IProxy* proxy_;

    // Service control
    Gtk::Button* start_stop_btn_ = nullptr;
    Gtk::Label* status_label_ = nullptr;

    // KPIs
    Gtk::Label* tokens_label_ = nullptr;
    Gtk::Label* requests_label_ = nullptr;
    Gtk::Label* cost_label_ = nullptr;

    // Chart
    ChartWidget* chart_ = nullptr;

    sigc::connection refresh_timer_;
};

} // namespace firebox::frontend
