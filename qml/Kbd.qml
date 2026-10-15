import QtQuick

// A key name, like "W" or "Esc". `plain` drops the outline (inside buttons).
Text {
    property bool plain: false

    font.family: Theme.mono
    font.pixelSize: 11
    color: Theme.text3
    leftPadding: plain ? 0 : 6
    rightPadding: plain ? 0 : 6
    topPadding: plain ? 0 : 1
    bottomPadding: plain ? 0 : 1

    Rectangle {
        visible: !parent.plain
        anchors.fill: parent
        radius: 6
        color: "transparent"
        border.color: Theme.selected
    }
}
