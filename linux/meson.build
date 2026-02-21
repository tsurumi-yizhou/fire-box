project('fire-box', 'cpp',
  version: '1.0.0',
  default_options: ['cpp_std=c++20']
)

# Import modules
i18n = import('i18n')
gnome = import('gnome')

# Directories
locale_dir = join_paths(get_option('prefix'), get_option('localedir'))

# Find blueprint compiler
blueprint_compiler = find_program('blueprint-compiler', required: true)

# Dependencies
gtkmm_dep = dependency('gtkmm-4.0')
adwaita_dep = dependency('libadwaita-1')
sdbus_dep = dependency('sdbus-c++')
appindicator_dep = dependency('ayatana-appindicator3-0.1')

# Compile Blueprint UI files
blueprints = files('ui/window.blp')

compiled_blueprints = []
foreach blueprint : blueprints
  compiled_blueprints += custom_target(
    '@BASENAME@.ui',
    input: blueprint,
    output: '@BASENAME@.ui',
    command: [blueprint_compiler, 'compile', '--output', '@OUTPUT@', '@INPUT@'],
    install: false
  )
endforeach

# Compile GResources
resources = gnome.compile_resources('fire-box-resources',
  'resources/fire-box.gresource.xml',
  source_dir: ['resources', meson.current_build_dir()],
  dependencies: compiled_blueprints,
  c_name: 'fire_box'
)

executable('App', ['src/app.cpp', resources],
  install: true,
  install_dir: get_option('bindir'),
  dependencies: [
    gtkmm_dep,
    adwaita_dep,
    sdbus_dep,
    appindicator_dep
  ],
  cpp_args: ['-DLOCALEDIR="@0@"'.format(locale_dir), '-fcoroutines'],
)

executable('Helper', 'src/helper.cpp',
  install: true,
  install_dir: get_option('bindir'),
  dependencies: [
    gtkmm_dep,
    adwaita_dep
  ],
  cpp_args: ['-DLOCALEDIR="@0@"'.format(locale_dir), '-fcoroutines'],
)

# i18n configuration
i18n.gettext('fire-box', preset: 'glib')
