project('fire-box', 'cpp',
  version: '1.0.0',
  default_options: ['cpp_std=c++20']
)

# Import modules
i18n = import('i18n')

# Directories
locale_dir = join_paths(get_option('prefix'), get_option('localedir'))

# Dependencies
adwaita_dep = dependency('libadwaita-1')
sdbus_dep = dependency('sdbus-c++')
json_glib_dep = dependency('json-glib-1.0')

executable('App', 'src/app.cpp',
  install: true,
  install_dir: get_option('bindir'),
  dependencies: [
    adwaita_dep,
    sdbus_dep,
    json_glib_dep,
  ],
  cpp_args: ['-DLOCALEDIR="@0@"'.format(locale_dir)],
)

executable('Helper', 'src/helper.cpp',
  install: true,
  install_dir: get_option('bindir'),
  dependencies: [
    adwaita_dep,
  ],
  cpp_args: ['-DLOCALEDIR="@0@"'.format(locale_dir)],
)

# i18n configuration
subdir('po')
