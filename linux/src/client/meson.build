# ── client SDK library ───────────────────────────────────────────

client_sources = files(
  'firebox_client.cpp',
  'chat_stream.cpp',
)

client_deps = [
  common_dep,
  sdbus_dep,
  spdlog_dep,
]

# Generated D-Bus proxy headers
client_gen = [capability_proxy]

firebox_client_lib = static_library('firebox-client',
  client_sources,
  client_gen,
  include_directories : [common_inc],
  dependencies        : client_deps,
)

firebox_client_dep = declare_dependency(
  link_with           : firebox_client_lib,
  include_directories : [common_inc],
  dependencies        : client_deps,
)
