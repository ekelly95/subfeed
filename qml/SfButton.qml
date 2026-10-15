import QtQuick
import QtQuick.Layouts
import QtQuick.Templates as T
import org.kde.kirigami as Kirigami

// A rounded button: `primary` fills with the accent; `hint` shows its key, like "W".
T.Button {
    id: control

    property bool primary: false
    property string iconName: ""
    property string hint: ""
    property int size: 44
    property color tone: Theme.raised

    implicitHeight: size
    implicitWidth: Math.max(size, row.implicitWidth + (text !== "" ? 32 : 0))
    font.family: Theme.font
    font.pixelSize: 14
    font.weight: primary ? Font.DemiBold : Font.Medium
    focusPolicy: Qt.NoFocus
    hoverEnabled: true

    background: Rectangle {
        radius: Theme.radiusM
        color: control.primary ? (control.down ? Qt.darker(Theme.accent, 1.15) : control.hovered ? Qt.lighter(Theme.accent, 1.08) : Theme.accent)
                               : (control.down ? Theme.surface : control.hovered ? Theme.hover : control.tone)
    }

    contentItem: Item {
        RowLayout {
            id: row
            anchors.centerIn: parent
            spacing: 8
            Kirigami.Icon {
                visible: control.iconName !== ""
                source: Theme.icon(control.iconName)
                isMask: true
                color: label.color
                Layout.preferredWidth: 16
                Layout.preferredHeight: 16
            }
            Text {
                id: label
                visible: control.text !== ""
                text: control.text
                font: control.font
                color: control.primary ? Theme.accentInk : Theme.textSoft
            }
            Kbd {
                visible: control.hint !== ""
                text: control.hint
                plain: true
                color: control.primary ? Theme.accentInkSoft : Theme.text3
            }
        }
    }
}
