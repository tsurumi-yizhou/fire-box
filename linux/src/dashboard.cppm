module;

#include <adwaita.h>
#include <gtk/gtk.h>
#include <cstdio>
#include <string>

export module dashboard;
import dbus_client;
import i18n;

export struct DashboardRefreshData {
    class DashboardView* view;
    FireBoxDbusClient*   client;
};

export class DashboardView {
public:
    explicit DashboardView(FireBoxDbusClient* client) {
        clamp_ = adw_clamp_new();
        adw_clamp_set_maximum_size(ADW_CLAMP(clamp_), 600);

        GtkWidget* vbox = gtk_box_new(GTK_ORIENTATION_VERTICAL, 12);
        gtk_widget_set_margin_top(vbox, 24);
        gtk_widget_set_margin_bottom(vbox, 24);
        gtk_widget_set_margin_start(vbox, 12);
        gtk_widget_set_margin_end(vbox, 12);
        adw_clamp_set_child(ADW_CLAMP(clamp_), vbox);

        status_label_ = gtk_label_new(_("Connecting..."));
        gtk_widget_add_css_class(status_label_, "title-4");
        gtk_widget_set_halign(status_label_, GTK_ALIGN_START);
        gtk_box_append(GTK_BOX(vbox), status_label_);

        GtkWidget* group = adw_preferences_group_new();
        adw_preferences_group_set_title(ADW_PREFERENCES_GROUP(group),
                                        _("Service Metrics"));
        gtk_box_append(GTK_BOX(vbox), group);

        auto make_row = [&](const char* title, GtkWidget** value_label) {
            GtkWidget* row = adw_action_row_new();
            adw_preferences_row_set_title(ADW_PREFERENCES_ROW(row), title);

            *value_label = gtk_label_new("0");
            gtk_widget_set_valign(*value_label, GTK_ALIGN_CENTER);
            adw_action_row_add_suffix(ADW_ACTION_ROW(row), *value_label);

            adw_preferences_group_add(ADW_PREFERENCES_GROUP(group), row);
        };

        make_row(_("Requests Total"),      &requests_total_label_);
        make_row(_("Requests Failed"),      &requests_failed_label_);
        make_row(_("Prompt Tokens"),        &prompt_tokens_label_);
        make_row(_("Completion Tokens"),    &completion_tokens_label_);
        make_row(_("Cost Total"),           &cost_total_label_);
        make_row(_("Latency Avg"),          &latency_avg_label_);
        make_row(_("Active Connections"),   &active_connections_label_);

        refresh_data_ = new DashboardRefreshData{this, client};
        refresh_timer_id_ = g_timeout_add_seconds(2, on_auto_refresh, refresh_data_);
    }

    ~DashboardView() {
        if (refresh_timer_id_ != 0) {
            g_source_remove(refresh_timer_id_);
            refresh_timer_id_ = 0;
        }
        if (refresh_data_) {
            refresh_data_->view = nullptr;
            refresh_data_->client = nullptr;
        }
    }

    GtkWidget* widget() const { return clamp_; }

    Task refresh(FireBoxDbusClient* client) {
        try {
            auto metrics = co_await client->get_metrics_snapshot();

            set_label(requests_total_label_,
                      std::to_string(metrics.requests_total).c_str());
            set_label(requests_failed_label_,
                      std::to_string(metrics.requests_failed).c_str());
            set_label(prompt_tokens_label_,
                      std::to_string(metrics.prompt_tokens).c_str());
            set_label(completion_tokens_label_,
                      std::to_string(metrics.completion_tokens).c_str());

            char cost_buf[32];
            std::snprintf(cost_buf, sizeof(cost_buf), "$%.4f",
                          metrics.cost_total);
            set_label(cost_total_label_, cost_buf);

            char latency_buf[32];
            std::snprintf(latency_buf, sizeof(latency_buf), "%lu ms",
                          static_cast<unsigned long>(metrics.latency_avg_ms));
            set_label(latency_avg_label_, latency_buf);

            auto connections = co_await client->list_connections();
            set_label(active_connections_label_,
                      std::to_string(connections.size()).c_str());

            gtk_label_set_text(
                GTK_LABEL(status_label_),
                _("Connected"));
            gtk_widget_remove_css_class(status_label_, "error");
            gtk_widget_add_css_class(status_label_, "success");

        } catch (const std::exception& e) {
            gtk_label_set_text(GTK_LABEL(status_label_),
                               _("Disconnected"));
            gtk_widget_remove_css_class(status_label_, "success");
            gtk_widget_add_css_class(status_label_, "error");
        }
    }

private:
    GtkWidget* clamp_                    = nullptr;
    GtkWidget* status_label_             = nullptr;
    GtkWidget* requests_total_label_     = nullptr;
    GtkWidget* requests_failed_label_    = nullptr;
    GtkWidget* prompt_tokens_label_      = nullptr;
    GtkWidget* completion_tokens_label_  = nullptr;
    GtkWidget* cost_total_label_         = nullptr;
    GtkWidget* latency_avg_label_        = nullptr;
    GtkWidget* active_connections_label_ = nullptr;

    DashboardRefreshData* refresh_data_  = nullptr;
    guint                 refresh_timer_id_ = 0;

    static void set_label(GtkWidget* label, const char* text) {
        gtk_label_set_text(GTK_LABEL(label), text);
    }

    static gboolean on_auto_refresh(gpointer user_data) {
        auto* data = static_cast<DashboardRefreshData*>(user_data);
        if (data && data->view && data->client) {
            data->view->refresh(data->client);
        }
        return G_SOURCE_CONTINUE;
    }
};