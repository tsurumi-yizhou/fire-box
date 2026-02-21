#include <adwaita.h>
#include <libintl.h>
#include <clocale>
#include <string>

namespace {

constexpr int kExitApproved = 0;
constexpr int kExitDenied = 1;
constexpr int kExitError = 2;
#define _(Text) gettext(Text)

struct Strings {
    std::string title;
    std::string instruction;
    std::string content;
    std::string ok;
    std::string cancel;
};

struct DialogContext {
    Strings strings;
    int exit_code;
    GApplication* app;
};

auto parse_requester_name(int argc, char** argv) -> std::string {
    if (argc > 1) {
        std::string first = argv[1];
        if (!first.empty()) {
            return first;
        }
    }

    return _("An application");
}

auto localized_strings(const std::string& requester_name) -> Strings {
    const char* instruction_format = _("%s wants to use AI capabilities. Approve?");
    gchar* instruction = g_strdup_printf(instruction_format, requester_name.c_str());
    std::string instruction_text = instruction != nullptr ? instruction : requester_name + " wants to use AI capabilities. Approve?";
    if (instruction != nullptr) {
        g_free(instruction);
    }

    return {
        _("AI Capability Request"),
        instruction_text,
        _("This request is sent by the local AI capability management service."),
        _("Allow"),
        _("Cancel"),
    };
}

void on_allow_clicked(GtkButton*, gpointer user_data) {
    auto* context = static_cast<DialogContext*>(user_data);
    context->exit_code = kExitApproved;
    g_application_quit(context->app);
}

void on_cancel_clicked(GtkButton*, gpointer user_data) {
    auto* context = static_cast<DialogContext*>(user_data);
    context->exit_code = kExitDenied;
    g_application_quit(context->app);
}

gboolean on_close_request(GtkWindow*, gpointer user_data) {
    auto* context = static_cast<DialogContext*>(user_data);
    context->exit_code = kExitDenied;
    g_application_quit(context->app);
    return FALSE;
}

void on_activate(GApplication* app, gpointer user_data) {
    auto* context = static_cast<DialogContext*>(user_data);
    const Strings& s = context->strings;

    GtkWidget* window = adw_application_window_new(GTK_APPLICATION(app));
    gtk_window_set_title(GTK_WINDOW(window), s.title.c_str());
    gtk_window_set_modal(GTK_WINDOW(window), TRUE);
    gtk_window_set_resizable(GTK_WINDOW(window), FALSE);
    gtk_window_set_default_size(GTK_WINDOW(window), 420, 180);

    GtkWidget* root = gtk_box_new(GTK_ORIENTATION_VERTICAL, 16);
    gtk_widget_set_margin_start(root, 16);
    gtk_widget_set_margin_end(root, 16);
    gtk_widget_set_margin_top(root, 16);
    gtk_widget_set_margin_bottom(root, 16);
    adw_application_window_set_content(ADW_APPLICATION_WINDOW(window), root);

    GtkWidget* instruction = gtk_label_new(s.instruction.c_str());
    gtk_label_set_xalign(GTK_LABEL(instruction), 0.0f);
    gtk_label_set_wrap(GTK_LABEL(instruction), TRUE);
    gtk_widget_add_css_class(instruction, "title-2");
    gtk_box_append(GTK_BOX(root), instruction);

    GtkWidget* content = gtk_label_new(s.content.c_str());
    gtk_label_set_xalign(GTK_LABEL(content), 0.0f);
    gtk_label_set_wrap(GTK_LABEL(content), TRUE);
    gtk_box_append(GTK_BOX(root), content);

    GtkWidget* buttons = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 8);
    gtk_widget_set_halign(buttons, GTK_ALIGN_END);
    gtk_widget_set_margin_top(buttons, 8);
    gtk_box_append(GTK_BOX(root), buttons);

    GtkWidget* cancel_button = gtk_button_new_with_label(s.cancel.c_str());
    GtkWidget* ok_button = gtk_button_new_with_label(s.ok.c_str());
    gtk_widget_add_css_class(ok_button, "suggested-action");
    gtk_box_append(GTK_BOX(buttons), cancel_button);
    gtk_box_append(GTK_BOX(buttons), ok_button);

    g_signal_connect(ok_button, "clicked", G_CALLBACK(on_allow_clicked), context);
    g_signal_connect(cancel_button, "clicked", G_CALLBACK(on_cancel_clicked), context);
    g_signal_connect(window, "close-request", G_CALLBACK(on_close_request), context);

    gtk_window_present(GTK_WINDOW(window));
}

}  // namespace

auto main(int argc, char** argv) -> int {
    std::setlocale(LC_ALL, "");
    bindtextdomain("fire-box-helper", LOCALEDIR);
    bind_textdomain_codeset("fire-box-helper", "UTF-8");
    textdomain("fire-box-helper");

    std::string requester_name = parse_requester_name(argc, argv);
    DialogContext context{localized_strings(requester_name), kExitDenied, nullptr};
    AdwApplication* app = adw_application_new("com.firebox.helper", G_APPLICATION_NON_UNIQUE);
    if (app == nullptr) {
        return kExitError;
    }

    context.app = G_APPLICATION(app);
    g_signal_connect(app, "activate", G_CALLBACK(on_activate), &context);
    g_application_run(G_APPLICATION(app), argc, argv);
    const int exit_code = context.exit_code;
    g_object_unref(app);
    return exit_code;
}
