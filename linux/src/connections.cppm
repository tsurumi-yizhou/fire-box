module;

#include <adwaita.h>
#include <gtk/gtk.h>
#include <string>
#include <vector>

export module connections;
import dbus_client;
import i18n;

export class ConnectionsView {
public:
    ConnectionsView() {
        scrolled_ = gtk_scrolled_window_new();
        gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(scrolled_),
                                       GTK_POLICY_NEVER, GTK_POLICY_AUTOMATIC);

        stack_ = gtk_stack_new();
        gtk_stack_set_transition_type(GTK_STACK(stack_),
                                      GTK_STACK_TRANSITION_TYPE_CROSSFADE);
        gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(scrolled_), stack_);

        GtkWidget* empty_page = adw_status_page_new();
        adw_status_page_set_icon_name(ADW_STATUS_PAGE(empty_page),
                                      "network-server-symbolic");
        adw_status_page_set_title(ADW_STATUS_PAGE(empty_page),
                                  _("No active connections"));
        gtk_stack_add_named(GTK_STACK(stack_), empty_page, "empty");

        GtkWidget* list_clamp = adw_clamp_new();
        adw_clamp_set_maximum_size(ADW_CLAMP(list_clamp), 600);

        GtkWidget* list_vbox = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
        gtk_widget_set_margin_top(list_vbox, 24);
        gtk_widget_set_margin_bottom(list_vbox, 24);
        gtk_widget_set_margin_start(list_vbox, 12);
        gtk_widget_set_margin_end(list_vbox, 12);
        adw_clamp_set_child(ADW_CLAMP(list_clamp), list_vbox);

        group_ = adw_preferences_group_new();
        adw_preferences_group_set_title(ADW_PREFERENCES_GROUP(group_),
                                        _("Active Connections"));

        GtkWidget* refresh_btn = gtk_button_new_from_icon_name(
            "view-refresh-symbolic");
        gtk_widget_set_tooltip_text(refresh_btn, _("Refresh"));
        gtk_widget_set_valign(refresh_btn, GTK_ALIGN_CENTER);
        gtk_widget_add_css_class(refresh_btn, "flat");
        adw_preferences_group_set_header_suffix(
            ADW_PREFERENCES_GROUP(group_), refresh_btn);
        refresh_button_ = refresh_btn;

        gtk_box_append(GTK_BOX(list_vbox), group_);
        gtk_stack_add_named(GTK_STACK(stack_), list_clamp, "list");

        gtk_stack_set_visible_child_name(GTK_STACK(stack_), "empty");
    }

    GtkWidget* widget() const { return scrolled_; }
    GtkWidget* refresh_button() const { return refresh_button_; }

    Task refresh(FireBoxDbusClient* client) {
        for (auto* row : rows_) {
            adw_preferences_group_remove(ADW_PREFERENCES_GROUP(group_), row);
        }
        rows_.clear();

        try {
            auto connections = co_await client->list_connections();

            if (connections.empty()) {
                gtk_stack_set_visible_child_name(GTK_STACK(stack_), "empty");
                co_return;
            }

            for (const auto& conn : connections) {
                GtkWidget* row = adw_action_row_new();
                adw_preferences_row_set_title(ADW_PREFERENCES_ROW(row),
                                              conn.client_name.c_str());

                std::string subtitle = conn.app_path + " | " +
                    std::to_string(conn.requests_count) + " " + _("requests");
                adw_action_row_set_subtitle(ADW_ACTION_ROW(row),
                                            subtitle.c_str());

                adw_preferences_group_add(ADW_PREFERENCES_GROUP(group_), row);
                rows_.push_back(row);
            }

            gtk_stack_set_visible_child_name(GTK_STACK(stack_), "list");

        } catch (const std::exception& e) {
            gtk_stack_set_visible_child_name(GTK_STACK(stack_), "empty");
        }
    }

private:
    GtkWidget* scrolled_       = nullptr;
    GtkWidget* stack_          = nullptr;
    GtkWidget* group_          = nullptr;
    GtkWidget* refresh_button_ = nullptr;
    std::vector<GtkWidget*> rows_;
};