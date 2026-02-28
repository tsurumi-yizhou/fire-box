cmake_minimum_required(VERSION 4.00)
project(FireBox VERSION 1.1.1)

set(CPACK_PACKAGE_NAME "FireBox")
set(CPACK_PACKAGE_VENDOR "FireBox")
set(CPACK_PACKAGE_DESCRIPTION_SUMMARY "AI Gateway Service")
set(CPACK_PACKAGE_VERSION ${PROJECT_VERSION})
set(CPACK_PACKAGE_INSTALL_DIRECTORY "FireBox")
set(CPACK_PACKAGE_FILE_NAME "FireBox-${PROJECT_VERSION}")

if(WIN32)
    set(CPACK_GENERATOR "WIX")
    set(CPACK_WIX_VERSION 4)
    set(CPACK_WIX_PATCH_FILE "${CMAKE_SOURCE_DIR}/windows/wix-patch.xml")

    add_custom_target(firebox-service ALL
        COMMAND cargo build --release
        WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}/service
        COMMENT "Building Rust service..."
    )

    set(WIN_APP_OUT "${CMAKE_BINARY_DIR}/windows_app")
    set(WIN_HELPER_OUT "${CMAKE_BINARY_DIR}/windows_helper")

    add_custom_target(windows_app ALL
        COMMAND dotnet publish -c Release --self-contained -o ${WIN_APP_OUT}
        WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}/windows/App
        DEPENDS firebox-service
        COMMENT "Building Windows App..."
    )

    add_custom_target(windows_helper ALL
        COMMAND dotnet publish -c Release --self-contained -o ${WIN_HELPER_OUT}
        WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}/windows/Helper
        DEPENDS firebox-service
        COMMENT "Building Windows Helper..."
    )

    install(PROGRAMS ${CMAKE_SOURCE_DIR}/target/release/firebox-service.exe DESTINATION bin)
    install(DIRECTORY ${WIN_APP_OUT}/
            DESTINATION FireBox
            USE_SOURCE_PERMISSIONS
            PATTERN "*.pdb" EXCLUDE)
    install(PROGRAMS ${WIN_HELPER_OUT}/Helper.exe DESTINATION bin)

elseif(APPLE)
    set(CPACK_GENERATOR "productbuild")
    set(CPACK_PACKAGING_INSTALL_PREFIX "/")

    add_custom_target(firebox-service ALL
        COMMAND cargo build --release
        WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}/service
        COMMENT "Building Rust service..."
    )

    set(MACOS_BIN_PATH "${CMAKE_SOURCE_DIR}/macos/.build/apple/Products/Release")

    add_custom_target(macos_app ALL
        COMMAND swift build -c release
        WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}/macos
        DEPENDS firebox-service
        COMMENT "Building macOS application..."
    )

    install(PROGRAMS ${CMAKE_SOURCE_DIR}/target/release/firebox-service DESTINATION usr/local/bin)
    install(DIRECTORY ${MACOS_BIN_PATH}/
            DESTINATION Applications/FireBox.app/Contents/MacOS
            USE_SOURCE_PERMISSIONS
            FILES_MATCHING PATTERN "App"
            PATTERN "*.dSYM" EXCLUDE)
    install(PROGRAMS ${MACOS_BIN_PATH}/Helper
            DESTINATION Applications/FireBox.app/Contents/Helpers)
    install(FILES ${CMAKE_SOURCE_DIR}/macos/Info.plist
            DESTINATION Applications/FireBox.app/Contents)
elseif(UNIX)
    set(CPACK_GENERATOR "DEB")
    set(CPACK_DEBIAN_PACKAGE_MAINTAINER "FireBox Team")
    set(CPACK_DEBIAN_PACKAGE_SHLIBDEPS ON)

    add_custom_target(firebox-service ALL
        COMMAND cargo build --release
        WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}/service
        COMMENT "Building Rust service..."
    )

    add_custom_target(linux_gui ALL
        COMMAND meson setup ${CMAKE_BINARY_DIR}/linux-build ${CMAKE_SOURCE_DIR}/linux --prefix=/usr
        COMMAND meson compile -C ${CMAKE_BINARY_DIR}/linux-build
        WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}
        DEPENDS firebox-service
        COMMENT "Building Linux GUI..."
    )

    install(PROGRAMS ${CMAKE_SOURCE_DIR}/target/release/firebox-service DESTINATION bin)
    install(PROGRAMS ${CMAKE_BINARY_DIR}/linux-build/App DESTINATION bin RENAME firebox-app)
    install(PROGRAMS ${CMAKE_BINARY_DIR}/linux-build/Helper DESTINATION bin RENAME firebox-helper)
    install(FILES ${CMAKE_SOURCE_DIR}/linux/firebox.desktop DESTINATION share/applications)
    install(FILES ${CMAKE_SOURCE_DIR}/linux/firebox.service DESTINATION lib/systemd/user)
endif()

include(CPack)
