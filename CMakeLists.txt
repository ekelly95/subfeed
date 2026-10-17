cmake_minimum_required(VERSION 3.24)
project(subfeed VERSION 0.1.2 LANGUAGES CXX)

set(CMAKE_AUTOMOC ON)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(Qt6 6.8 REQUIRED COMPONENTS Core Gui Qml Quick QuickControls2 DBus)
# QML modules live under :/qt/qml (on the default import path), so the app runs from anywhere,
# not just next to build/dev/subfeed/qmldir.
qt_policy(SET QTP0001 NEW)
# VA-API, to find out which codecs the GPU decodes
find_package(PkgConfig REQUIRED)
pkg_check_modules(LIBVA REQUIRED IMPORTED_TARGET libva libva-drm)
# KDE's libmpv QML item (the same one Haruna uses), from the `mpvqt` package
find_package(MpvQt REQUIRED)

# Rust <-> Qt bridge. Pinned to the tag matching the cxx-qt 0.10 crates.
find_package(CxxQt QUIET)
if(NOT CxxQt_FOUND)
    include(FetchContent)
    FetchContent_Declare(
        CxxQt
        GIT_REPOSITORY https://github.com/kdab/cxx-qt-cmake.git
        GIT_TAG 0.10.0
    )
    FetchContent_MakeAvailable(CxxQt)
endif()

# Rust core: data, feeds, settings. Exposed to QML as module `dev.subfeed.core`
cxx_qt_import_crate(
    MANIFEST_PATH rust/Cargo.toml
    CRATES subfeed_core
    LOCKED
    QT_MODULES Qt::Core Qt::Gui Qt::Qml
)
cxx_qt_import_qml_module(subfeed_core_qml
    URI "dev.subfeed.core"
    SOURCE_CRATE subfeed_core)

qt_add_executable(subfeed cpp/main.cpp cpp/appbus.h cpp/appbus.cpp)

# Theme.qml is the design tokens singleton (direction A).
set_source_files_properties(qml/Theme.qml PROPERTIES QT_QML_SINGLETON_TYPE TRUE)

# C++ QML module: the player item (MpvQt subclass) and the UI
qt_add_qml_module(subfeed
    URI dev.subfeed
    VERSION 1.0
    QML_FILES
        qml/Main.qml qml/Theme.qml
        qml/FeedPage.qml qml/VideoCard.qml qml/HeroSection.qml qml/ChannelChips.qml
        qml/PlayerPage.qml qml/ScrubBar.qml
        qml/SfButton.qml qml/Kbd.qml qml/Toast.qml qml/WelcomePage.qml qml/AddChannelBox.qml qml/ChannelsPage.qml
    RESOURCES
        fonts/Geist-Regular.ttf fonts/Geist-Medium.ttf fonts/Geist-SemiBold.ttf fonts/Geist-Bold.ttf
        fonts/GeistMono-Medium.ttf
        icons/search.svg icons/refresh-cw.svg icons/play.svg icons/pause.svg icons/rotate-ccw.svg
        icons/rotate-cw.svg icons/volume-2.svg icons/volume-x.svg icons/maximize.svg icons/minimize.svg
        icons/panel-right-close.svg icons/panel-right-open.svg icons/check.svg icons/copy.svg
        icons/chevron-left.svg icons/triangle-alert.svg icons/battery-medium.svg icons/eye-off.svg
        icons/funnel.svg icons/loader-circle.svg icons/smartphone.svg icons/users.svg icons/captions.svg icons/file-text.svg icons/io.github.ekelly95.Subfeed.svg
    SOURCES cpp/mpvitem.h cpp/mpvitem.cpp cpp/systeminfo.h cpp/systeminfo.cpp
            cpp/mpris.h cpp/mpris.cpp cpp/inhibitor.h cpp/inhibitor.cpp
)

target_compile_definitions(subfeed PRIVATE SUBFEED_VERSION="${PROJECT_VERSION}")
# qmltyperegistrations includes "mpvitem.h" by bare name
target_include_directories(subfeed PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/cpp)
target_link_libraries(subfeed PRIVATE
    Qt::Core Qt::Gui Qt::Qml Qt::Quick Qt::QuickControls2 Qt::DBus
    MpvQt::MpvQt
    PkgConfig::LIBVA
    subfeed_core_qml
)

# The app ID (io.github.ekelly95.Subfeed) names the .desktop file, the icon, the D-Bus service and the
# Wayland app_id, so desktops and portals can match the window to its launcher.
include(GNUInstallDirs)
install(TARGETS subfeed RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR})
install(FILES io.github.ekelly95.Subfeed.desktop DESTINATION ${CMAKE_INSTALL_DATADIR}/applications)
install(FILES io.github.ekelly95.Subfeed.metainfo.xml DESTINATION ${CMAKE_INSTALL_DATADIR}/metainfo)
install(FILES icons/io.github.ekelly95.Subfeed.svg DESTINATION ${CMAKE_INSTALL_DATADIR}/icons/hicolor/scalable/apps)
