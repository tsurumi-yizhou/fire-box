#pragma once

#include "dbus_client.hpp"

#include <adwaita.h>
#include <gtk/gtk.h>
#include <libintl.h>

#include <string>
#include <vector>

#define _(S) gettext(S)

/// Providers view — lists configured LLM providers with add/delete actions.
///
/// GtkStack pages:
///   - "empty" : Adw.StatusPage placeholder
///   - "list"  : Adw.PreferencesGroup with one Adw.ActionRow per provider
class ProvidersView {
public:
    ProvidersView() {
        scrolled_ = gtk_scrolled_window_new();
        gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(scrolled_),
                                       GTK_POLICY_NEVER, GTK_POLICY_AUTOMATIC);

        stack_ = gtk_stack_new();
        gtk_stack_set_transition_type(GTK_STACK(stack_),
                                      GTK_STACK_TRANSITION_TYPE_CROSSFADE);
        gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(scrolled_), stack_);

        // ---- empty state --------------------------------------------------
        GtkWidget* empty_page = adw_status_page_new();
        adw_status_page_set_icon_name(ADW_STATUS_PAGE(empty_page),
                                      "application-x-addon-symbolic");
        adw_status_page_set_title(ADW_STATUS_PAGE(empty_page),
                                  _("No providers configured"));
        adw_status_page_set_description(ADW_STATUS_PAGE(empty_page),
            _("Add a provider to start using Fire Box."));
        gtk_stack_add_named(GTK_STACK(stack_), empty_page, "empty");

        // ---- list state ---------------------------------------------------
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
                                        _("Providers"));
        gtk_box_append(GTK_BOX(list_vbox), group_);
        gtk_stack_add_named(GTK_STACK(stack_), list_clamp, "list");

        gtk_stack_set_visible_child_name(GTK_STACK(stack_), "empty");
    }

    GtkWidget* widget() const { return scrolled_; }

    /// Refresh the provider list from the service.
    void refresh(FireBoxDbusClient& client) {
        // Remove old rows
        for (auto* row : rows_) {
            adw_preferences_group_remove(ADW_PREFERENCES_GROUP(group_), row);
        }
        rows_.clear();

        try {
            auto providers = client.list_providers();

            if (providers.empty()) {
                gtk_stack_set_visible_child_name(GTK_STACK(stack_), "empty");
                return;
            }

            for (const auto& prov : providers) {
                GtkWidget* row = adw_action_row_new();
                adw_preferences_row_set_title(ADW_PREFERENCES_ROW(row),
                                              prov.name.c_str());

                std::string subtitle = prov.base_url.empty()
                    ? type_label(prov.type)
                    : prov.base_url;
                adw_action_row_set_subtitle(ADW_ACTION_ROW(row),
                                            subtitle.c_str());

                // Delete button suffix
                GtkWidget* del_btn = gtk_button_new_from_icon_name(
                    "user-trash-symbolic");
                gtk_widget_set_valign(del_btn, GTK_ALIGN_CENTER);
                gtk_widget_add_css_class(del_btn, "flat");
                gtk_widget_add_css_class(del_btn, "error");
                gtk_widget_set_tooltip_text(del_btn, _("Delete provider"));

                // Store provider id and view pointer in the button
                auto* ctx = new DeleteContext{this, &client, prov.id};
                g_object_set_data_full(G_OBJECT(del_btn), "ctx", ctx,
                    [](gpointer p) { delete static_cast<DeleteContext*>(p); });
                g_signal_connect(del_btn, "clicked",
                                 G_CALLBACK(on_delete_clicked), nullptr);

                adw_action_row_add_suffix(ADW_ACTION_ROW(row), del_btn);
                adw_preferences_group_add(ADW_PREFERENCES_GROUP(group_), row);
                rows_.push_back(row);
            }

            gtk_stack_set_visible_child_name(GTK_STACK(stack_), "list");

        } catch (const std::exception& e) {
            gtk_stack_set_visible_child_name(GTK_STACK(stack_), "empty");
        }
    }

    /// Show the "Add Provider" flow.
    ///
    /// First presents a choice dialog (API Key / OAuth / Local), then opens the
    /// appropriate entry dialog.
    void show_add_dialog(GtkWindow* parent, FireBoxDbusClient& client) {
        // ---- Step 1: choose provider kind ---------------------------------
        AdwMessageDialog* chooser = ADW_MESSAGE_DIALOG(
            adw_message_dialog_new(parent,
                                   _("Add Provider"),
                                   _("Choose the authentication method for the new provider.")));
        adw_message_dialog_add_response(chooser, "cancel", _("Cancel"));
        adw_message_dialog_add_response(chooser, "api_key", _("API Key"));
        adw_message_dialog_add_response(chooser, "oauth",   _("OAuth"));
        adw_message_dialog_add_response(chooser, "local",   _("Local Model"));
        adw_message_dialog_set_default_response(chooser, "cancel");
        adw_message_dialog_set_close_response(chooser, "cancel");

        auto* add_ctx = new AddContext{this, parent, &client};
        g_signal_connect(chooser, "response",
                         G_CALLBACK(on_chooser_response), add_ctx);
        gtk_window_present(GTK_WINDOW(chooser));
    }

private:
    GtkWidget* scrolled_ = nullptr;
    GtkWidget* stack_    = nullptr;
    GtkWidget* group_    = nullptr;

    std::vector<GtkWidget*> rows_;

    // -----------------------------------------------------------------------
    // Delete confirmation
    // -----------------------------------------------------------------------

    struct DeleteContext {
        ProvidersView*     view;
        FireBoxDbusClient* client;
        std::string        provider_id;
    };

    static void on_delete_clicked(GtkButton* button, gpointer /*unused*/) {
        auto* ctx = static_cast<DeleteContext*>(
            g_object_get_data(G_OBJECT(button), "ctx"));
        if (!ctx) return;

        GtkRoot* root = gtk_widget_get_root(GTK_WIDGET(button));
        GtkWindow* win = GTK_IS_WINDOW(root) ? GTK_WINDOW(root) : nullptr;

        AdwMessageDialog* dlg = ADW_MESSAGE_DIALOG(
            adw_message_dialog_new(win,
                                   _("Delete Provider"),
                                   _("Are you sure you want to delete this provider? "
                                     "This action cannot be undone.")));
        adw_message_dialog_add_response(dlg, "cancel", _("Cancel"));
        adw_message_dialog_add_response(dlg, "delete", _("Delete"));
        adw_message_dialog_set_response_appearance(
            dlg, "delete", ADW_RESPONSE_DESTRUCTIVE);
        adw_message_dialog_set_default_response(dlg, "cancel");
        adw_message_dialog_set_close_response(dlg, "cancel");

        // Copy context for the response handler (button's ctx may outlive dialog)
        auto* confirm_ctx = new DeleteContext{ctx->view, ctx->client,
                                              ctx->provider_id};
        g_signal_connect(dlg, "response",
                         G_CALLBACK(on_delete_confirmed), confirm_ctx);
        gtk_window_present(GTK_WINDOW(dlg));
    }

    static void on_delete_confirmed(AdwMessageDialog* dlg,
                                    const char* response,
                                    gpointer user_data) {
        auto* ctx = static_cast<DeleteContext*>(user_data);
        if (g_strcmp0(response, "delete") == 0 && ctx) {
            try {
                ctx->client->delete_provider(ctx->provider_id);
                ctx->view->refresh(*ctx->client);
            } catch (const std::exception& e) {
                show_error(GTK_WINDOW(dlg), e.what());
            }
        }
        delete ctx;
        gtk_window_destroy(GTK_WINDOW(dlg));
    }

    // -----------------------------------------------------------------------
    // Add provider — chooser response
    // -----------------------------------------------------------------------

    struct AddContext {
        ProvidersView*     view;
        GtkWindow*         parent;
        FireBoxDbusClient* client;
    };

    static void on_chooser_response(AdwMessageDialog* dlg,
                                    const char* response,
                                    gpointer user_data) {
        auto* ctx = static_cast<AddContext*>(user_data);
        gtk_window_destroy(GTK_WINDOW(dlg));

        if (!ctx) return;

        if (g_strcmp0(response, "api_key") == 0) {
            show_api_key_dialog(ctx);
        } else if (g_strcmp0(response, "oauth") == 0) {
            show_oauth_dialog(ctx);
        } else if (g_strcmp0(response, "local") == 0) {
            show_local_dialog(ctx);
        } else {
            delete ctx;
        }
    }

    // -----------------------------------------------------------------------
    // API Key dialog
    // -----------------------------------------------------------------------

    struct ApiKeyDialogWidgets {
        GtkWidget* name_entry;
        GtkWidget* type_combo;
        GtkWidget* api_key_entry;
        GtkWidget* base_url_entry;
        AddContext* ctx;
    };

    static void show_api_key_dialog(AddContext* ctx) {
        GtkWidget* dlg = adw_message_dialog_new(
            ctx->parent,
            _("Add API Key Provider"),
            _("Enter provider details."));
        adw_message_dialog_add_response(ADW_MESSAGE_DIALOG(dlg), "cancel",
                                        _("Cancel"));
        adw_message_dialog_add_response(ADW_MESSAGE_DIALOG(dlg), "add",
                                        _("Add"));
        adw_message_dialog_set_response_appearance(
            ADW_MESSAGE_DIALOG(dlg), "add", ADW_RESPONSE_SUGGESTED);
        adw_message_dialog_set_default_response(ADW_MESSAGE_DIALOG(dlg), "add");
        adw_message_dialog_set_close_response(ADW_MESSAGE_DIALOG(dlg), "cancel");

        // Build extra content
        GtkWidget* box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 8);
        gtk_widget_set_margin_start(box, 12);
        gtk_widget_set_margin_end(box, 12);

        // Name entry
        GtkWidget* name_entry = gtk_entry_new();
        gtk_entry_set_placeholder_text(GTK_ENTRY(name_entry), _("Display name"));
        gtk_box_append(GTK_BOX(box), name_entry);

        // Provider type combo
        const char* api_key_types[] = {"openai", "anthropic", "ollama", "vllm", nullptr};
        GtkWidget* type_combo = gtk_drop_down_new_from_strings(api_key_types);
        gtk_box_append(GTK_BOX(box), type_combo);

        // API key entry
        GtkWidget* api_key_entry = gtk_password_entry_new();
        gtk_password_entry_set_show_peek_icon(GTK_PASSWORD_ENTRY(api_key_entry),
                                              TRUE);
        gtk_editable_set_text(GTK_EDITABLE(api_key_entry), "");
        // Use a regular entry with placeholder as password entry doesn't have it
        GtkWidget* api_key_label = gtk_label_new(_("API Key"));
        gtk_widget_set_halign(api_key_label, GTK_ALIGN_START);
        gtk_widget_add_css_class(api_key_label, "caption");
        gtk_box_append(GTK_BOX(box), api_key_label);
        gtk_box_append(GTK_BOX(box), api_key_entry);

        // Base URL entry
        GtkWidget* base_url_entry = gtk_entry_new();
        gtk_entry_set_placeholder_text(GTK_ENTRY(base_url_entry),
                                       _("Base URL (optional)"));
        gtk_box_append(GTK_BOX(box), base_url_entry);

        adw_message_dialog_set_extra_child(ADW_MESSAGE_DIALOG(dlg), box);

        auto* widgets = new ApiKeyDialogWidgets{
            name_entry, type_combo, api_key_entry, base_url_entry, ctx};
        g_signal_connect(dlg, "response",
                         G_CALLBACK(on_api_key_response), widgets);
        gtk_window_present(GTK_WINDOW(dlg));
    }

    static void on_api_key_response(AdwMessageDialog* dlg,
                                    const char* response,
                                    gpointer user_data) {
        auto* w = static_cast<ApiKeyDialogWidgets*>(user_data);
        if (g_strcmp0(response, "add") == 0 && w && w->ctx) {
            std::string name     = gtk_editable_get_text(GTK_EDITABLE(w->name_entry));
            guint type_idx       = gtk_drop_down_get_selected(GTK_DROP_DOWN(w->type_combo));
            std::string api_key  = gtk_editable_get_text(GTK_EDITABLE(w->api_key_entry));
            std::string base_url = gtk_editable_get_text(GTK_EDITABLE(w->base_url_entry));

            const char* type_slugs[] = {"openai", "anthropic", "ollama", "vllm"};
            std::string ptype = (type_idx < 4) ? type_slugs[type_idx] : "openai";

            try {
                w->ctx->client->add_api_key_provider(name, ptype, api_key,
                                                     base_url);
                w->ctx->view->refresh(*w->ctx->client);
            } catch (const std::exception& e) {
                show_error(GTK_WINDOW(dlg), e.what());
            }
        }
        if (w) {
            delete w->ctx;
            delete w;
        }
        gtk_window_destroy(GTK_WINDOW(dlg));
    }

    // -----------------------------------------------------------------------
    // OAuth dialog
    // -----------------------------------------------------------------------

    struct OAuthDialogWidgets {
        GtkWidget* name_entry;
        GtkWidget* type_combo;
        AddContext* ctx;
    };

    static void show_oauth_dialog(AddContext* ctx) {
        GtkWidget* dlg = adw_message_dialog_new(
            ctx->parent,
            _("Add OAuth Provider"),
            _("Enter provider details. A device code will be shown "
              "for you to authorize."));
        adw_message_dialog_add_response(ADW_MESSAGE_DIALOG(dlg), "cancel",
                                        _("Cancel"));
        adw_message_dialog_add_response(ADW_MESSAGE_DIALOG(dlg), "start",
                                        _("Start OAuth"));
        adw_message_dialog_set_response_appearance(
            ADW_MESSAGE_DIALOG(dlg), "start", ADW_RESPONSE_SUGGESTED);
        adw_message_dialog_set_default_response(ADW_MESSAGE_DIALOG(dlg),
                                                "start");
        adw_message_dialog_set_close_response(ADW_MESSAGE_DIALOG(dlg),
                                              "cancel");

        GtkWidget* box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 8);
        gtk_widget_set_margin_start(box, 12);
        gtk_widget_set_margin_end(box, 12);

        GtkWidget* name_entry = gtk_entry_new();
        gtk_entry_set_placeholder_text(GTK_ENTRY(name_entry), _("Display name"));
        gtk_box_append(GTK_BOX(box), name_entry);

        const char* oauth_types[] = {"copilot", "dashscope", nullptr};
        GtkWidget* type_combo = gtk_drop_down_new_from_strings(oauth_types);
        gtk_box_append(GTK_BOX(box), type_combo);

        adw_message_dialog_set_extra_child(ADW_MESSAGE_DIALOG(dlg), box);

        auto* widgets = new OAuthDialogWidgets{name_entry, type_combo, ctx};
        g_signal_connect(dlg, "response",
                         G_CALLBACK(on_oauth_start_response), widgets);
        gtk_window_present(GTK_WINDOW(dlg));
    }

    static void on_oauth_start_response(AdwMessageDialog* dlg,
                                        const char* response,
                                        gpointer user_data) {
        auto* w = static_cast<OAuthDialogWidgets*>(user_data);
        if (g_strcmp0(response, "start") == 0 && w && w->ctx) {
            std::string name = gtk_editable_get_text(GTK_EDITABLE(w->name_entry));
            guint type_idx   = gtk_drop_down_get_selected(GTK_DROP_DOWN(w->type_combo));

            const char* oauth_slugs[] = {"copilot", "dashscope"};
            std::string ptype = (type_idx < 2) ? oauth_slugs[type_idx] : "copilot";

            try {
                auto [provider_id, challenge] =
                    w->ctx->client->add_oauth_provider(name, ptype);

                // Show device code dialog
                gtk_window_destroy(GTK_WINDOW(dlg));
                show_device_code_dialog(w->ctx, provider_id, challenge);
                delete w;
                return;

            } catch (const std::exception& e) {
                show_error(GTK_WINDOW(dlg), e.what());
            }
        }
        if (w) {
            delete w->ctx;
            delete w;
        }
        gtk_window_destroy(GTK_WINDOW(dlg));
    }

    /// Show the device code to the user and wait for completion.
    static void show_device_code_dialog(AddContext* ctx,
                                        const std::string& provider_id,
                                        const OAuthChallenge& challenge) {
        GtkWidget* dlg = adw_message_dialog_new(
            ctx->parent,
            _("Authorize Device"),
            nullptr);

        std::string body = std::string(_("Open the following URL and enter the code:")) +
            "\n\n" + challenge.verification_uri +
            "\n\n" + _("Code: ") + challenge.user_code;
        adw_message_dialog_set_body(ADW_MESSAGE_DIALOG(dlg), body.c_str());
        adw_message_dialog_set_body_use_markup(ADW_MESSAGE_DIALOG(dlg), FALSE);

        adw_message_dialog_add_response(ADW_MESSAGE_DIALOG(dlg), "cancel",
                                        _("Cancel"));
        adw_message_dialog_add_response(ADW_MESSAGE_DIALOG(dlg), "done",
                                        _("I've Authorized"));
        adw_message_dialog_set_response_appearance(
            ADW_MESSAGE_DIALOG(dlg), "done", ADW_RESPONSE_SUGGESTED);
        adw_message_dialog_set_default_response(ADW_MESSAGE_DIALOG(dlg), "done");
        adw_message_dialog_set_close_response(ADW_MESSAGE_DIALOG(dlg), "cancel");

        struct DeviceCtx {
            AddContext* add_ctx;
            std::string provider_id;
        };
        auto* dctx = new DeviceCtx{ctx, provider_id};

        g_signal_connect(dlg, "response",
                         G_CALLBACK(+[](AdwMessageDialog* d,
                                        const char* resp,
                                        gpointer ud) {
            auto* dc = static_cast<DeviceCtx*>(ud);
            if (g_strcmp0(resp, "done") == 0 && dc && dc->add_ctx) {
                try {
                    dc->add_ctx->client->complete_oauth(dc->provider_id);
                    dc->add_ctx->view->refresh(*dc->add_ctx->client);
                } catch (const std::exception& e) {
                    show_error(GTK_WINDOW(d), e.what());
                }
            }
            if (dc) {
                delete dc->add_ctx;
                delete dc;
            }
            gtk_window_destroy(GTK_WINDOW(d));
        }), dctx);

        gtk_window_present(GTK_WINDOW(dlg));
    }

    // -----------------------------------------------------------------------
    // Local model dialog
    // -----------------------------------------------------------------------

    struct LocalDialogWidgets {
        GtkWidget* name_entry;
        GtkWidget* path_entry;
        AddContext* ctx;
    };

    static void show_local_dialog(AddContext* ctx) {
        GtkWidget* dlg = adw_message_dialog_new(
            ctx->parent,
            _("Add Local Model"),
            _("Provide the path to a GGUF model file."));
        adw_message_dialog_add_response(ADW_MESSAGE_DIALOG(dlg), "cancel",
                                        _("Cancel"));
        adw_message_dialog_add_response(ADW_MESSAGE_DIALOG(dlg), "add",
                                        _("Add"));
        adw_message_dialog_set_response_appearance(
            ADW_MESSAGE_DIALOG(dlg), "add", ADW_RESPONSE_SUGGESTED);
        adw_message_dialog_set_default_response(ADW_MESSAGE_DIALOG(dlg), "add");
        adw_message_dialog_set_close_response(ADW_MESSAGE_DIALOG(dlg), "cancel");

        GtkWidget* box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 8);
        gtk_widget_set_margin_start(box, 12);
        gtk_widget_set_margin_end(box, 12);

        GtkWidget* name_entry = gtk_entry_new();
        gtk_entry_set_placeholder_text(GTK_ENTRY(name_entry), _("Display name"));
        gtk_box_append(GTK_BOX(box), name_entry);

        GtkWidget* path_entry = gtk_entry_new();
        gtk_entry_set_placeholder_text(GTK_ENTRY(path_entry),
                                       _("Model path (e.g. /models/qwen.gguf)"));
        gtk_box_append(GTK_BOX(box), path_entry);

        adw_message_dialog_set_extra_child(ADW_MESSAGE_DIALOG(dlg), box);

        auto* widgets = new LocalDialogWidgets{name_entry, path_entry, ctx};
        g_signal_connect(dlg, "response",
                         G_CALLBACK(on_local_response), widgets);
        gtk_window_present(GTK_WINDOW(dlg));
    }

    static void on_local_response(AdwMessageDialog* dlg,
                                  const char* response,
                                  gpointer user_data) {
        auto* w = static_cast<LocalDialogWidgets*>(user_data);
        if (g_strcmp0(response, "add") == 0 && w && w->ctx) {
            std::string name = gtk_editable_get_text(GTK_EDITABLE(w->name_entry));
            std::string path = gtk_editable_get_text(GTK_EDITABLE(w->path_entry));
            try {
                w->ctx->client->add_local_provider(name, path);
                w->ctx->view->refresh(*w->ctx->client);
            } catch (const std::exception& e) {
                show_error(GTK_WINDOW(dlg), e.what());
            }
        }
        if (w) {
            delete w->ctx;
            delete w;
        }
        gtk_window_destroy(GTK_WINDOW(dlg));
    }

    // -----------------------------------------------------------------------
    // Utility
    // -----------------------------------------------------------------------

    /// Convert integer provider type to a display label.
    static std::string type_label(int type) {
        switch (type) {
            case 1: return "API Key";
            case 2: return "OAuth";
            case 3: return "Local";
            default: return "Unknown";
        }
    }

    static void show_error(GtkWindow* parent, const char* message) {
        GtkWidget* err_dlg = adw_message_dialog_new(
            parent, _("Error"), message);
        adw_message_dialog_add_response(ADW_MESSAGE_DIALOG(err_dlg), "ok",
                                        _("OK"));
        adw_message_dialog_set_default_response(ADW_MESSAGE_DIALOG(err_dlg),
                                                "ok");
        adw_message_dialog_set_close_response(ADW_MESSAGE_DIALOG(err_dlg),
                                              "ok");
        gtk_window_present(GTK_WINDOW(err_dlg));
    }
};

#undef _
