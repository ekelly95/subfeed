import QtQuick
import QtQuick.Layouts
import QtQuick.Effects
import org.kde.kirigami as Kirigami

// The newest unwatched video, large, with a glow taken from its own frame; beside it,
// "Continue watching".
Item {
    id: hero

    property var video: null          // feed.heroJson, parsed
    property var continuing: []       // feed.continueJson, parsed
    property bool focused: false
    property string kicker: "Newest"

    signal play(var video)
    signal markWatched()
    signal hide()
    // The pointer is on a video (true) or left it (false), for prefetching.
    signal pointing(var video, bool on)

    implicitHeight: 304

    RowLayout {
        anchors.fill: parent
        spacing: 40

        Item {
            Layout.preferredWidth: 540
            Layout.fillHeight: true

            Image {
                id: frame
                anchors.fill: parent
                source: hero.video ? hero.video.hero : ""
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                visible: false
            }
            // The glow: the same frame, blurred and spilling past the edges.
            MultiEffect {
                source: frame
                x: -20; y: 24
                width: parent.width + 40
                height: parent.height + 6
                blurEnabled: true
                blur: 1.0
                blurMax: 64
                saturation: 0.5
                opacity: 0.6
                visible: frame.status === Image.Ready
            }
            Kirigami.ShadowedImage {
                anchors.fill: parent
                radius: Theme.radiusL
                color: Theme.surface
                source: frame.source
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
            }
            Rectangle {
                anchors.fill: parent
                anchors.margins: -5
                radius: Theme.radiusL + 5
                color: "transparent"
                border.width: 2
                border.color: Theme.accent
                visible: hero.focused
            }
            Rectangle {
                visible: hero.video && hero.video.duration !== ""
                anchors { right: parent.right; bottom: parent.bottom; margins: 12 }
                width: heroDur.implicitWidth + 14
                height: 22
                radius: 6
                color: Qt.rgba(0, 0, 0, 0.72)
                Text {
                    id: heroDur
                    anchors.centerIn: parent
                    text: hero.video ? hero.video.duration : ""
                    color: "#eceef1"    // on the dark badge over the picture, in either theme
                    font.family: Theme.mono
                    font.pixelSize: 12
                }
            }
            TapHandler { onTapped: hero.play(hero.video) }
            HoverHandler {
                cursorShape: Qt.PointingHandCursor
                onHoveredChanged: hero.pointing(hero.video, hovered)
            }
        }

        ColumnLayout {
            // Takes what's left: its long title would otherwise squeeze "Continue watching".
            Layout.fillWidth: true
            Layout.preferredWidth: 0
            Layout.alignment: Qt.AlignVCenter
            spacing: 14

            Text {
                text: (hero.kicker + " · " + (hero.video ? hero.video.age : "")).toUpperCase()
                color: Theme.accent
                font.family: Theme.font
                font.pixelSize: 12
                font.weight: Font.DemiBold
                font.letterSpacing: 1.4
            }
            Text {
                Layout.fillWidth: true
                text: hero.video ? hero.video.title : ""
                color: Theme.text
                font.family: Theme.font
                font.pixelSize: 34
                font.weight: Font.Bold
                font.letterSpacing: -0.8
                lineHeight: 1.1
                wrapMode: Text.Wrap
                maximumLineCount: 3
                elide: Text.ElideRight
            }
            Text {
                Layout.fillWidth: true
                text: hero.video ? hero.video.channelTitle : ""
                color: Theme.text2
                font.family: Theme.font
                font.pixelSize: 15
                elide: Text.ElideRight
            }
            RowLayout {
                Layout.topMargin: 10
                spacing: 10
                SfButton {
                    primary: true; iconName: "play"; text: "Play"; hint: "⏎"
                    onClicked: hero.play(hero.video)
                    onHoveredChanged: hero.pointing(hero.video, hovered)
                }
                SfButton { text: "Mark watched"; hint: "W"; onClicked: hero.markWatched() }
                SfButton { text: "Hide"; hint: "H"; onClicked: hero.hide() }
            }
        }

        ColumnLayout {
            // Only where it fits beside the video and its title.
            visible: hero.continuing.length > 0 && hero.width >= 1100
            Layout.preferredWidth: 300
            Layout.minimumWidth: 300
            Layout.maximumWidth: 300
            Layout.alignment: Qt.AlignTop
            spacing: 14

            Text {
                text: "CONTINUE WATCHING"
                color: Theme.text3
                font.family: Theme.font
                font.pixelSize: 12
                font.weight: Font.DemiBold
                font.letterSpacing: 1.4
            }
            Repeater {
                model: hero.continuing
                delegate: RowLayout {
                    id: item
                    required property var modelData
                    spacing: 12
                    Layout.fillWidth: true
                    Item {
                        Layout.preferredWidth: 128
                        Layout.preferredHeight: 72
                        Kirigami.ShadowedImage {
                            anchors.fill: parent
                            radius: 10
                            color: Theme.surface
                            source: item.modelData.thumbnail
                            fillMode: Image.PreserveAspectCrop
                            sourceSize.width: 256
                        }
                        Rectangle {
                            anchors { left: parent.left; right: parent.right; bottom: parent.bottom; leftMargin: 4; rightMargin: 4 }
                            height: 3
                            color: Qt.rgba(1, 1, 1, 0.18)
                            Rectangle { width: parent.width * item.modelData.progress; height: 3; color: Theme.accent }
                        }
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 4
                        Text {
                            Layout.fillWidth: true
                            text: item.modelData.title
                            color: Theme.text
                            font.family: Theme.font
                            font.pixelSize: 14
                            font.weight: Font.DemiBold
                            wrapMode: Text.Wrap
                            maximumLineCount: 2
                            elide: Text.ElideRight
                        }
                        Text {
                            Layout.fillWidth: true
                            text: Theme.meta([item.modelData.channelTitle, item.modelData.left !== "" ? item.modelData.left + " left" : ""])
                            color: Theme.text2
                            font.family: Theme.font
                            font.pixelSize: 12
                            elide: Text.ElideRight
                        }
                    }
                    TapHandler { onTapped: hero.play(item.modelData) }
                    HoverHandler {
                        cursorShape: Qt.PointingHandCursor
                        onHoveredChanged: hero.pointing(item.modelData, hovered)
                    }
                }
            }
            Item { Layout.fillHeight: true }
        }
    }
}
