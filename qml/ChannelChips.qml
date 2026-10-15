import QtQuick
import QtQuick.Templates as T
import org.kde.kirigami as Kirigami

// "Shorts" (a toggle for the Shorts view) and "All" stay put at the left; the channel pills
// scroll sideways beside them. Counts are unwatched videos (Shorts, in the Shorts view).
Item {
    id: chips

    required property var channels      // ChannelModel
    property string selected: ""
    property bool shortsMode: false
    property real leftMargin: 0
    property real rightMargin: 0

    signal picked(string channelId, string title)
    signal shortsToggled()

    Row {
        id: fixed
        x: chips.leftMargin
        height: parent.height
        spacing: 8
        Chip {
            label: "Shorts"
            iconName: "smartphone"
            outlined: true
            count: chips.channels.totalUnwatchedShorts
            active: chips.shortsMode
            onClicked: chips.shortsToggled()
        }
        Rectangle {
            width: 1
            height: 20
            anchors.verticalCenter: parent.verticalCenter
            color: Theme.hairline
        }
        Chip {
            label: "All"
            count: chips.shortsMode ? chips.channels.totalUnwatchedShorts : chips.channels.totalUnwatched
            active: chips.selected === ""
            onClicked: chips.picked("", "")
        }
    }

    ListView {
        id: list
        anchors { left: fixed.right; leftMargin: 8; right: parent.right; top: parent.top; bottom: parent.bottom }
        rightMargin: chips.rightMargin
        orientation: ListView.Horizontal
        spacing: 8
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        model: chips.channels

        delegate: Chip {
            required property string channelId
            required property string title
            required property int unwatched
            required property int unwatchedShorts
            label: title
            count: chips.shortsMode ? unwatchedShorts : unwatched
            active: chips.selected === channelId
            onClicked: chips.picked(channelId, title)
        }

        // Soft edges where there's more to scroll.
        Rectangle {
            anchors { left: parent.left; top: parent.top; bottom: parent.bottom }
            width: 40
            visible: list.contentX > list.originX + 1
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop { position: 0; color: Theme.bg }
                GradientStop { position: 1; color: "transparent" }
            }
        }
        Rectangle {
            anchors { right: parent.right; top: parent.top; bottom: parent.bottom }
            width: 72
            visible: list.contentX + list.width < list.originX + list.contentWidth + list.rightMargin - 1
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop { position: 0; color: "transparent" }
                GradientStop { position: 1; color: Theme.bg }
            }
        }

        // Wheel scrolls sideways.
        WheelHandler {
            onWheel: (event) => {
                list.contentX = Math.max(0, Math.min(list.contentWidth - list.width,
                                                      list.contentX - event.angleDelta.y - event.angleDelta.x))
            }
        }
    }

    component Chip: T.Button {
        id: chip
        property string label
        property string iconName
        property int count
        property bool active
        // A mode rather than a filter: shown "on" with an accent outline, not a fill.
        property bool outlined: false
        readonly property bool filled: active && !outlined
        readonly property color ink: filled ? Theme.accentInk : active ? Theme.accent : Theme.textSoft
        anchors.verticalCenter: parent ? parent.verticalCenter : undefined
        height: 34
        width: row.implicitWidth + 28
        focusPolicy: Qt.NoFocus
        hoverEnabled: true
        background: Rectangle {
            radius: height / 2
            color: chip.filled ? Theme.accent : chip.hovered ? Theme.hover : Theme.surface
            border.width: chip.active && chip.outlined ? 1.5 : 0
            border.color: Theme.accent
        }
        contentItem: Item {
            Row {
                id: row
                anchors.centerIn: parent
                spacing: 8
                Kirigami.Icon {
                    visible: chip.iconName !== ""
                    anchors.verticalCenter: parent.verticalCenter
                    width: 14; height: 14
                    source: chip.iconName !== "" ? Theme.icon(chip.iconName) : ""
                    isMask: true
                    color: chip.ink
                }
                Text {
                    id: chipLabel
                    text: chip.label
                    color: chip.ink
                    font.family: Theme.font
                    font.pixelSize: 13
                    font.weight: Font.Medium
                }
                Text {
                    visible: chip.count > 0
                    anchors.baseline: chipLabel.baseline
                    text: chip.count
                    color: chip.filled ? Theme.accentInkSoft : Theme.text3
                    font.family: Theme.mono
                    font.pixelSize: 11
                }
            }
        }
    }
}
