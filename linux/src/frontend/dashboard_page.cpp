/// @file dashboard_page.cpp
#include "dashboard_page.hpp"
#include <spdlog/spdlog.h>
#include <gio/gio.h>

namespace firebox::frontend {

DashboardPage::DashboardPage(sdbus::IProxy* proxy)
    : Gtk::Box(Gtk::Orientation::VERTICAL, 12)
    , proxy_(proxy) {
    set_margin(16);
    setup_ui();

    // Start auto-refresh timer (1 second)
    refresh_timer_ = Glib::signal_timeout().connect(
        sigc::mem_fun(*this, &DashboardPage::on_auto_refresh), 1000);

    refresh_metrics();
}

void DashboardPage::setup_ui() {
    // ── Header with service control ──────────────────────────
    auto* header = Gtk::manage(new Gtk::Box(Gtk::Orientation::HORIZONTAL, 8));

    status_label_ = Gtk::manage(new Gtk::Label("● Unknown"));
    status_label_->add_css_class("dim-label");
    header->append(*status_label_);

    auto* spacer = Gtk::manage(new Gtk::Box());
    spacer->set_hexpand(true);
    header->append(*spacer);

    start_stop_btn_ = Gtk::manage(new Gtk::Button("Start / Stop"));
    start_stop_btn_->add_css_class("suggested-action");
    start_stop_btn_->signal_clicked().connect(
        sigc::mem_fun(*this, &DashboardPage::on_start_stop_clicked));
    header->append(*start_stop_btn_);

    append(*header);

    // ── KPI Cards ────────────────────────────────────────────
    auto* kpi_box = Gtk::manage(new Gtk::Box(Gtk::Orientation::HORIZONTAL, 12));
    kpi_box->set_homogeneous(true);

    auto make_kpi = [](const char* title_text, Gtk::Label*& value_label) {
        auto* card = Gtk::manage(new Gtk::Box(Gtk::Orientation::VERTICAL, 4));
        card->add_css_class("card");
        card->set_margin(4);
        card->set_margin_top(0);
        card->set_margin_bottom(0);
        auto* inner = Gtk::manage(new Gtk::Box(Gtk::Orientation::VERTICAL, 4));
        inner->set_margin(16);
        auto* lbl = Gtk::manage(new Gtk::Label(title_text));
        lbl->add_css_class("dim-label");
        lbl->set_xalign(0);
        inner->append(*lbl);
        value_label = Gtk::manage(new Gtk::Label("—"));
        value_label->add_css_class("title-2");
        value_label->set_xalign(0);
        inner->append(*value_label);
        card->append(*inner);
        return card;
    };

    kpi_box->append(*make_kpi("Total Tokens", tokens_label_));
    kpi_box->append(*make_kpi("Requests", requests_label_));
    kpi_box->append(*make_kpi("Estimated Cost", cost_label_));
    append(*kpi_box);

    // ── Chart ────────────────────────────────────────────────
    chart_ = Gtk::manage(new ChartWidget());
    chart_->set_vexpand(true);
    append(*chart_);
}

void DashboardPage::refresh_metrics() {
    if (!proxy_) {
        status_label_->set_text("Not connected");
        return;
    }

    try {
        std::tuple<bool, std::string, int64_t, int64_t,
                   int64_t, int64_t, int64_t, int64_t, double> result;
        proxy_->callMethod("GetMetricsSnapshot")
            .onInterface("org.firebox.Control")
            .storeResultsTo(result);

        auto& [success, message, wstart, wend,
               req_total, req_failed, prompt_tok, comp_tok, cost] = result;

        if (success) {
            status_label_->set_text("● Running");
            status_label_->remove_css_class("error");
            status_label_->add_css_class("success");
            tokens_label_->set_text(
                std::to_string(prompt_tok + comp_tok));
            requests_label_->set_text(
                std::to_string(req_total) + " / " + std::to_string(req_failed) + " failed");
            cost_label_->set_text("$" + std::to_string(cost).substr(0, 6));

            chart_->add_data_point(static_cast<double>(req_total));
        }
    } catch (const sdbus::Error& e) {
        status_label_->set_text("● Disconnected");
        status_label_->remove_css_class("success");
        status_label_->add_css_class("error");
        spdlog::warn("Metrics refresh failed: {}", e.what());
    }
}

void DashboardPage::on_start_stop_clicked() {
    start_stop_btn_->set_sensitive(false);
    status_label_->set_text("● Processing…");

    GError* err = nullptr;
    GSubprocess* check = g_subprocess_new(
        G_SUBPROCESS_FLAGS_NONE, &err,
        "systemctl", "--user", "is-active", "--quiet", "firebox.service", nullptr);
    if (!check) {
        spdlog::error("Failed to check service status: {}",
                      err ? err->message : "unknown");
        if (err) g_error_free(err);
        start_stop_btn_->set_sensitive(true);
        return;
    }

    g_subprocess_wait_check_async(check, nullptr,
        [](GObject* src, GAsyncResult* res, gpointer data) {
            auto* self = static_cast<DashboardPage*>(data);
            GError* e = nullptr;
            bool is_active = g_subprocess_wait_check_finish(
                G_SUBPROCESS(src), res, &e);
            if (e) g_error_free(e);
            g_object_unref(src);

            const char* action = is_active ? "stop" : "start";
            self->status_label_->set_text(
                is_active ? "● Stopping…" : "● Starting…");

            GError* err2 = nullptr;
            GSubprocess* cmd = g_subprocess_new(
                G_SUBPROCESS_FLAGS_NONE, &err2,
                "systemctl", "--user", action, "firebox.service", nullptr);
            if (!cmd) {
                spdlog::error("Failed to {} service: {}",
                    action, err2 ? err2->message : "unknown");
                if (err2) g_error_free(err2);
                self->start_stop_btn_->set_sensitive(true);
                return;
            }

            g_subprocess_wait_async(cmd, nullptr,
                [](GObject* src2, GAsyncResult* res2, gpointer data2) {
                    auto* self2 = static_cast<DashboardPage*>(data2);
                    g_subprocess_wait_finish(G_SUBPROCESS(src2), res2, nullptr);
                    g_object_unref(src2);
                    self2->start_stop_btn_->set_sensitive(true);
                }, self);
        }, this);
}

bool DashboardPage::on_auto_refresh() {
    refresh_metrics();
    return true; // keep timer alive
}

} // namespace firebox::frontend
