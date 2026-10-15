import QtQuick
import QtQuick.Layouts

// A short message at the bottom, optionally with one action (like Undo).
Rectangle {
    id: toast

    property var action: null

    function show(message, actionText, actionFn) {
        label.text = message
        actionButton.text = actionText || ""
        action = actionFn || null
        opacity = 1
        timer.restart()
    }

    anchors.horizontalCenter: parent.horizontalCenter
    anchors.bottom: parent.bottom
    anchors.bottomMargin: 28
    width: Math.min(row.implicitWidth + 32, parent.width - 64)
    height: 48
    radius: Theme.radiusM
    color: Theme.popup
    border.color: Theme.hairline
    opacity: 0
    visible: opacity > 0
    z: 100
    Behavior on opacity { NumberAnimation { duration: 160 } }

    Timer { id: timer; interval: 5000; onTriggered: toast.opacity = 0 }

    RowLayout {
        id: row
        anchors.centerIn: parent
        spacing: 16
        Text {
            id: label
            color: Theme.text
            font.family: Theme.font
            font.pixelSize: 14
            elide: Text.ElideRight
            Layout.maximumWidth: toast.parent ? toast.parent.width - 220 : 600
        }
        SfButton {
            id: actionButton
            visible: text !== ""
            size: 32
            tone: Theme.hover
            onClicked: {
                if (toast.action) toast.action()
                toast.opacity = 0
            }
        }
    }
}
