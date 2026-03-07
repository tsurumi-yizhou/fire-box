# ── tests ────────────────────────────────────────────────────────

# boost-ut is header-only; find it or use a subproject
boost_ut_dep = dependency('boost.ut',
  required : false,
  modules : [],
)

if not boost_ut_dep.found()
  # Try finding as a header-only library
  boost_ut_dep = cpp.find_library('ut', required : false)
endif

if not boost_ut_dep.found()
  # Create a minimal dependency — user needs boost-ut headers in include path
  boost_ut_dep = declare_dependency()
  message('boost-ut not found via pkg-config; assumed header-only in include path')
endif

test_deps = [
  common_dep,
  boost_ut_dep,
  sdbus_dep,
  spdlog_dep,
  firebox_client_dep,
]

# ── Individual test executables ──────────────────────────────────

test_storage = executable('test-storage',
  'test_storage.cpp',
  include_directories : [common_inc],
  dependencies        : test_deps,
)
test('storage', test_storage)

test_coroutine = executable('test-coroutine',
  'test_coroutine.cpp',
  include_directories : [common_inc],
  dependencies        : test_deps,
)
test('coroutine', test_coroutine)

test_router = executable('test-router',
  'test_router.cpp',
  include_directories : [common_inc],
  dependencies        : test_deps,
)
test('router', test_router)

test_client = executable('test-client',
  'test_client.cpp',
  include_directories : [common_inc],
  dependencies        : test_deps,
)
test('client', test_client)

test_dbus_ping = executable('test-dbus-ping',
  'test_dbus_ping.cpp',
  include_directories : [common_inc],
  dependencies        : test_deps,
)
test('dbus-ping', test_dbus_ping)
