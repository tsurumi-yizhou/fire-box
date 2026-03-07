project('firebox', 'cpp',
  version : '0.1.0',
  default_options : [
    'cpp_std=c++23',
    'warning_level=2',
    'werror=false',
  ],
  meson_version : '>= 1.1.0',
)

# ── compilers & flags ─────────────────────────────────────────────
cpp = meson.get_compiler('cpp')

# ── dependencies ──────────────────────────────────────────────────
glib_dep       = dependency('glib-2.0',       version : '>= 2.76')
gio_dep        = dependency('gio-2.0',        version : '>= 2.76')
giomm_dep      = dependency('giomm-2.68',     version : '>= 2.68', required : false)
gtkmm_dep      = dependency('gtkmm-4.0',      version : '>= 4.10')
adw_dep        = dependency('libadwaita-1',    version : '>= 1.3')
soup_dep       = dependency('libsoup-3.0',     version : '>= 3.0')
sqlite_dep     = dependency('sqlite3',         version : '>= 3.36')
sdbus_dep      = dependency('sdbus-c++',       version : '>= 1.3')
spdlog_dep     = dependency('spdlog',          version : '>= 1.11')
sigc_dep       = dependency('sigc++-3.0')
json_dep       = dependency('nlohmann_json',   version : '>= 3.11', required : false)

# If nlohmann_json not found as pkg-config, try cmake
if not json_dep.found()
  json_dep = dependency('nlohmann_json', method : 'cmake', required : false)
endif

# If still not found, use a header-only subproject fallback
if not json_dep.found()
  json_dep = declare_dependency()
  message('nlohmann_json not found; assumed header-only in include path')
endif

# systemd for sd-journal
libsystemd_dep = dependency('libsystemd', required : false)

# polkit for TOFU authorization
polkit_dep = dependency('polkit-gobject-1', required : false)

# ── sdbus-c++ xml2cpp code generator ─────────────────────────────
sdbus_xml2cpp = find_program('sdbus-c++-xml2cpp', required : true)

# Helper to generate adaptor/proxy headers from D-Bus XML interfaces
dbus_xml_dir = meson.project_source_root() / 'data' / 'dbus'

capability_adaptor = custom_target('capability-adaptor',
  input  : dbus_xml_dir / 'org.firebox.Capability.xml',
  output : 'capability_adaptor.h',
  command: [sdbus_xml2cpp, '@INPUT@',
            '--adaptor=@OUTPUT@'],
)

capability_proxy = custom_target('capability-proxy',
  input  : dbus_xml_dir / 'org.firebox.Capability.xml',
  output : 'capability_proxy.h',
  command: [sdbus_xml2cpp, '@INPUT@',
            '--proxy=@OUTPUT@'],
)

control_adaptor = custom_target('control-adaptor',
  input  : dbus_xml_dir / 'org.firebox.Control.xml',
  output : 'control_adaptor.h',
  command: [sdbus_xml2cpp, '@INPUT@',
            '--adaptor=@OUTPUT@'],
)

control_proxy = custom_target('control-proxy',
  input  : dbus_xml_dir / 'org.firebox.Control.xml',
  output : 'control_proxy.h',
  command: [sdbus_xml2cpp, '@INPUT@',
            '--proxy=@OUTPUT@'],
)

# ── sub-directories ──────────────────────────────────────────────
subdir('src')
subdir('test')

# ── data files install ───────────────────────────────────────────
install_data(
  'data' / 'systemd' / 'firebox.service',
  install_dir : get_option('prefix') / 'lib' / 'systemd' / 'user',
)

install_data(
  'data' / 'polkit' / 'org.firebox.policy',
  install_dir : get_option('prefix') / 'share' / 'polkit-1' / 'actions',
)

install_data(
  'data' / 'dbus' / 'org.firebox.Capability.xml',
  'data' / 'dbus' / 'org.firebox.Control.xml',
  install_dir : get_option('prefix') / 'share' / 'dbus-1' / 'interfaces',
)

# GSettings schema
install_data(
  'data' / 'gsettings' / 'org.firebox.gschema.xml',
  install_dir : get_option('prefix') / 'share' / 'glib-2.0' / 'schemas',
)
