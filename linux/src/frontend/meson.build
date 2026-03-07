# ── frontend GUI application ─────────────────────────────────────

frontend_sources = files(
  'main.cpp',
  'application.cpp',
  'window.cpp',
  'dashboard_page.cpp',
  'settings_page.cpp',
  'route_page.cpp',
  'allowlist_page.cpp',
  'connections_page.cpp',
  'chart_widget.cpp',
)

frontend_deps = [
  common_dep,
  gtkmm_dep,
  adw_dep,
  sdbus_dep,
  spdlog_dep,
  sigc_dep,
]

# Generated D-Bus proxy headers
frontend_gen = [control_proxy]

firebox_frontend = executable('firebox',
  frontend_sources,
  frontend_gen,
  include_directories : [common_inc],
  dependencies        : frontend_deps,
  install             : true,
)
