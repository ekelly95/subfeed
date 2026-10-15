pragma Singleton
import QtQuick

// Direction A ("Cinema"): subfeed's own look. It follows the desktop's light or dark setting
// (only the accent comes from the desktop's palette); the player is always dark, like a cinema.
QtObject {
    id: theme

    // Set while the player is showing (Main.qml).
    property bool forceDark: false
    // "light" or "dark" instead of the desktop's setting (SUBFEED_COLOR_SCHEME, for screenshots).
    property string override: ""
    readonly property bool light: !forceDark
        && (override !== "" ? override === "light" : Qt.styleHints.colorScheme === Qt.ColorScheme.Light)

    readonly property color bg: light ? "#f4f5f7" : "#0e0f12"
    readonly property color panel: light ? "#eceef1" : "#121317"
    readonly property color surface: light ? "#ffffff" : "#17191d"
    readonly property color raised: light ? "#e8eaee" : "#1b1d22"
    readonly property color hover: light ? "#dfe2e7" : "#24272d"
    readonly property color selected: light ? "#d6dae0" : "#2a2d33"
    readonly property color hairline: light ? Qt.rgba(0, 0, 0, 0.09) : Qt.rgba(1, 1, 1, 0.06)
    readonly property color text: light ? "#14161a" : "#eceef1"
    readonly property color textSoft: light ? "#3a3f46" : "#c9cdd3"
    readonly property color text2: light ? "#5a616b" : "#9aa1ab"
    readonly property color text3: light ? "#7d848e" : "#6e7580"
    readonly property color placeholder: light ? "#848b95" : "#8a919b"
    readonly property color popup: light ? "#ffffff" : "#23262c"
    readonly property color mid: light ? "#c3c8cf" : "#3a3e45"
    readonly property color warning: light ? "#a85f00" : "#f5a524"
    // The desktop's accent, and the text that sits on it.
    readonly property color accent: palette.highlight
    readonly property color accentInk: light ? "#ffffff" : "#0b0c0e"
    readonly property color accentInkSoft: light ? Qt.rgba(1, 1, 1, 0.7) : Qt.rgba(0, 0, 0, 0.55)

    readonly property string font: geist.name !== "" ? geist.name : "sans-serif"
    readonly property string mono: geistMono.name !== "" ? geistMono.name : "monospace"

    readonly property int radiusS: 8
    readonly property int radiusM: 12
    readonly property int radiusL: 16
    readonly property int gutter: 40

    property SystemPalette palette: SystemPalette { colorGroup: SystemPalette.Active }

    property FontLoader geist: FontLoader { source: Qt.resolvedUrl("../fonts/Geist-Regular.ttf") }
    property FontLoader geistMedium: FontLoader { source: Qt.resolvedUrl("../fonts/Geist-Medium.ttf") }
    property FontLoader geistSemiBold: FontLoader { source: Qt.resolvedUrl("../fonts/Geist-SemiBold.ttf") }
    property FontLoader geistBold: FontLoader { source: Qt.resolvedUrl("../fonts/Geist-Bold.ttf") }
    property FontLoader geistMono: FontLoader { source: Qt.resolvedUrl("../fonts/GeistMono-Medium.ttf") }

    // Bundled Lucide icons (icons/), tinted by Kirigami.Icon's isMask.
    function icon(name) { return name === "" ? "" : Qt.resolvedUrl("../icons/" + name + ".svg") }

    // "Fireship · 5:06", leaving out empty parts (a video's length isn't known until it's played).
    function meta(parts) { return parts.filter((p) => p !== undefined && p !== "").join(" · ") }

    function timecode(seconds) {
        if (!isFinite(seconds) || seconds < 0) seconds = 0
        const s = Math.floor(seconds % 60), m = Math.floor(seconds / 60) % 60, h = Math.floor(seconds / 3600)
        const ss = (s < 10 ? "0" : "") + s
        return h > 0 ? h + ":" + (m < 10 ? "0" : "") + m + ":" + ss : m + ":" + ss
    }
}
