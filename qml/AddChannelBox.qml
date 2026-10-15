import QtQuick
import QtQuick.Layouts
import QtQuick.Templates as T
import org.kde.kirigami as Kirigami
import dev.subfeed.core

// Follow a channel by @handle, name or URL (looked up with yt-dlp), with what was added so far.
// Used on the first-run page and the Channels page.
ColumnLayout {
    id: box

    // Titles of channels followed here, newest first.
    readonly property var added: JSON.parse(setup.addedJson)
    signal channelAdded()

    function focusField() { field.forceActiveFocus() }

    spacing: 14

    Setup {
        id: setup
        onAddedJsonChanged: { field.text = ""; box.channelAdded() }
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: 10
        T.TextField {
            id: field
            Layout.fillWidth: true
            implicitHeight: 40
            leftPadding: 14
            rightPadding: 14
            color: Theme.text
            font.family: Theme.font
            font.pixelSize: 14
            verticalAlignment: TextInput.AlignVCenter
            selectByMouse: true
            selectionColor: Theme.accent
            selectedTextColor: Theme.accentInk
            enabled: !setup.busy
            focus: true
            background: Rectangle {
                radius: Theme.radiusM
                color: Theme.raised
                border.width: field.activeFocus ? 1 : 0
                border.color: Theme.accent
                Text {
                    visible: field.text === "" && !field.preeditText
                    text: "@handle or youtube.com/…"
                    color: Theme.placeholder
                    font: field.font
                    anchors { left: parent.left; leftMargin: field.leftPadding; verticalCenter: parent.verticalCenter }
                }
            }
            Keys.onReturnPressed: setup.addChannel(text)
            Keys.onEnterPressed: setup.addChannel(text)
        }
        SfButton {
            text: setup.busy ? "Finding…" : "Add"
            size: 40
            enabled: !setup.busy && field.text.trim() !== ""
            onClicked: setup.addChannel(field.text)
        }
    }
    Text {
        Layout.fillWidth: true
        visible: setup.error !== ""
        text: setup.error
        color: Theme.warning
        font.family: Theme.font
        font.pixelSize: 13
        wrapMode: Text.Wrap
    }
    Flow {
        Layout.fillWidth: true
        visible: box.added.length > 0
        spacing: 8
        Repeater {
            model: box.added
            delegate: Rectangle {
                required property string modelData
                height: 30
                width: addedRow.implicitWidth + 22
                radius: 15
                color: Theme.raised
                Row {
                    id: addedRow
                    anchors.centerIn: parent
                    spacing: 6
                    Kirigami.Icon {
                        source: Theme.icon("check")
                        isMask: true
                        color: Theme.accent
                        width: 14; height: 14
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    Text {
                        text: modelData
                        color: Theme.text
                        font.family: Theme.font
                        font.pixelSize: 13
                    }
                }
            }
        }
    }
}
