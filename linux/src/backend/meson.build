# ── backend service ──────────────────────────────────────────────

backend_sources = files(
  'main.cpp',
  'service.cpp',
  'capability_adaptor.cpp',
  'control_adaptor.cpp',
  'tofu.cpp',
  'http_client.cpp',
)

backend_deps = [
  common_dep,
  sdbus_dep,
  soup_dep,
  spdlog_dep,
  gio_dep,
  glib_dep,
]

if polkit_dep.found()
  backend_deps += polkit_dep
endif

# Generated D-Bus adaptor headers
backend_gen = [capability_adaptor, control_adaptor]

firebox_backend = executable('firebox-backend',
  backend_sources,
  backend_gen,
  include_directories : [common_inc],
  dependencies        : backend_deps,
  install             : true,
)
