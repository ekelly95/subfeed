import QtQuick
import QtQuick.Layouts
import QtQuick.Templates as T
import QtQuick.Dialogs
import QtCore
import org.kde.kirigami as Kirigami
import dev.subfeed.core

// First run (no channels yet): import a Google Takeout export, or add channels by hand.
// A Takeout file can also be dropped anywhere on the window.
FocusScope {
    id: page

    // Channels are in: show the feed. `imported` is how many the file listed (0 when added by hand).
    signal done(int imported)

    readonly property bool wide: width >= 1040

    // For the Takeout import; adding channels has its own (AddChannelBox).
    Setup { id: setup }

    function importFile(url) {
        const path = decodeURIComponent(url.toString().replace(/^file:\/\//, ""))
        const count = setup.importTakeout(path)
        if (count > 0) done(count)
    }

    Rectangle { anchors.fill: parent; color: Theme.bg }

    Flickable {
        anchors.fill: parent
        contentWidth: width
        contentHeight: column.implicitHeight + 120
        boundsBehavior: Flickable.StopAtBounds
        clip: true

        ColumnLayout {
            id: column
            width: Math.min(page.wide ? 1000 : 560, parent.width - 2 * Theme.gutter)
            x: (parent.width - width) / 2
            y: Math.max(48, (page.height - implicitHeight) / 2 - 20)
            spacing: 0

            Row {
                spacing: 12
                Rectangle { width: 14; height: 14; radius: 4; color: Theme.accent; anchors.verticalCenter: parent.verticalCenter }
                Text {
                    text: "subfeed"
                    color: Theme.text
                    font.family: Theme.font
                    font.pixelSize: 24
                    font.weight: Font.Bold
                    font.letterSpacing: -0.5
                }
            }
            Text {
                Layout.topMargin: 28
                Layout.fillWidth: true
                text: "Your subscriptions, and nothing else."
                color: Theme.text
                font.family: Theme.font
                font.pixelSize: 34
                font.weight: Font.DemiBold
                font.letterSpacing: -0.8
                wrapMode: Text.Wrap
            }
            Text {
                Layout.topMargin: 10
                Layout.fillWidth: true
                text: "The channels you follow, newest first. No Google account, no recommendations, no autoplay. Start by bringing in your channels."
                color: Theme.text2
                font.family: Theme.font
                font.pixelSize: 16
                lineHeight: 1.2
                wrapMode: Text.Wrap
            }

            GridLayout {
                Layout.topMargin: 36
                Layout.fillWidth: true
                columns: page.wide ? 2 : 1
                columnSpacing: 20
                rowSpacing: 20

                // Import from Google Takeout.
                Card {
                    Layout.fillWidth: true
                    Layout.preferredWidth: 3
                    Layout.alignment: Qt.AlignTop
                    title: "Import from YouTube"
                    subtitle: "Google Takeout exports the channels you subscribe to. It takes a couple of minutes."

                    Repeater {
                        model: [
                            "Open Google Takeout and sign in to your YouTube account.",
                            "Under “All YouTube data included”, keep only subscriptions. Export, then download the file.",
                            "Unzip it and choose subscriptions.csv (in YouTube and YouTube Music → subscriptions).",
                        ]
                        delegate: RowLayout {
                            required property string modelData
                            required property int index
                            Layout.fillWidth: true
                            spacing: 14
                            Rectangle {
                                Layout.alignment: Qt.AlignTop
                                width: 24; height: 24; radius: 12
                                color: Theme.raised
                                Text {
                                    anchors.centerIn: parent
                                    text: index + 1
                                    color: Theme.text2
                                    font.family: Theme.mono
                                    font.pixelSize: 12
                                }
                            }
                            Text {
                                Layout.fillWidth: true
                                text: modelData
                                color: Theme.textSoft
                                font.family: Theme.font
                                font.pixelSize: 14
                                lineHeight: 1.2
                                wrapMode: Text.Wrap
                            }
                        }
                    }

                    Flow {
                        Layout.fillWidth: true
                        Layout.topMargin: 6
                        spacing: 10
                        SfButton {
                            text: "Open Google Takeout"
                            size: 40
                            onClicked: Qt.openUrlExternally("https://takeout.google.com/settings/takeout/custom/youtube")
                        }
                        SfButton {
                            primary: true
                            text: "Choose subscriptions.csv…"
                            size: 40
                            onClicked: fileDialog.open()
                        }
                    }
                    Text {
                        Layout.fillWidth: true
                        text: setup.error !== "" ? setup.error : "Or drop the file anywhere on this window."
                        color: setup.error !== "" ? Theme.warning : Theme.text3
                        font.family: Theme.font
                        font.pixelSize: 13
                        wrapMode: Text.Wrap
                    }
                }

                // Add channels one by one.
                Card {
                    Layout.fillWidth: true
                    Layout.preferredWidth: 2
                    Layout.alignment: Qt.AlignTop
                    title: "Or add channels yourself"
                    subtitle: "Type a channel’s @handle or paste its URL."

                    AddChannelBox {
                        id: addBox
                        Layout.fillWidth: true
                    }
                    SfButton {
                        visible: addBox.added.length > 0
                        primary: true
                        text: "Show my feed"
                        size: 40
                        onClicked: page.done(0)
                    }
                }
            }
        }
    }

    // Drop a Takeout file anywhere.
    DropArea {
        id: drop
        anchors.fill: parent
        onDropped: (event) => { if (event.hasUrls) page.importFile(event.urls[0]) }
    }
    Rectangle {
        anchors.fill: parent
        anchors.margins: 12
        visible: drop.containsDrag
        radius: Theme.radiusL
        color: Qt.rgba(0, 0, 0, 0.55)
        border.width: 2
        border.color: Theme.accent
        Text {
            anchors.centerIn: parent
            text: "Drop subscriptions.csv to import"
            color: Theme.text
            font.family: Theme.font
            font.pixelSize: 20
            font.weight: Font.DemiBold
        }
    }

    FileDialog {
        id: fileDialog
        title: "Choose subscriptions.csv"
        nameFilters: ["Takeout subscriptions (*.csv)", "All files (*)"]
        currentFolder: StandardPaths.standardLocations(StandardPaths.DownloadLocation)[0]
        onAccepted: page.importFile(selectedFile)
    }

    component Card: Rectangle {
        id: card
        property string title
        property string subtitle
        default property alias content: body.data
        implicitHeight: body.implicitHeight + 48
        radius: Theme.radiusL
        color: Theme.surface
        border.color: Theme.hairline
        ColumnLayout {
            id: body
            anchors { left: parent.left; right: parent.right; top: parent.top; margins: 24 }
            spacing: 14
            Text {
                Layout.fillWidth: true
                text: card.title
                color: Theme.text
                font.family: Theme.font
                font.pixelSize: 18
                font.weight: Font.DemiBold
            }
            Text {
                Layout.fillWidth: true
                Layout.topMargin: -6
                text: card.subtitle
                color: Theme.text2
                font.family: Theme.font
                font.pixelSize: 14
                lineHeight: 1.2
                wrapMode: Text.Wrap
            }
        }
    }
}
