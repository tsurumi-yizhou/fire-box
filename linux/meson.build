project('fire-box', 'c')

executable('daemon', 'src/daemon.c',
  include_directories: include_directories('../generated'),
  dependencies: [
    declare_dependency(
      link_args: [meson.project_source_root() / '..' / 'generated' / 'libcore.a'],
      dependencies: [ dependency('threads') ],
    ),
  ],
  install: true,
)

executable('main', 'src/main.c',
  include_directories: include_directories('include'),
  dependencies: [
    dependency('gtk4', version: '>=4.16', required: false),
    dependency('libadwaita-1', version: '>=1.6', required: false),
    dependency('gio-2.0', version: '>=2.82', required: false),
    dependency('ayatana-appindicator3-0.1', version: '>=0.6', required: false),
  ],
  install: true,
)
