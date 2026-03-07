# ── common library (header-only + compiled helpers) ─────────────
# Use parent dir (src/) so includes like "common/storage.hpp" resolve
common_inc = include_directories('..')

common_sources = files(
  'storage.cpp',
  'credential.cpp',
)

common_deps = [
  glib_dep,
  gio_dep,
  sqlite_dep,
  spdlog_dep,
]

if libsystemd_dep.found()
  common_deps += libsystemd_dep
endif

common_lib = static_library('firebox-common',
  common_sources,
  include_directories : common_inc,
  dependencies        : common_deps,
)

common_dep = declare_dependency(
  link_with           : common_lib,
  include_directories : common_inc,
  dependencies        : common_deps,
)
