import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2

// The channels you follow: click one to see its videos, add more, or remove one (asks first,
// since its videos and your watch history for them go too). Esc goes back to the feed.
FocusScope {
    id: page

    required property var channels      // ChannelModel

    // Back to the feed: whether channels were followed here (so the feed refreshes), the ids
    // removed here, and the channel to show ("" for the whole feed).
    signal back(bool added, var removed, string channelId, string title)

    property bool addedAny: false
    property var removed: []
    function leave() { back(addedAny, removed, "", "") }

    Keys.onEscapePressed: page.leave()
    Component.onCompleted: addBox.focusField()

    Rectangle { anchors.fill: parent; color: Theme.bg }

    ColumnLayout {
        anchors.fill: parent
        anchors.leftMargin: Theme.gutter
        anchors.rightMargin: Theme.gutter
        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: 64
            spacing: 20
            SfButton {
                iconName: "chevron-left"
                text: "Feed"
                hint: "Esc"
                size: 40
                tone: Theme.surface
                onClicked: page.leave()
            }
            Text {
                text: "Channels"
                color: Theme.text
                font.family: Theme.font
                font.pixelSize: 20
                font.weight: Font.DemiBold
            }
            Text {
                Layout.leftMargin: -8
                text: list.count + (list.count === 1 ? " channel" : " channels")
                color: Theme.text3
                font.family: Theme.font
                font.pixelSize: 13
            }
            Item { Layout.fillWidth: true }
        }

        Text {
            Layout.topMargin: 12
            text: "Follow another channel"
            color: Theme.text2
            font.family: Theme.font
            font.pixelSize: 13
            font.weight: Font.Medium
        }
        AddChannelBox {
            id: addBox
            Layout.topMargin: 10
            Layout.maximumWidth: 560
            Layout.fillWidth: true
            onChannelAdded: { page.addedAny = true; page.channels.reload() }
        }

        ListView {
            id: list
            Layout.topMargin: 28
            Layout.fillWidth: true
            Layout.fillHeight: true
            model: page.channels
            spacing: 8
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            bottomMargin: 24
            QQC2.ScrollBar.vertical: QQC2.ScrollBar {}

            delegate: Rectangle {
                id: row
                required property string channelId
                required property string title
                required property int videoCount
                required property string latest
                required property string problem
                property bool confirming: false

                width: ListView.view.width
                height: 64
                radius: Theme.radiusM
                color: rowHover.hovered && !row.confirming ? Theme.raised : Theme.surface

                // Click a channel: its videos in the feed. (The buttons take their own clicks.)
                HoverHandler { id: rowHover; cursorShape: row.confirming ? Qt.ArrowCursor : Qt.PointingHandCursor }
                TapHandler {
                    enabled: !row.confirming
                    onTapped: page.back(page.addedAny, page.removed, row.channelId, row.title)
                }

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 20
                    anchors.rightMargin: 12
                    spacing: 12

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 3
                        Text {
                            Layout.fillWidth: true
                            text: row.title !== "" ? row.title : row.channelId
                            color: Theme.text
                            font.family: Theme.font
                            font.pixelSize: 15
                            font.weight: Font.DemiBold
                            elide: Text.ElideRight
                        }
                        Text {
                            Layout.fillWidth: true
                            text: row.confirming
                                ? "Its " + row.videoCount + (row.videoCount === 1 ? " video" : " videos") + " and your watch history for them go too."
                                : row.problem !== "" ? "Can’t refresh lately: " + row.problem
                                : row.videoCount === 0 ? "No videos yet; they arrive with the next refresh"
                                : row.videoCount + (row.videoCount === 1 ? " video" : " videos") + " · newest " + row.latest
                            color: row.confirming || row.problem !== "" ? Theme.warning : Theme.text2
                            font.family: Theme.font
                            font.pixelSize: 13
                            elide: Text.ElideRight
                        }
                    }

                    SfButton {
                        visible: !row.confirming
                        text: "Remove"
                        size: 36
                        tone: Theme.raised
                        onClicked: row.confirming = true
                    }
                    SfButton {
                        visible: row.confirming
                        text: "Cancel"
                        size: 36
                        tone: Theme.raised
                        onClicked: row.confirming = false
                    }
                    SfButton {
                        visible: row.confirming
                        primary: true
                        text: "Remove " + (row.title !== "" ? row.title : "channel")
                        size: 36
                        onClicked: {
                            page.removed = page.removed.concat([row.channelId])
                            page.channels.remove(row.channelId)
                        }
                    }
                }
            }
        }
    }
}
