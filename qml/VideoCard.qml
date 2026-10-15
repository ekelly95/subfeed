import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// One video in the grid: rounded 16:9 thumbnail with duration, then title and "Channel · age".
// Shorts get a tall 9:16 thumbnail instead.
Item {
    id: card

    required property int index
    required property string videoId
    required property string title
    required property string channelTitle
    required property string channelId
    required property string age
    required property string duration
    required property string thumbnail
    required property bool watched
    required property real progress
    required property bool isShort

    property bool focused: false
    property int gap: 24

    signal activated()
    signal selected()
    signal contextMenuRequested()
    // The pointer came onto the card (true) or left it (false), for prefetching.
    signal pointing(bool on)

    ColumnLayout {
        anchors.fill: parent
        anchors.rightMargin: card.gap
        anchors.bottomMargin: 28
        spacing: 0
        opacity: card.watched ? 0.45 : 1

        Item {
            Layout.fillWidth: true
            Layout.preferredHeight: card.isShort ? width * 16 / 9 : width * 9 / 16

            Kirigami.ShadowedImage {
                id: thumb
                anchors.fill: parent
                radius: Theme.radiusM
                color: Theme.surface
                // hqdefault is 4:3 with letterbox bars; cropping to 16:9 removes them. A Short's
                // is its vertical frame between side bars, which cropping to 9:16 leaves exactly.
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                sourceSize.width: 480
                source: card.thumbnail
                scale: hover.hovered ? 1.015 : 1
                Behavior on scale { NumberAnimation { duration: 140 } }
            }
            Rectangle {
                anchors.fill: parent
                anchors.margins: -4
                radius: Theme.radiusM + 4
                color: "transparent"
                border.width: 2
                border.color: Theme.accent
                visible: card.focused
            }
            Rectangle {
                visible: card.duration !== ""
                anchors { right: parent.right; bottom: parent.bottom; margins: 8 }
                width: dur.implicitWidth + 12
                height: 20
                radius: 6
                color: Qt.rgba(0, 0, 0, 0.72)
                Text {
                    id: dur
                    anchors.centerIn: parent
                    text: card.duration
                    color: "#eceef1"    // on the dark badge over the picture, in either theme
                    font.family: Theme.mono
                    font.pixelSize: 11
                }
            }
            Rectangle {
                visible: card.progress > 0.01 && !card.watched
                anchors { left: parent.left; right: parent.right; bottom: parent.bottom; leftMargin: 6; rightMargin: 6 }
                height: 3
                radius: 2
                color: Qt.rgba(1, 1, 1, 0.18)
                Rectangle { width: parent.width * card.progress; height: 3; radius: 2; color: Theme.accent }
            }
        }

        Text {
            Layout.fillWidth: true
            Layout.topMargin: 12
            text: card.title
            color: Theme.text
            font.family: Theme.font
            font.pixelSize: 15
            font.weight: Font.DemiBold
            lineHeight: 1.15
            wrapMode: Text.Wrap
            maximumLineCount: 2
            elide: Text.ElideRight
        }
        Text {
            Layout.fillWidth: true
            Layout.topMargin: 6
            text: (card.watched ? "Watched · " : "") + card.channelTitle + " · " + card.age
            color: Theme.text2
            font.family: Theme.font
            font.pixelSize: 13
            elide: Text.ElideRight
        }
        Item { Layout.fillHeight: true }
    }

    HoverHandler {
        id: hover
        cursorShape: Qt.PointingHandCursor
        onHoveredChanged: card.pointing(hovered)
    }
    // One click plays, like the large video at the top.
    TapHandler {
        acceptedButtons: Qt.LeftButton
        onTapped: { card.selected(); card.activated() }
    }
    TapHandler {
        acceptedButtons: Qt.RightButton
        onTapped: { card.selected(); card.contextMenuRequested() }
    }
}
